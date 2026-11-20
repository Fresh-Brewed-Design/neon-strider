import { useEffect, useRef, useState, type ComponentType } from "react";
import { Activity, History, LayoutDashboard, ListChecks, Pin, PinOff, Settings2 } from "lucide-react";
import { toast, Toaster } from "sonner";
import { subscribeAppSnapshot, useAppState } from "../state/useAppState";
import { ConfirmDialog } from "./ConfirmDialog";
import { GuidedPinMarker } from "./guided/GuidedPinMarker";
import { LinkControl } from "./link/LinkControl";
import { readPref, writePref } from "./localPrefs";
import { cx, send } from "./util";
import { LivePage } from "./pages/LivePage";
import { ProgramsPage } from "./pages/ProgramsPage";
import { HistoryPage } from "./pages/HistoryPage";
import { HudPage } from "./pages/HudPage";
import { SystemPage } from "./pages/SystemPage";
import { CalibrateOffer } from "./system/calibration/CalibrateOffer";
import { UpdateDot } from "./system/updates/UpdateDot";
import { showUpdateBadge } from "./system/updates/updates";
import { WhatsNewDialog } from "./system/updates/WhatsNewDialog";

type PageId = "live" | "programs" | "history" | "hud" | "system";

const PAGES: { id: PageId; label: string; Icon: ComponentType<{ size?: number }>; Page: ComponentType }[] = [
  { id: "live", label: "Live", Icon: Activity, Page: LivePage },
  { id: "programs", label: "Programs", Icon: ListChecks, Page: ProgramsPage },
  { id: "history", label: "History", Icon: History, Page: HistoryPage },
  { id: "hud", label: "HUD", Icon: LayoutDashboard, Page: HudPage },
  { id: "system", label: "System", Icon: Settings2, Page: SystemPage },
];

const PAGE_KEY = "dashboard.page";
/** The last HUD "open the notes" request this dashboard followed (`update.show_notes_at_ms`). */
const NOTES_SHOWN_KEY = "dashboard.updateNotesShownAt";
const UPDATES_SECTION_ID = "sys-updates";
const CALIBRATION_SECTION_ID = "sys-calibration";

function readPage(): PageId {
  const saved = readPref(PAGE_KEY);
  return PAGES.find((p) => p.id === saved)?.id ?? "live";
}

/**
 * Raises a toast for results that land while their button may be off screen
 * (auto-sync failing in the background). The inline message stays where the
 * button is; this is the supplement.
 */
function BackgroundToasts() {
  const result = useAppState((s) => s.strava.last_result);
  const seen = useRef<number | null | undefined>(undefined);
  useEffect(() => {
    const at = result?.at_ms ?? null;
    if (seen.current === undefined) {
      seen.current = at;
      return;
    }
    if (at === seen.current) return;
    seen.current = at;
    if (result && !result.ok) toast.error(result.message, { id: "strava-result", duration: 8000 });
  }, [result]);
  return null;
}

/** The tray popup: a nav rail on the left, the chosen page on the right. */
export function DashboardApp() {
  const pinned = useAppState((s) => s.hud.dashboard_pinned) ?? false;
  const [page, setPage] = useState<PageId>(readPage);
  const Current = PAGES.find((p) => p.id === page)?.Page ?? LivePage;

  const updateBadge = useAppState((s) => showUpdateBadge(s.update)) ?? false;

  const openPage = (id: PageId) => {
    setPage(id);
    writePref(PAGE_KEY, id);
  };

  // INSTALL on the HUD's update card: open System -> Updates on the notes, once per request.
  useEffect(
    () =>
      subscribeAppSnapshot((s) => {
        const at = s.update.show_notes_at_ms;
        if (at === null || String(at) === readPref(NOTES_SHOWN_KEY)) return;
        writePref(NOTES_SHOWN_KEY, String(at));
        writePref(PAGE_KEY, "system");
        setPage("system");
        // 80 ms: a frame or so for the System page to render before scrolling to it.
        setTimeout(() => document.getElementById(UPDATES_SECTION_ID)?.scrollIntoView({ block: "start" }), 80);
      }),
    [],
  );

  return (
    <div className="relative flex h-screen w-screen overflow-hidden bg-[#050505] text-white select-none">
      <div className="crt-overlay" />
      <nav className="flex w-40 shrink-0 flex-col border-r border-neon-cyan/15 bg-black/50" aria-label="Sections">
        <div className="px-4 pb-4 pt-4" data-tauri-drag-region>
          <div className="font-digital text-sm font-black tracking-[0.25em] text-neon-cyan" data-tauri-drag-region>
            NEON
          </div>
          <div className="font-digital text-sm font-black tracking-[0.25em] text-neon-magenta" data-tauri-drag-region>
            STRIDER
          </div>
        </div>

        <ul className="flex flex-col gap-1 px-2">
          {PAGES.map(({ id, label, Icon }) => {
            const active = id === page;
            return (
              <li key={id}>
                <button
                  type="button"
                  aria-current={active ? "page" : undefined}
                  onClick={() => openPage(id)}
                  className={cx(
                    "relative flex w-full items-center gap-3 rounded px-3 py-2.5 text-left transition-colors",
                    active ? "bg-neon-cyan/10 text-neon-cyan" : "text-white/55 hover:bg-white/5 hover:text-white",
                  )}
                >
                  {active && <span className="absolute inset-y-1.5 left-0 w-0.5 rounded bg-neon-cyan shadow-[0_0_8px_#00f3ff]" />}
                  <Icon size={16} />
                  <span className="font-mono text-xs font-bold uppercase tracking-[0.2em]">{label}</span>
                  {id === "system" && updateBadge && <UpdateDot className="ml-auto" />}
                </button>
              </li>
            );
          })}
        </ul>

        <div className="mt-auto flex flex-col gap-1 border-t border-white/5 p-2">
          <GuidedPinMarker />
          <button
            type="button"
            onClick={() => send({ type: "dashboard_set_pinned", pinned: !pinned })}
            title={pinned ? "Close this window when it loses focus" : "Keep this window open when it loses focus"}
            className={cx(
              "flex w-full items-center gap-2 rounded px-2.5 py-2 text-left transition-colors hover:bg-white/5",
              pinned ? "text-neon-cyan" : "text-white/50",
            )}
          >
            {pinned ? <Pin size={14} /> : <PinOff size={14} />}
            <span className="font-mono text-[11px] font-bold uppercase tracking-[0.18em]">
              {pinned ? "Pinned" : "Pin open"}
            </span>
          </button>
          <LinkControl />
        </div>
      </nav>

      <main className="flex min-w-0 flex-1 flex-col">
        {page !== "system" && (
          <CalibrateOffer
            onCalibrate={() => {
              openPage("system");
              setTimeout(() => document.getElementById(CALIBRATION_SECTION_ID)?.scrollIntoView({ block: "start" }), 80);
            }}
          />
        )}
        <div className="min-h-0 flex-1">
          <Current />
        </div>
      </main>

      <Toaster position="bottom-right" theme="dark" expand={false} richColors />
      <BackgroundToasts />
      <WhatsNewDialog />
      <ConfirmDialog />
    </div>
  );
}
