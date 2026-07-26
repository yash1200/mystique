/**
 * Export ER canvas DOM to PNG image or PDF.
 */
import { toPng } from "html-to-image";
import { jsPDF } from "jspdf";

export type DiagramExportBounds = {
  width: number;
  height: number;
};

function safeFileBase(name: string): string {
  const base = (name || "diagram").trim() || "diagram";
  return base.replace(/[^\w.\-]+/g, "_").slice(0, 80);
}

function downloadDataUrl(dataUrl: string, filename: string) {
  const a = document.createElement("a");
  a.href = dataUrl;
  a.download = filename;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
}

function readCssBackground(el: HTMLElement): string {
  const bg = getComputedStyle(el).backgroundColor;
  if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") return bg;
  const root = getComputedStyle(document.documentElement);
  return root.getPropertyValue("--bg-app").trim() || "#0f1219";
}

/**
 * Capture a DOM node (ER canvas) as a PNG data URL.
 * Uses scrollWidth/Height so off-viewport tables are included.
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

  // Temporarily expand overflow so html-to-image sees full content
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
      // Filter out fixed overlays if any
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
): Promise<void> {
  const dataUrl = await captureDiagramPng(el, bounds);
  downloadDataUrl(dataUrl, `${safeFileBase(diagramName)}.png`);
}

/**
 * Export canvas as a single-page (or multi if tall) PDF embedding the PNG.
 */
export async function exportDiagramPdf(
  el: HTMLElement,
  diagramName: string,
  bounds?: DiagramExportBounds,
): Promise<void> {
  const dataUrl = await captureDiagramPng(el, bounds);

  // Load image to get natural pixel size
  const img = await new Promise<HTMLImageElement>((resolve, reject) => {
    const i = new Image();
    i.onload = () => resolve(i);
    i.onerror = () => reject(new Error("Failed to load export image"));
    i.src = dataUrl;
  });

  const pxW = img.naturalWidth || img.width;
  const pxH = img.naturalHeight || img.height;
  // Convert px → mm at ~96dpi (1in = 25.4mm, 96px = 1in)
  const mmW = (pxW * 25.4) / 96;
  const mmH = (pxH * 25.4) / 96;

  // Cap page to A3 landscape-ish; scale image to fit if huge
  const maxW = 420; // A3 width mm
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
  pdf.save(`${safeFileBase(diagramName)}.pdf`);
}
