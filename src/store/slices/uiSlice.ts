import type { StateCreator } from "zustand";
import type { TreadmillStore, UISlice } from "../types";

export const createUISlice: StateCreator<TreadmillStore, [], [], UISlice> = (set) => ({
  pendingConfirmation: null,

  confirm: (config) => set({ pendingConfirmation: config }),
  clearConfirmation: () => set({ pendingConfirmation: null }),

  programRequest: null,
  openProgram: (id) => set({ programRequest: id }),
  clearProgramRequest: () => set({ programRequest: null }),

  leaveGuard: null,
  setLeaveGuard: (guard) => set({ leaveGuard: guard }),
});
