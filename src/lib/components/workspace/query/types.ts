import type { QueryResult } from "$lib/api/engines";
import type { QueryMode } from "$lib/utils/queryPolicyFe";

/** Default page size for query results table (UI only; fetch limit is separate). */
export const RESULT_PAGE_SIZE_DEFAULT = 5000;

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
