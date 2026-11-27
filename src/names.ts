// One name per thing: each codename's plain meaning, shown as the small grey
// line under it (Live tiles, cards, the HUD page's widget rows) and after it in
// System labels ("OCULAR_IMPLANT: Eye break"). The names and the plain words
// are Fresh Brewed Design's (issue #392); add a codename here before giving it
// a plain line anywhere.

export const PLAIN = {
  VELOCITY: "belt speed",
  TILT: "incline",
  DECK_CYCLES: "walk time",
  DISPLACEMENT: "walk distance",
  DAILY_TOTAL: "distance today",
  LOCOMOTION_INC: "step count",
  METABOLIC_BURN: "calories burned",
  NEURAL_FLUIDITY: "pace while typing",
  LIFETIME_ODOMETER: "all-time distance",
  PULSE_RATE: "heart rate",
  OCULAR_IMPLANT: "eye break",
  DESK_SYNC: "desk days walked",
  WALL_CLOCK: "walks by hour",
  GROUND_TRUTH: "belt calibration",
  HARDWARE_LINK: "connect treadmill",
  HEART_LINK: "heart sensor",
  STRIDE_FIT: "stride limit",
  ATROPHY_WARNING: "sit-too-long reminder",
  NO_CARRIER: "treadmill offline",
} as const;

export type Codename = keyof typeof PLAIN;

/** A System label: "OCULAR_IMPLANT: Eye break". */
export function systemLabel(codename: Codename): string {
  const plain = PLAIN[codename];
  return `${codename}: ${plain.charAt(0).toUpperCase()}${plain.slice(1)}`;
}
