// Shapes of the session rows Rust reads from SQLite. Rows are parsed on the way
// in, so a schema change in Rust fails loudly here instead of rendering nonsense.

import { z } from "zod";

export const SessionSchema = z.object({
  id: z.number().optional(),
  start_time: z.number(),
  end_time: z.number(),
  duration_seconds: z.number().default(0),
  distance_meters: z.number().default(0),
  raw_distance_meters: z.number().default(0),
  calories: z.number().default(0),
  steps: z.number().default(0),
  raw_steps: z.number().default(0),
  avg_speed: z.number().default(0),
  raw_avg_speed: z.number().default(0),
  avg_incline: z.number().default(0),
  strava_id: z.string().nullable().default(null),
  neural_heat: z.number().default(0),
  neural_ticks: z.number().default(0),
  last_hardware_time: z.number().default(0),
  /** Beats per minute over the walk; null for a walk without a heart rate sensor. */
  avg_heart_rate: z.number().nullable().default(null),
  max_heart_rate: z.number().nullable().default(null),
});

export const SampleSchema = z.object({
  id: z.number().optional(),
  session_id: z.number(),
  timestamp: z.number(),
  speed: z.number(),
  raw_speed: z.number().default(0),
  incline: z.number(),
  distance: z.number().default(0),
  raw_distance: z.number().default(0),
  steps: z.number().default(0),
  raw_steps: z.number().default(0),
  /** Beats per minute from HEART_LINK in that second; null without a reading. */
  heart_rate: z.number().nullable().default(null),
});

export type Session = z.infer<typeof SessionSchema>;
export type Sample = z.infer<typeof SampleSchema>;
