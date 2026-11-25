import { useEffect, useRef, useState, type ComponentType } from "react";
import { Activity, History, LayoutDashboard, ListChecks, Pin, PinOff, Settings2 } from "lucide-react";
import { toast, Toaster } from "sonner";
import { subscribeAppSnapshot, useAppState } from "../state/useAppState";
import { useTreadmillStore } from "../store/treadmillStore";
import { ConfirmDialog } from "./ConfirmDialog";
import { GuidedPinMarker } from "./guided/GuidedPinMarker";
import { leavePage } from "./leavePage";
import { LinkControl } from "./link/LinkControl";
import { readPref, writePref } from "./localPrefs";
import { cx, send } from "./util";
import { LivePage } from "./pages/LivePage";
import { ProgramsPage } from "./pages/ProgramsPage";
import { HistoryPage } from "./pages/HistoryPage";
import { HudPage } from "./pages/HudPage";
import { SystemPage } from "./pages/SystemPage";
import { CalibrateOffer } from "./system/calibration/CalibrateOffer";
import { showSystemSection } from "./system/systemTabs";
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

  // ECHO's "Open in Programs" on a History row: show the Programs page, where the editor opens it.
  useEffect(
    () =>
      useTreadmillStore.subscribe((st, prev) => {
        if (st.programRequest === null || st.programRequest === prev.programRequest) return;
        writePref(PAGE_KEY, "programs");
        setPage("programs");
      }),
    [],
  );

  // INSTALL on the HUD's update card: open System, CORE, Updates on the notes, once per request.
  useEffect(
    () =>
      subscribeAppSnapshot((s) => {
        const at = s.update.show_notes_at_ms;
        if (at === null || String(at) === readPref(NOTES_SHOWN_KEY)) return;
        writePref(NOTES_SHOWN_KEY, String(at));
        leavePage(() => {
          writePref(PAGE_KEY, "system");
          setPage("system");
          showSystemSection("sys-updates");
        });
      }),
    [],
  );

  return (
    // fixed inset-0, not w-screen/h-screen: the window's own box under any zoom, so at a large
    // Windows text size the root (the container index.css sizes the layout by) is never wider.
    <div className="dash-root fixed inset-0 flex overflow-hidden bg-[#050505] text-white select-none">
      <div className="crt-overlay" />
      <nav className="dash-rail flex w-40 shrink-0 flex-col border-r border-neon-cyan/15 bg-black/50" aria-label="Sections">
        <div className="dash-rail-logo px-4 pb-4 pt-4" data-tauri-drag-region>
          <div className="font-digital text-sm font-black tracking-[0.25em] text-neon-cyan" data-tauri-drag-region>
            N<span className="dash-rail-text">EON</span>
          </div>
          <div className="font-digital text-sm font-black tracking-[0.25em] text-neon-magenta" data-tauri-drag-region>
            S<span className="dash-rail-text">TRIDER</span>
          </div>
        </div>

        <ul className="dash-rail-list flex flex-col gap-1 px-2">
          {PAGES.map(({ id, label, Icon }) => {
            const active = id === page;
            return (
              <li key={id}>
                <button
                  type="button"
                  aria-current={active ? "page" : undefined}
                  title={label}
                  onClick={() =>
                    leavePage(() => {
                      useTreadmillStore.getState().clearProgramRequest();
                      openPage(id);
                    })
                  }
                  className={cx(
                    "dash-rail-item relative flex w-full items-center gap-3 rounded px-3 py-2.5 text-left transition-colors",
                    active ? "bg-neon-cyan/10 text-neon-cyan" : "text-white/55 hover:bg-white/5 hover:text-white",
                  )}
                >
                  {active && <span className="absolute inset-y-1.5 left-0 w-0.5 rounded bg-neon-cyan shadow-[0_0_8px_#00f3ff]" />}
                  <Icon size={16} />
                  <span className="dash-rail-text font-mono text-xs font-bold uppercase tracking-[0.2em]">{label}</span>
                  {id === "system" && updateBadge && <UpdateDot className="dash-rail-dot ml-auto" />}
                </button>
              </li>
            );
          })}
        </ul>

        <div className="dash-rail-foot mt-auto flex flex-col gap-1 border-t border-white/5 p-2">
          <div className="dash-wide-only empty:hidden">
            <GuidedPinMarker />
          </div>
          <button
            type="button"
            onClick={() => send({ type: "dashboard_set_pinned", pinned: !pinned })}
            title={pinned ? "Close the dashboard when it loses focus" : "Keep the dashboard open when it loses focus"}
            className={cx(
              "dash-rail-item flex w-full items-center gap-2 rounded px-2.5 py-2 text-left transition-colors hover:bg-white/5",
              pinned ? "text-neon-cyan" : "text-white/60",
            )}
          >
            {pinned ? <Pin size={14} /> : <PinOff size={14} />}
            <span className="dash-rail-text font-mono text-[11px] font-bold uppercase tracking-[0.18em]">
              {pinned ? "Pinned" : "Pin open"}
            </span>
          </button>
          <LinkControl />
        </div>
      </nav>

      <main className="flex min-w-0 flex-1 flex-col">
        {/* With the rail at icons only, the guided task's marker sits over the page instead. */}
        <div className="dash-compact-only shrink-0 px-3 pt-2">
          <GuidedPinMarker />
        </div>
        {page !== "system" && (
          <CalibrateOffer
            onCalibrate={() =>
              leavePage(() => {
                openPage("system");
                showSystemSection("sys-calibration");
              })
            }
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
