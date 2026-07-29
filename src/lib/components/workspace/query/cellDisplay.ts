export function cellDisplay(v: unknown): string {
  if (v === null || v === undefined) return "NULL";
  if (typeof v === "object") {
    try {
      return JSON.stringify(v);
    } catch {
      return String(v);
    }
  }
  return String(v);
}
