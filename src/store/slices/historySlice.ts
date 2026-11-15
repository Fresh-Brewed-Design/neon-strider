import type { StateCreator } from "zustand";
import type { HistorySlice, TreadmillStore } from "../types";

export const createHistorySlice: StateCreator<TreadmillStore, [], [], HistorySlice> = (set) => ({
  historyRefreshTrigger: 0,
  sessionMergeThresholdMinutes: 10,

  triggerHistoryRefresh: () => set((s) => ({ historyRefreshTrigger: s.historyRefreshTrigger + 1 })),
  setSessionMergeThreshold: (minutes) => set({ sessionMergeThresholdMinutes: minutes }),
});
