import type { QueryResult } from "$lib/api/engines";
import type { QueryMode } from "$lib/utils/queryPolicyFe";

/** Default page size for query results table (UI only; fetch limit is separate). */
export const RESULT_PAGE_SIZE_DEFAULT = 50_000;

export const RESULT_PAGE_SIZE_OPTIONS = [
  { value: "100", label: "100 / page" },
  { value: "500", label: "500 / page" },
  { value: "1000", label: "1,000 / page" },
  { value: "5000", label: "5,000 / page" },
  { value: "50000", label: "50,000 / page" },
];

/** Max rows requested from the engine for a single Run. */
export const QUERY_FETCH_MAX_ROWS = 50_000;

export type RunState =
  | { status: "idle" }
  | { status: "running" }
  | {
      status: "ok" | "blocked" | "error";
      message: string;
      mode: QueryMode;
      elapsedMs: number;
      result?: QueryResult | null;
    };
