/**
 * Appearance + session prefs (localStorage). Applied via documentElement.
 */
export type ThemePref = "light" | "dark" | "system";
export type DensityPref = "compact" | "comfortable";

const THEME_KEY = "mystique-theme";
const DENSITY_KEY = "mystique-density";
const LAST_CONN_KEY = "mystique-last-connection-id";
const REMEMBER_KEY = "mystique-remember-last-connection";

export function readThemePref(): ThemePref {
  try {
    const raw = localStorage.getItem(THEME_KEY);
    if (raw === "light" || raw === "dark" || raw === "system") return raw;
  } catch {
    /* ignore */
  }
  return "dark";
}

export function writeThemePref(pref: ThemePref) {
  try {
    localStorage.setItem(THEME_KEY, pref);
  } catch {
    /* ignore */
  }
}

export function resolveTheme(pref: ThemePref): "light" | "dark" {
  if (pref === "system") {
    if (typeof window === "undefined") return "dark";
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return pref;
}

export function applyThemePref(pref: ThemePref) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = resolveTheme(pref);
  const bg = getComputedStyle(document.documentElement)
    .getPropertyValue("--bg-app")
    .trim();
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta && bg) meta.setAttribute("content", bg);
}

export function readDensityPref(): DensityPref {
  try {
    const raw = localStorage.getItem(DENSITY_KEY);
    if (raw === "comfortable" || raw === "compact") return raw;
  } catch {
    /* ignore */
  }
  return "compact";
}

export function writeDensityPref(pref: DensityPref) {
  try {
    localStorage.setItem(DENSITY_KEY, pref);
  } catch {
    /* ignore */
  }
}

export function applyDensityPref(pref: DensityPref) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.density = pref;
}

export function readLastConnectionId(): string | null {
  try {
    return localStorage.getItem(LAST_CONN_KEY);
  } catch {
    return null;
  }
}

export function writeLastConnectionId(id: string) {
  try {
    localStorage.setItem(LAST_CONN_KEY, id);
  } catch {
    /* ignore */
  }
}

export function readRememberLastConnection(): boolean {
  try {
    const raw = localStorage.getItem(REMEMBER_KEY);
    if (raw == null) return true;
    return raw === "1" || raw === "true";
  } catch {
    return true;
  }
}

export function writeRememberLastConnection(on: boolean) {
  try {
    localStorage.setItem(REMEMBER_KEY, on ? "1" : "0");
  } catch {
    /* ignore */
  }
}

/** Apply stored theme + density (call once at app boot). */
export function applyStoredAppearance() {
  applyThemePref(readThemePref());
  applyDensityPref(readDensityPref());
}

/** Compact 28px / comfortable 32px — keep in sync with `--grid-row-height`. */
export function gridRowHeightPx(): number {
  if (typeof document === "undefined") return 28;
  return document.documentElement.dataset.density === "comfortable" ? 32 : 28;
}

/** Watch `<html>` attribute changes (theme / density). */
export function watchHtmlAttr(
  name: string,
  cb: () => void,
): () => void {
  if (typeof document === "undefined") return () => {};
  cb();
  const mo = new MutationObserver(cb);
  mo.observe(document.documentElement, {
    attributes: true,
    attributeFilter: [name],
  });
  return () => mo.disconnect();
}
