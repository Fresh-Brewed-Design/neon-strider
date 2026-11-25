import "@testing-library/jest-dom";
import { configure } from "@testing-library/react";
import { vi } from "vitest";

// findBy*/waitFor poll the real condition and return the moment it holds, so a
// long ceiling costs nothing when it passes. The 1 s default fails on a PC
// busy with other builds (#343).
configure({ asyncUtilTimeout: 10_000 });

// Tauri runtime isn't available under jsdom; stub the APIs the services import
// so constructing them in tests doesn't blow up on transformCallback/__TAURI_INTERNALS__.
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({ label: "main" })),
}));
