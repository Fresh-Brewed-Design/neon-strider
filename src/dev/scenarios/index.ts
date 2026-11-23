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
import { echo } from "./echo";
import { engage } from "./engage";
import { eyeBreak } from "./eyebreak";
import { guided } from "./guided";
import { heartRate } from "./heart";
import { history } from "./history";
import { hrCool } from "./hrcool";
import { hrSpark } from "./hrspark";
import { hrZone } from "./hrzone";
import { hud } from "./hud";
import { lightsOut } from "./lightsout";
import { link } from "./link";
import { maintenance } from "./maintenance";
import { neural } from "./neural";
import { pcWalkShare } from "./pcwalk";
import { platform } from "./platform";
import { programs } from "./programs";
import { seated } from "./seated";
import { stasis } from "./stasis";
import { strava } from "./strava";
import { strideFit } from "./stridefit";
import { traffic } from "./traffic";
import type { Scenario } from "./types";
import { typingSpeed } from "./typingspeed";
import { update } from "./update";
import { walkaway } from "./walkaway";
import { wallClock } from "./wallclock";
import { weekly } from "./weekly";
import { wristLink } from "./wristlink";

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
  wristLink,
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
  typingSpeed,
];
