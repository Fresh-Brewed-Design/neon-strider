import { create } from "zustand";
import type { TreadmillStore } from "./types";
import { createHistorySlice } from "./slices/historySlice";
import { createSettingsSlice } from "./slices/settingsSlice";
import { createUISlice } from "./slices/uiSlice";

export const useTreadmillStore = create<TreadmillStore>()((...a) => ({
  ...createSettingsSlice(...a),
  ...createHistorySlice(...a),
  ...createUISlice(...a),
}));
