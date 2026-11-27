// Auto-update. ?update=available: v1.3.0 is on offer (notes for it and the
// skipped v1.2.1; with ?autoinstall=off the HUD shows UPDATE READY, on it
// installs by itself and only the dot shows). ?update=downloading: it is
// downloading. ?update=failed: the last automatic check failed (offline).
// ?update=tomorrow: on offer after today's automatic install already ran.
// ?update=whatsnew: the first launch after updating to v1.2.0 ("What's new").
// ?autoinstall=off: "Install automatically" switched off.
import { later, params, snapshot } from "../mockState";
import type { Scenario } from "./types";

const PREVIEW_NOTES = [
  "## v1.3.0",
  "",
  "### New",
  "",
  "- **Auto-update**: Neon Strider checks GitHub when it starts and once a day, shows the release notes, and",
  "  installs and restarts with one click. The walk carries on.",
  "- `NEON STRIDER // WHAT'S NEW` after each update.",
  "",
  "### Fixed",
  "",
  "- The HUD's link label reconnects when clicked.",
  "",
  "## v1.2.1",
  "",
  "### Fixed",
  "",
  "- Tilt calibration applies at once, without a reconnect.",
].join("\n");

/** NOTES or LATER pressed on the HUD card for the offer (`update_hud_seen`). */
let hudSeen = false;

/** Rust's `update::show_hud_card`: never while it installs by itself. */
function hudCard(): boolean {
  const u = snapshot.update;
  return u.status.state === "available" && !u.dismissed && !hudSeen && !u.auto_install;
}

function offerUpdate(dismissed = false) {
  const offer = { version: "1.3.0", notes: PREVIEW_NOTES, published: "2026-11-20T09:30:00Z" };
  Object.assign(snapshot.update, {
    status: { state: "available", ...offer },
    offer,
    dismissed,
    dot: !dismissed,
  });
  snapshot.update.hud_card = hudCard();
}

/** Rust's download progress, then the installer (which would close the app). */
function simulateInstall() {
  snapshot.update.status = { state: "downloading", percent: 0 };
  const step = () =>
    later(400, () => {
      const s = snapshot.update.status;
      if (s.state !== "downloading") return;
      const percent = Math.min(100, (s.percent ?? 0) + 17);
      snapshot.update.status = percent >= 100 ? { state: "installing" } : { state: "downloading", percent };
      if (percent < 100) step();
    });
  step();
}

const updateFlag = params.get("update");

export const update: Scenario = {
  apply() {
    if (params.get("autoinstall") === "off") snapshot.update.auto_install = false;
    if (updateFlag === "available") offerUpdate();
    if (updateFlag === "tomorrow") {
      snapshot.update.auto_installed_today = true;
      offerUpdate();
    }
    if (updateFlag === "downloading") {
      offerUpdate();
      snapshot.update.hud_card = false;
      simulateInstall();
    }
    if (updateFlag === "failed") {
      snapshot.update.status = {
        state: "failed",
        message: "Can't reach GitHub. Check the internet connection, then try again.",
      };
    }
    if (updateFlag === "whatsnew") {
      snapshot.update.whats_new = {
        version: "1.2.0",
        notes: "## v1.2.0\n\n### New\n\n- **Walk-away safety**: locking the PC pauses the belt.\n- **Drag the HUD** by its NEON STRIDER logo.\n\n### Fixed\n\n- Engage now says when the treadmill's screen is off.",
      };
    }
  },
  commands: {
    update_check: () => {
      snapshot.update.status = { state: "checking" };
      later(1200, () => {
        const at_ms = Date.now();
        snapshot.update.last_checked_ms = at_ms;
        if (updateFlag === "failed") {
          const message = "Can't reach GitHub. Check the internet connection, then try again.";
          snapshot.update.status = { state: "failed", message };
          snapshot.update.last_result = { ok: false, message, at_ms };
        } else if (updateFlag === "available" || updateFlag === "tomorrow" || snapshot.update.offer) {
          offerUpdate(snapshot.update.dismissed);
          snapshot.update.last_result = { ok: true, message: "v1.3.0 is available.", at_ms };
        } else {
          snapshot.update.status = { state: "up_to_date" };
          snapshot.update.last_result = { ok: true, message: "Up to date: v1.2.0 is the newest.", at_ms };
        }
      });
    },
    update_install: () => {
      if (!snapshot.update.offer) throw "No update to install. Check for updates first.";
      simulateInstall();
    },
    update_dismiss: () => Object.assign(snapshot.update, { dismissed: true, hud_card: false }),
    update_hud_card: (c) => {
      hudSeen = true;
      snapshot.update.hud_card = false;
      if (c.open) snapshot.update.show_notes_at_ms = Date.now();
    },
    update_whats_new_ack: () => (snapshot.update.whats_new = null),
    update_auto_install: (c) => {
      snapshot.update.auto_install = c.on;
      snapshot.update.hud_card = hudCard();
    },
  },
};
