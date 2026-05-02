import { createContext, use } from 'react';

export interface ContestSlotContextValue {
  /** Numeric id of the current contest. */
  contestId: number;
  /**
   * Contest type identifier (e.g. "ioi", "icpc"). Slot entries declared with
   * `contest_type = "x"` in plugin.toml are only rendered when this value
   * matches.
   */
  contestType: string | null;
}

/**
 * Context populated on contest-scoped pages. The Slot component uses this to
 * filter slot entries that declare a `contest_type`. When no provider is in
 * the tree (i.e. on non-contest pages), slots with a non-empty `contest_type`
 * are skipped entirely.
 */
export const ContestSlotContext = createContext<ContestSlotContextValue | null>(
  null,
);

export function useContestSlotContext(): ContestSlotContextValue | null {
  return use(ContestSlotContext);
}
