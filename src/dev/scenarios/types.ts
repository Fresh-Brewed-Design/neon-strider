import type { Command } from "../../ipc";

/** What dispatched commands do to the snapshot, by command type. */
export type CommandHandlers = { [K in Command["type"]]?: (c: Extract<Command, { type: K }>) => void };

/** A named `invoke` call besides `dispatch`. */
export type Query = (args: Record<string, unknown>) => unknown;

/**
 * One feature's part of the mocked backend: the URL flags it reads and what
 * its commands and queries do. Every part is optional.
 */
export interface Scenario {
  /** Applies this feature's URL flags to the snapshot when the mock loads. */
  apply?: () => void;
  commands?: CommandHandlers;
  queries?: Record<string, Query>;
  /** Once a second, like Rust's countdowns; true when it changed the snapshot. */
  tick?: () => boolean;
  /** Runs once when a preview installs the mocked backend. */
  install?: () => void;
}
