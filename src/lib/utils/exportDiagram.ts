/**
 * Export ER canvas DOM to PNG image or PDF.
 * Always asks the user where to save (native save dialog in Tauri / File System
 * Access API in modern browsers; last-resort automatic download).
 */
import { toPng } from "html-to-image";
import { jsPDF } from "jspdf";
import { isTauri } from "$lib/api/tauri";

export type DiagramExportBounds = {
  width: number;
  height: number;
};

export type SaveResult =
  | { ok: true; path: string }
  | { ok: false; cancelled: true }
  | { ok: false; cancelled: false; error: string };

function safeFileBase(name: string): string {
  const base = (name || "diagram").trim() || "diagram";
  return base.replace(/[^\w.\-]+/g, "_").slice(0, 80);
}

function readCssBackground(el: HTMLElement): string {
  const bg = getComputedStyle(el).backgroundColor;
  if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") return bg;
  const root = getComputedStyle(document.documentElement);
  return root.getPropertyValue("--bg-app").trim() || "#0f1219";
}

function dataUrlToUint8Array(dataUrl: string): Uint8Array {
  const comma = dataUrl.indexOf(",");
  const b64 = comma >= 0 ? dataUrl.slice(comma + 1) : dataUrl;
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function bytesToArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy.buffer;
}

function downloadFallback(dataUrl: string, filename: string) {
  const a = document.createElement("a");
  a.href = dataUrl;
  a.download = filename;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
}

/**
 * Ask the user where to store a file, then write bytes there.
 */
async function saveBytesWithPrompt(
  bytes: Uint8Array,
  defaultName: string,
  filters: { name: string; extensions: string[] }[],
  mimeType: string,
): Promise<SaveResult> {
  // ── Tauri: native save dialog + write file ──────────────────────────
  if (isTauri()) {
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { writeFile } = await import("@tauri-apps/plugin-fs");
      const path = await save({
        title: "Save diagram export",
        defaultPath: defaultName,
        filters,
      });
      if (path == null) {
        return { ok: false, cancelled: true };
      }
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

  // ── Browser: File System Access API (Chrome/Edge) ───────────────────
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
        accept[mimeType] = f.extensions.map((ext) => `.${ext}`);
      }
      const handle = await w.showSaveFilePicker({
        suggestedName: defaultName,
        types: filters.map((f) => ({
          description: f.name,
          accept: {
            [mimeType]: f.extensions.map((ext) => `.${ext}`),
          },
        })),
      });
      const writable = await handle.createWritable();
      await writable.write(bytesToArrayBuffer(bytes));
      await writable.close();
      return { ok: true, path: handle.name };
    } catch (e) {
      // User cancel is AbortError
      if (e instanceof DOMException && e.name === "AbortError") {
        return { ok: false, cancelled: true };
      }
      // Fall through to download if picker fails for other reasons
    }
  }

  // ── Last resort: browser download (user's Downloads folder) ─────────
  const blob = new Blob([bytesToArrayBuffer(bytes)], { type: mimeType });
  const url = URL.createObjectURL(blob);
  try {
    downloadFallback(url, defaultName);
    return {
      ok: true,
      path: `Downloads/${defaultName} (browser default — pick a path when using the desktop app)`,
    };
  } finally {
    URL.revokeObjectURL(url);
  }
}

/**
 * Capture a DOM node (ER canvas) as a PNG data URL.
 */
export async function captureDiagramPng(
  el: HTMLElement,
  bounds?: DiagramExportBounds,
): Promise<string> {
  const width = Math.max(
    bounds?.width ?? 0,
    el.scrollWidth,
    el.clientWidth,
    800,
  );
  const height = Math.max(
    bounds?.height ?? 0,
    el.scrollHeight,
    el.clientHeight,
    600,
  );
  const backgroundColor = readCssBackground(el);

  const prev = {
    overflow: el.style.overflow,
    width: el.style.width,
    height: el.style.height,
  };
  el.style.overflow = "visible";
  el.style.width = `${width}px`;
  el.style.height = `${height}px`;

  try {
    return await toPng(el, {
      backgroundColor,
      width,
      height,
      pixelRatio: 2,
      cacheBust: true,
      filter: (node) => {
        if (!(node instanceof HTMLElement)) return true;
        if (node.dataset?.ctxMenu != null) return false;
        return true;
      },
    });
  } finally {
    el.style.overflow = prev.overflow;
    el.style.width = prev.width;
    el.style.height = prev.height;
  }
}

export async function exportDiagramPng(
  el: HTMLElement,
  diagramName: string,
  bounds?: DiagramExportBounds,
): Promise<SaveResult> {
  const dataUrl = await captureDiagramPng(el, bounds);
  const bytes = dataUrlToUint8Array(dataUrl);
  const defaultName = `${safeFileBase(diagramName)}.png`;
  return saveBytesWithPrompt(
    bytes,
    defaultName,
    [{ name: "PNG image", extensions: ["png"] }],
    "image/png",
  );
}

/**
 * Export canvas as PDF; prompts for save location.
 */
export async function exportDiagramPdf(
  el: HTMLElement,
  diagramName: string,
  bounds?: DiagramExportBounds,
): Promise<SaveResult> {
  const dataUrl = await captureDiagramPng(el, bounds);

  const img = await new Promise<HTMLImageElement>((resolve, reject) => {
    const i = new Image();
    i.onload = () => resolve(i);
    i.onerror = () => reject(new Error("Failed to load export image"));
    i.src = dataUrl;
  });

  const pxW = img.naturalWidth || img.width;
  const pxH = img.naturalHeight || img.height;
  const mmW = (pxW * 25.4) / 96;
  const mmH = (pxH * 25.4) / 96;

  const maxW = 420;
  const maxH = 297;
  let pageW = mmW;
  let pageH = mmH;
  const scale = Math.min(1, maxW / mmW, maxH / mmH);
  if (scale < 1) {
    pageW = mmW * scale;
    pageH = mmH * scale;
  }

  const orientation = pageW >= pageH ? "landscape" : "portrait";
  const pdf = new jsPDF({
    orientation,
    unit: "mm",
    format: [pageW, pageH],
    compress: true,
  });

  pdf.addImage(dataUrl, "PNG", 0, 0, pageW, pageH, undefined, "FAST");
  const ab = pdf.output("arraybuffer") as ArrayBuffer;
  const bytes = new Uint8Array(ab);
  const defaultName = `${safeFileBase(diagramName)}.pdf`;
  return saveBytesWithPrompt(
    bytes,
    defaultName,
    [{ name: "PDF document", extensions: ["pdf"] }],
    "application/pdf",
  );
}
