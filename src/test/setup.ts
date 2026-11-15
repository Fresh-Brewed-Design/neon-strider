import "@testing-library/jest-dom";
import { vi } from "vitest";

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
