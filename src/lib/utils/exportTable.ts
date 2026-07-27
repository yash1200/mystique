/**
 * Export tabular result sets as CSV or PDF.
 * Uses native save dialog in Tauri; File System Access / download fallback in browser.
 */
import { jsPDF } from "jspdf";
import { isTauri } from "$lib/api/tauri";

export type TableExportColumn = { name: string };

export type SaveResult =
  | { ok: true; path: string }
  | { ok: false; cancelled: true }
  | { ok: false; cancelled: false; error: string };

function safeFileBase(name: string): string {
  const base = (name || "export").trim() || "export";
  return base.replace(/[^\w.\-]+/g, "_").slice(0, 80);
}

function cellToString(v: unknown): string {
  if (v === null || v === undefined) return "";
  if (typeof v === "string") return v;
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

/** RFC-style CSV cell escape. */
export function csvEscape(value: string): string {
  if (/[",\r\n]/.test(value)) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

/** Build a CSV string from column headers + row-major cells. */
export function buildCsv(
  columns: TableExportColumn[],
  rows: unknown[][],
): string {
  const header = columns.map((c) => csvEscape(c.name)).join(",");
  const body = rows
    .map((row) =>
      columns
        .map((_, i) => csvEscape(cellToString(row[i])))
        .join(","),
    )
    .join("\n");
  // BOM helps Excel open UTF-8 correctly
  return `\uFEFF${header}\n${body}\n`;
}

/** Object rows (DataGrid) → row-major cells. */
export function objectsToRows(
  columns: TableExportColumn[],
  objects: Record<string, unknown>[],
): unknown[][] {
  return objects.map((obj) => columns.map((c) => obj[c.name]));
}

function bytesToArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy.buffer;
}

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2_000);
}

async function saveBytesWithPrompt(
  bytes: Uint8Array,
  defaultName: string,
  filters: { name: string; extensions: string[] }[],
  mimeType: string,
): Promise<SaveResult> {
  if (isTauri()) {
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { writeFile } = await import("@tauri-apps/plugin-fs");
      const path = await save({
        title: "Export results",
        defaultPath: defaultName,
        filters,
      });
      if (path == null) return { ok: false, cancelled: true };
      await writeFile(path, bytes);
      return { ok: true, path };
    } catch (e) {
      return {
        ok: false,
        cancelled: false,
        error: e instanceof Error ? e.message : String(e),
      };
    }
  }

  const w = window as Window & {
    showSaveFilePicker?: (opts: {
      suggestedName?: string;
      types?: { description: string; accept: Record<string, string[]> }[];
    }) => Promise<FileSystemFileHandle>;
  };

  if (typeof w.showSaveFilePicker === "function") {
    try {
      const accept: Record<string, string[]> = {};
      for (const f of filters) {
        const mime =
          f.extensions.includes("csv")
            ? "text/csv"
            : f.extensions.includes("pdf")
              ? "application/pdf"
              : mimeType;
        accept[mime] = f.extensions.map((ext) => `.${ext}`);
      }
      const handle = await w.showSaveFilePicker({
        suggestedName: defaultName,
        types: [{ description: filters[0]?.name ?? "File", accept }],
      });
      const writable = await handle.createWritable();
      await writable.write(bytesToArrayBuffer(bytes));
      await writable.close();
      return { ok: true, path: handle.name };
    } catch (e) {
      const name = e instanceof Error ? e.name : "";
      if (name === "AbortError") return { ok: false, cancelled: true };
      // fall through to download
    }
  }

  try {
    downloadBlob(new Blob([bytesToArrayBuffer(bytes)], { type: mimeType }), defaultName);
    return { ok: true, path: defaultName };
  } catch (e) {
    return {
      ok: false,
      cancelled: false,
      error: e instanceof Error ? e.message : String(e),
    };
  }
}

export async function exportRowsAsCsv(
  columns: TableExportColumn[],
  rows: unknown[][],
  fileBase = "query-results",
): Promise<SaveResult> {
  const csv = buildCsv(columns, rows);
  const bytes = new TextEncoder().encode(csv);
  return saveBytesWithPrompt(
    bytes,
    `${safeFileBase(fileBase)}.csv`,
    [{ name: "CSV", extensions: ["csv"] }],
    "text/csv;charset=utf-8",
  );
}

/**
 * Simple multi-page landscape PDF table (text). Best for moderate column counts.
 */
export async function exportRowsAsPdf(
  columns: TableExportColumn[],
  rows: unknown[][],
  opts?: { title?: string; fileBase?: string },
): Promise<SaveResult> {
  const title = opts?.title ?? "Query results";
  const fileBase = opts?.fileBase ?? "query-results";
  const doc = new jsPDF({
    orientation: "landscape",
    unit: "pt",
    format: "a4",
  });

  const pageW = doc.internal.pageSize.getWidth();
  const pageH = doc.internal.pageSize.getHeight();
  const margin = 28;
  const usableW = pageW - margin * 2;
  const colCount = Math.max(1, columns.length);
  const colW = usableW / colCount;
  const rowH = 14;
  const fontSize = colCount > 10 ? 7 : colCount > 6 ? 8 : 9;
  const maxCellChars = Math.max(6, Math.floor(colW / (fontSize * 0.48)));

  function trunc(s: string): string {
    if (s.length <= maxCellChars) return s;
    return `${s.slice(0, Math.max(1, maxCellChars - 1))}…`;
  }

  let y = margin;
  doc.setFont("helvetica", "bold");
  doc.setFontSize(11);
  doc.text(title, margin, y);
  y += 16;
  doc.setFont("helvetica", "normal");
  doc.setFontSize(8);
  doc.setTextColor(100);
  doc.text(
    `${rows.length} row(s) · ${columns.length} column(s) · exported ${new Date().toISOString()}`,
    margin,
    y,
  );
  doc.setTextColor(0);
  y += 14;

  function drawHeader() {
    doc.setFont("helvetica", "bold");
    doc.setFontSize(fontSize);
    doc.setFillColor(240, 242, 245);
    doc.rect(margin, y - rowH + 4, usableW, rowH, "F");
    columns.forEach((c, i) => {
      const x = margin + i * colW + 2;
      doc.text(trunc(c.name), x, y, { maxWidth: colW - 4 });
    });
    y += rowH;
    doc.setFont("helvetica", "normal");
  }

  drawHeader();

  for (let ri = 0; ri < rows.length; ri++) {
    if (y + rowH > pageH - margin) {
      doc.addPage();
      y = margin;
      drawHeader();
    }
    if (ri % 2 === 1) {
      doc.setFillColor(248, 249, 251);
      doc.rect(margin, y - rowH + 4, usableW, rowH, "F");
    }
    const row = rows[ri] ?? [];
    columns.forEach((_, i) => {
      const x = margin + i * colW + 2;
      doc.text(trunc(cellToString(row[i])), x, y, { maxWidth: colW - 4 });
    });
    y += rowH;
  }

  const ab = doc.output("arraybuffer") as ArrayBuffer;
  const bytes = new Uint8Array(ab);
  return saveBytesWithPrompt(
    bytes,
    `${safeFileBase(fileBase)}.pdf`,
    [{ name: "PDF", extensions: ["pdf"] }],
    "application/pdf",
  );
}
