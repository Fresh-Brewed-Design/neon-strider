import { useSyncExternalStore } from "react";
import { getSnapshot, onAppState, type AppSnapshot } from "../ipc";
import { shareUnchanged } from "./shareUnchanged";

// One subscription per window. Rust owns the state; every window just mirrors
// the latest snapshot, so windows never need to talk to each other.
let current: AppSnapshot | null = null;
let started = false;
const listeners = new Set<() => void>();

function publish(next: AppSnapshot) {
  // Unchanged parts keep their reference, so a heartbeat only re-renders
  // what it changed; an identical snapshot notifies nobody.
  const shared = shareUnchanged(current, next);
  if (shared === current) return;
  current = shared;
  listeners.forEach((l) => l());
}

function start() {
  if (started) return;
  started = true;
  onAppState(publish).catch((e) => console.error("[state] subscribe failed", e));
  // The first event can be up to a heartbeat away; fetch once so the window
  // paints immediately. An event that lands first wins.
  getSnapshot()
    .then((s) => {
      if (!current) publish(s);
    })
    .catch((e) => console.error("[state] initial snapshot failed", e));
}

function subscribe(listener: () => void) {
  start();
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Latest app snapshot, or null until the first one arrives. */
export function useAppSnapshot(): AppSnapshot | null {
  return useSyncExternalStore(subscribe, () => current);
}

/**
 * Select a value from the snapshot; the component re-renders only when that
 * value changes. Objects and arrays keep their reference while their contents
 * stay the same, so selecting one is fine; a selector must not build a new
 * object, and a fallback for a null snapshot should be a module constant.
 */
export function useAppState<T>(selector: (s: AppSnapshot) => T): T | null {
  return useSyncExternalStore(subscribe, () => (current ? selector(current) : null));
}

/** Latest snapshot for non-React code, or null until the first one arrives. */
export function getAppSnapshot(): AppSnapshot | null {
  return current;
}

/** Call `cb` with every snapshot as it arrives. Returns the unsubscribe function. */
export function subscribeAppSnapshot(cb: (snapshot: AppSnapshot) => void): () => void {
  return subscribe(() => {
    if (current) cb(current);
  });
}
