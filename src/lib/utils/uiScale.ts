/**
 * UI scale — multiplies root font-size so rem-based design-language tokens scale together.
 *
 * Default is 2× (100% larger than the design-language base of 16px root / 13px body).
 */

export const UI_SCALE_STORAGE_KEY = "mystique-ui-scale";

/** Default: 100% larger than design-language base. */
export const DEFAULT_UI_SCALE = 2;

/** Stored preference: auto or a fixed multiplier. */
export type UiScalePreference = "auto" | number;

const MIN = 1;
const MAX = 2.5;

export function clampScale(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_UI_SCALE;
  return Math.min(MAX, Math.max(MIN, Math.round(n * 100) / 100));
}

/**
 * Auto scale — always at least 2× so UI is readable on large/low-DPI desktops.
 * Slightly higher on very wide screens.
 */
export function computeAutoScale(): number {
  if (typeof window === "undefined") return DEFAULT_UI_SCALE;

  const screenW = window.screen?.width ?? 0;
  const innerW = window.innerWidth ?? 0;
  const w = Math.max(screenW, innerW);

  if (w >= 3840) return 2.25;
  if (w >= 2560) return 2.1;
  return DEFAULT_UI_SCALE;
}

export function readScalePreference(): UiScalePreference {
  try {
    const raw = localStorage.getItem(UI_SCALE_STORAGE_KEY);
    if (raw == null || raw === "auto") return "auto";
    const n = Number(raw);
    if (Number.isFinite(n)) return clampScale(n);
  } catch {
    /* ignore */
  }
  return "auto";
}

export function writeScalePreference(pref: UiScalePreference) {
  try {
    localStorage.setItem(
      UI_SCALE_STORAGE_KEY,
      pref === "auto" ? "auto" : String(clampScale(pref)),
    );
  } catch {
    /* ignore */
  }
}

export function resolveScale(
  pref: UiScalePreference = readScalePreference(),
): number {
  if (pref === "auto") return clampScale(computeAutoScale());
  return clampScale(pref);
}

type ScaleListener = (scale: number) => void;
const listeners = new Set<ScaleListener>();

/** Subscribe to UI scale changes (e.g. ER canvas geometry). */
export function onUiScaleChange(listener: ScaleListener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Current scale from DOM (dataset or root font-size / 16). */
export function currentUiScale(): number {
  if (typeof document === "undefined") return DEFAULT_UI_SCALE;
  const fromData = Number(document.documentElement.dataset.uiScale);
  if (Number.isFinite(fromData) && fromData > 0) return clampScale(fromData);
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize);
  if (Number.isFinite(fs) && fs > 0) return clampScale(fs / 16);
  return DEFAULT_UI_SCALE;
}

/** Apply root rem base so entire design-language scale multiplies. */
export function applyUiScale(scale: number) {
  if (typeof document === "undefined") return;
  const s = clampScale(scale);
  // 16px * scale → rem units match design-language px at scale 1
  document.documentElement.style.fontSize = `${16 * s}px`;
  document.documentElement.dataset.uiScale = String(s);
  for (const listener of listeners) {
    try {
      listener(s);
    } catch {
      /* ignore */
    }
  }
}

export function applyPreferredUiScale() {
  applyUiScale(resolveScale());
}
