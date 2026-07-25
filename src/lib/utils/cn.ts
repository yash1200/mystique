/** Join class names; drop falsy. Accepts Svelte `ClassValue` and plain strings. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function cn(...parts: any[]): string {
  const out: string[] = [];

  const walk = (input: unknown) => {
    if (!input && input !== 0) return;
    if (typeof input === "string" || typeof input === "number") {
      const s = String(input).trim();
      if (s) out.push(s);
      return;
    }
    if (Array.isArray(input)) {
      for (const item of input) walk(item);
      return;
    }
    if (typeof input === "object") {
      for (const [key, on] of Object.entries(input as Record<string, unknown>)) {
        if (on) out.push(key);
      }
    }
  };

  for (const part of parts) walk(part);
  return out.join(" ");
}
