// Dev-only HUD preview (hud-preview.html): renders the HUD in a plain browser
// with a mocked backend, so its design can be worked on without a treadmill.
// Not part of the production build.
import { StrictMode } from "react";
import ReactDOM from "react-dom/client";
import { emit } from "@tauri-apps/api/event";
import { installMockBackend } from "../dev/mockBackend";

// Scenario flags (?anchor=, ?idle, ?offline, ?noprogram, ?program=, ?seated, ?due, ?weekly) are
// read by the mock; ?toasts, ?call and ?program=ready's toast are handled below.
const params = new URLSearchParams(location.search);

installMockBackend();

// A page background so the translucent panel reads as it would over a desktop.
const backdrop = document.createElement("div");
backdrop.style.cssText = "position:fixed;inset:0;z-index:-1;background:linear-gradient(135deg,#2b3a4a,#c9d3dd)";
document.body.prepend(backdrop);

const { HudApp } = await import("./HudApp");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <HudApp />
  </StrictMode>,
);

// ?program=ready: the one toast Rust sends when the rest ends.
if (params.get("program") === "ready") {
  setTimeout(() => emit("hud-update", { type: "AUTOMATION", message: "REST COMPLETE // PRESS GO", isActionable: false }), 300);
}

// ?call: the toast Rust sends when a call starts.
if (params.has("call")) {
  setTimeout(() => emit("hud-update", { type: "CALL", message: "CALL DETECTED // QUIET MODE" }), 300);
}

// ?lightsout: the toast Rust sends when the display turns off mid-walk.
if (params.has("lightsout")) {
  setTimeout(() => emit("hud-update", { type: "LIGHTS_OUT", message: "DISPLAY DARK // DECK POWERING DOWN" }), 300);
}

// ?toasts=deck: the program toast is the one for a start at the treadmill taken
// as GO (the longest seeded name; AUTOMATION has one slot, so it replaces the
// 30 s warning).
if (params.has("toasts")) {
  const deck = params.get("toasts") === "deck";
  setTimeout(() => {
    emit("hud-update", { type: "VELOCITY", message: "SPEED: 3.5", value: 3.5 });
    emit(
      "hud-update",
      deck
        ? { type: "AUTOMATION", message: "DECK START // POST-MEAL RESUMED", isActionable: false }
        : { type: "AUTOMATION", message: "WORKDAY IN 30S", isActionable: true },
    );
    emit("hud-update", { type: "STATUS", message: "SESSION SAVED · 1.72 KM · 32:14" });
  }, 300);
}
