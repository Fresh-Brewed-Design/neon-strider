import { useEffect, useRef, useState, type ReactNode } from "react";
import { useAppState } from "../../state/useAppState";
import { GuidedCalibration } from "../system/calibration/GuidedCalibration";
import { DeskSection } from "../system/DeskSection";
import { DiagnosticsSection } from "../system/DiagnosticsSection";
import { HeartRateSection } from "../system/HeartRateSection";
import { LabsSection } from "../system/LabsSection";
import { Section } from "../system/fields";
import { MaintenanceSection } from "../system/MaintenanceSection";
import { NeuralSection } from "../system/NeuralSection";
import { PermissionsSection } from "../system/PermissionsSection";
import { SessionsSection } from "../system/SessionsSection";
import { ShortcutsSection } from "../system/ShortcutsSection";
import { SpeedDriftSection } from "../system/SpeedDriftSection";
import { StartupSection } from "../system/StartupSection";
import { StravaSection } from "../system/strava/StravaSection";
import { SYSTEM_TABS, openSystemTab, readSystemTab, subscribeSystemTab, type SystemTab } from "../system/systemTabs";
import { TreadmillSection } from "../system/TreadmillSection";
import { UpdateDot } from "../system/updates/UpdateDot";
import { showUpdateBadge } from "../system/updates/updates";
import { UpdatesSection } from "../system/updates/UpdatesSection";
import { cx } from "../util";
import { OptionPicker, Page } from "../ui";

function CalibrationSection() {
  return (
    <Section
      id="sys-calibration"
      title="Calibration"
      description="Teach the app how far the belt really travels, so distance and speed match what you walk."
    >
      <GuidedCalibration />
    </Section>
  );
}

/** Each section by its id; `SYSTEM_TABS` says which tab shows it and in what order. */
const SECTION_VIEWS: Record<string, () => ReactNode> = {
  "sys-treadmill": () => <TreadmillSection />,
  "sys-calibration": () => <CalibrationSection />,
  "sys-drift": () => <SpeedDriftSection />,
  "sys-maintenance": () => <MaintenanceSection />,
  "sys-strava": () => <StravaSection />,
  "sys-heart": () => <HeartRateSection />,
  "sys-neural": () => <NeuralSection />,
  "sys-sessions": () => <SessionsSection />,
  "sys-desk": () => <DeskSection />,
  "sys-shortcuts": () => <ShortcutsSection />,
  "sys-startup": () => <StartupSection />,
  // Renders nothing where the OS asks for no permissions (Windows).
  "sys-permissions": () => <PermissionsSection />,
  "sys-updates": () => <UpdatesSection />,
  "sys-labs": () => <LabsSection />,
  "sys-diagnostics": () => <DiagnosticsSection />,
};

/** The four group tabs. CORE carries the update dot, since Updates is there. */
export function SystemTabs({ tab, onChange }: { tab: SystemTab; onChange: (next: SystemTab) => void }) {
  const updateDot = useAppState((s) => showUpdateBadge(s.update)) ?? false;
  return (
    <OptionPicker
      label="System groups"
      className="flex flex-wrap overflow-hidden rounded border border-white/10"
      options={SYSTEM_TABS.map((t) => ({
        value: t.id,
        label: (
          <span className="flex items-center gap-1.5">
            {t.label}
            {t.id === "core" && updateDot && <UpdateDot />}
          </span>
        ),
      }))}
      value={tab}
      onChange={onChange}
      optionClassName={(active) =>
        cx(
          "px-2.5 py-1 font-mono text-[11px] font-bold uppercase tracking-[0.12em] transition-colors",
          active ? "bg-neon-cyan/15 text-neon-cyan" : "text-white/60 hover:bg-white/5 hover:text-white",
        )
      }
    />
  );
}

export function SystemPage() {
  const [tab, setTab] = useState<SystemTab>(readSystemTab);
  const body = useRef<HTMLDivElement>(null);
  useEffect(() => subscribeSystemTab(setTab), []);

  const choose = (next: SystemTab) => {
    openSystemTab(next);
    // A new tab starts at its top, not where the last one was scrolled to.
    const scroller = body.current?.closest(".dash-page-body");
    if (scroller) scroller.scrollTop = 0;
  };

  const sections = SYSTEM_TABS.find((t) => t.id === tab)?.sections ?? [];
  return (
    <Page codename="SYSTEM_CONFIG" title="System" actions={<SystemTabs tab={tab} onChange={choose} />}>
      <div ref={body} className="flex flex-col gap-3">
        {sections.map((id) => (
          <SectionView key={id} id={id} />
        ))}
      </div>
    </Page>
  );
}

function SectionView({ id }: { id: string }) {
  const View = SECTION_VIEWS[id];
  return View ? <View /> : null;
}
