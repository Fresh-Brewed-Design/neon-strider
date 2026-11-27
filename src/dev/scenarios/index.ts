// Every feature's part of the mocked backend, one file each. A new feature adds
// its file and one line here.
//
// Order matters only where two features' flags change the same field (most
// often the deck's status and speed): the mock applies them top to bottom, so
// the later one wins, and a later file's command handler replaces an earlier
// one's. A new feature that touches neither goes at the end.
import { belt } from "./belt";
import { calibration } from "./calibration";
import { calls } from "./calls";
import { deckProfile } from "./deckprofile";
import { deskSync } from "./desksync";
import { easyStart } from "./easystart";
import { easyStop } from "./easystop";
import { echo } from "./echo";
import { engage } from "./engage";
import { eyeBreak } from "./eyebreak";
import { fitToTime } from "./fittotime";
import { ghostPod } from "./ghostpod";
import { guided } from "./guided";
import { heartRate } from "./heart";
import { history } from "./history";
import { hrCool } from "./hrcool";
import { hrSpark } from "./hrspark";
import { hrZone } from "./hrzone";
import { hud } from "./hud";
import { hudMotion } from "./hudmotion";
import { lightsOut } from "./lightsout";
import { link } from "./link";
import { maintenance } from "./maintenance";
import { neural } from "./neural";
import { pcWalkShare } from "./pcwalk";
import { platform } from "./platform";
import { presenting } from "./presenting";
import { programs } from "./programs";
import { programTones } from "./programtones";
import { seated } from "./seated";
import { shortcuts } from "./shortcuts";
import { speedDrift } from "./speeddrift";
import { speedSpark } from "./speedspark";
import { stasis } from "./stasis";
import { stasisClock } from "./stasisclock";
import { strava } from "./strava";
import { stravaToday } from "./stravatoday";
import { strideFit } from "./stridefit";
import { today } from "./today";
import { traffic } from "./traffic";
import type { Scenario } from "./types";
import { typingSpeed } from "./typingspeed";
import { update } from "./update";
import { walkaway } from "./walkaway";
import { wallClock } from "./wallclock";
import { weekly } from "./weekly";

export const SCENARIOS: Scenario[] = [
  hud,
  belt,
  link,
  programs,
  strava,
  deskSync,
  deckProfile,
  weekly,
  eyeBreak,
  seated,
  calls,
  neural,
  engage,
  lightsOut,
  walkaway,
  update,
  stasis,
  traffic,
  calibration,
  ghostPod,
  heartRate,
  maintenance,
  history,
  guided,
  hrSpark,
  hrZone,
  echo,
  strideFit,
  wallClock,
  hrCool,
  pcWalkShare,
  platform,
  easyStart,
  typingSpeed,
  today,
  speedDrift,
  easyStop,
  fitToTime,
  programTones,
  // After strava and history: it reads the account and today's mock walks.
  stravaToday,
  // After the scenarios that set the belt's speed: VELOCITY reads the trace's last point.
  speedSpark,
  // After every scenario that pauses the belt at load.
  stasisClock,
  // After link and programs: it replays their connect and step changes.
  hudMotion,
  // After platform: the keys follow ?mac.
  shortcuts,
  // Last: it holds the cards the scenarios above put up.
  presenting,
];
