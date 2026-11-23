import { useAppState } from "../../state/useAppState";
import { GuidedCalibration } from "../system/calibration/GuidedCalibration";
import { DeskSection } from "../system/DeskSection";
import { DiagnosticsSection } from "../system/DiagnosticsSection";
import { HeartRateSection } from "../system/HeartRateSection";
import { LabsSection } from "../system/LabsSection";
import { Section } from "../system/fields";
import { MaintenanceSection } from "../system/MaintenanceSection";
import { NeuralSection } from "../system/NeuralSection";
import { SessionsSection } from "../system/SessionsSection";
import { SpeedDriftSection } from "../system/SpeedDriftSection";
import { StartupSection } from "../system/StartupSection";
import { StravaSection } from "../system/strava/StravaSection";
import { TreadmillSection } from "../system/TreadmillSection";
import { UpdateDot } from "../system/updates/UpdateDot";
import { showUpdateBadge } from "../system/updates/updates";
import { UpdatesSection } from "../system/updates/UpdatesSection";
import { Page } from "../ui";

const SECTIONS = [
  { id: "sys-treadmill", label: "Treadmill" },
  { id: "sys-calibration", label: "Calibration" },
  { id: "sys-strava", label: "Strava" },
  { id: "sys-heart", label: "Heart rate" },
  { id: "sys-neural", label: "Neural flow" },
  { id: "sys-drift", label: "Speed drift" },
  { id: "sys-sessions", label: "Sessions" },
  { id: "sys-desk", label: "Desk" },
  { id: "sys-maintenance", label: "Maintenance" },
  { id: "sys-startup", label: "Startup" },
  { id: "sys-updates", label: "Updates" },
  { id: "sys-labs", label: "Labs" },
  { id: "sys-diagnostics", label: "Diagnostics" },
];

/** Sticky links to each section, since the page is long. */
export function SectionIndex() {
  const updateDot = useAppState((s) => showUpdateBadge(s.update)) ?? false;
  return (
    <nav
      aria-label="System sections"
      className="sticky top-0 z-10 -mx-5 mb-3 flex flex-wrap gap-x-1 gap-y-0.5 border-b border-white/10 bg-[#050505]/95 px-5 py-2 backdrop-blur"
    >
      {SECTIONS.map((s) => (
        <button
          key={s.id}
          type="button"
          onClick={() => document.getElementById(s.id)?.scrollIntoView({ behavior: "smooth", block: "start" })}
          className="flex shrink-0 items-center gap-1.5 rounded px-2.5 py-1 font-mono text-[11px] font-bold uppercase tracking-[0.12em] text-white/50 transition-colors hover:bg-white/5 hover:text-neon-cyan"
        >
          {s.label}
          {s.id === "sys-updates" && updateDot && <UpdateDot />}
        </button>
      ))}
    </nav>
  );
}

export function SystemPage() {
  return (
    <Page codename="SYSTEM_CONFIG" title="System">
      <SectionIndex />
      <div className="flex flex-col gap-3">
        <TreadmillSection />
        <Section
          id="sys-calibration"
          title="Calibration"
          description="Teach the app how far the belt really travels, so distance and speed match what you walk."
        >
          <GuidedCalibration />
        </Section>
        <StravaSection />
        <HeartRateSection />
        <NeuralSection />
        <SpeedDriftSection />
        <SessionsSection />
        <DeskSection />
        <MaintenanceSection />
        <StartupSection />
        <UpdatesSection />
        <LabsSection />
        <DiagnosticsSection />
      </div>
    </Page>
  );
}
