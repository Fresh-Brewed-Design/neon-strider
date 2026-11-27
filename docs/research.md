# Research behind the programs and desk defaults

Neon Strider ships example programs and desk defaults that are meant to follow current research
on walking, sitting and desk work. This page checks each one against the literature (checked
November 2026, roadmap #26). Each section gives the value, what the studies say, the verdict and
the sources.

Some caveats apply to all of it:

- Most of the glucose studies are short crossover trials. Small groups (10 to 40 people), often
  overweight or with type 2 diabetes, are measured for a day or two. They show what happens after
  a meal, not long-term health outcomes.
- The intensity figures (METs) below are estimates from the ACSM walking equation
  (oxygen cost = 0.1 × speed + 1.8 × speed × grade + 3.5, speed in m/min), not measurements. They
  are less accurate below about 3 km/h. The app's incline is a percent grade once calibrated
  (System, Calibration), so the equation's grade is the value shown.
- None of this is medical advice. Anyone with heart disease or diabetes medication should check
  with their doctor before exercising straight after meals.

## Post-meal

**Was:** 10 min at 4.0 km/h, 10 % incline. **Now:** 10 min at 4.0 km/h, 5 % incline.

- Timing: start soon after eating. In a meta-analysis of crossover trials, exercise after a meal
  lowered the glucose rise more than the same exercise before it. The sooner after the meal,
  the bigger the effect (Engeroff et al. 2023). The app has no meal clock: the program is meant
  to be run when you get back to the desk after eating.
- Length: 10 to 15 minutes is well supported. People with type 2 diabetes who walked 10 minutes
  after each meal had lower post-meal glucose than when they walked 30 minutes once a day
  (Reynolds et al. 2016). In older adults, three 15-minute walks after meals controlled 24-hour
  glucose better than one 45-minute walk (DiPietro et al. 2013). Even 2 to 5 minutes of light
  walking helps (Buffey et al. 2022), so 10 minutes is a sensible middle.
- Intensity: the studies used light to moderate walking, about 3 METs (DiPietro) or a
  self-paced walk (Reynolds). At 4.0 km/h and 10 % the estimate is about 6.3 METs, the edge of
  vigorous and well above what was tested. It is also the deck's top incline level, under a
  desk, straight after a meal. At 5 % it is about 4.6 METs, which is moderate.

**Verdict:** keep the time and speed, lower the incline to 5 %. **In the app:** "Walking soon
after a meal blunts the rise in blood sugar."

## Workday

**Was:** two programs, Workday (5 min at 3.0 km/h and 5 %, every 30 min) and Micro-break (2 min
at 3.2 km/h, flat, every 20 min). **Now:** one program, Workday, at 5 min every 30 min, 3.0 km/h
and 5 %.

- Both were sitting breaks backed by the same trials, so they differ only in break length and
  spacing. In the program those are its two settings: the step's minutes are the break length
  and "Repeat every" is the spacing, edited like any program, along with speed and incline. The
  spacing has to be longer than the break, since the rest between breaks is the spacing minus
  the break.
- In a dose-response trial, Duran et al. (2023) had 11 middle-aged and older adults sit for
  8 hours with breaks of 1 or 5 minutes of light walking every 30 or 60 minutes. Only 5 minutes
  every 30 minutes significantly lowered the glucose rise after meals, so it is the default.
  Every dose lowered systolic blood pressure against unbroken sitting, by about 4 mmHg for
  5 minutes every 30 (−4.3 mmHg). That is a one-day lab result, not a training effect.
- Shorter, more frequent breaks help too: 2 minutes of light walking (3.2 km/h) every 20 minutes
  cut the post-meal glucose rise by about a quarter and insulin by about the same, compared with
  sitting through (Dunstan et al. 2012). In people with type 2 diabetes, 3 minutes of light
  walking (3.2 km/h) every 30 minutes also helped (Dempsey et al. 2016). Anyone who finds
  5 minutes every 30 too disruptive can set 2 minutes every 20. Breaks of 1 minute lowered blood
  pressure but not glucose (Duran et al. 2023), so 2 minutes is the shortest worth setting.
- A meta-analysis of sitting-break trials found that light walking breaks lower post-meal
  glucose and insulin compared with unbroken sitting, and do better than standing breaks
  (Buffey et al. 2022).
- At about 3.7 METs, 3.0 km/h with 5 % is light to moderate, in line with the trials.
- Workday and the idle nudge both follow these trials, but they do different jobs. Workday
  walks you: once started it runs a short walk on a timer, all day, and after each rest the
  belt waits for GO. The idle nudge only reminds you: after 30 minutes of PC use with the belt
  still it shows a card, and nothing moves until you pick a program or start the belt.
- Existing installs: a Workday never edited keeps its name and plan, with a reworded note. A
  Micro-break never edited is removed, unless the sit-too-long card offers it, it was the last
  program run, or it was running, resting or waiting for GO. Then it stays as Micro-break at
  2 minutes every 20, with a reworded note. It also stays when Workday was edited or deleted.
  Edited copies keep what you set.
**In the app:** "Five easy minutes every half hour keep the body online." Micro-break, where it stays: "Two easy minutes every 20 keep the body moving."

## Focus

**Was:** 2.5 km/h, 5 % incline, until ended. **Now:** 2.0 km/h, flat, until ended.

- Reviews of treadmill desks agree that thinking holds up while walking slowly: attention,
  reading and reasoning scores are about the same as sitting. Typing and mouse work get slower
  and less accurate, and treadmill desks slow typing more than cycle desks (John et al. 2009;
  Dupont et al. 2019; Podrekar et al. 2020). Practice reduces the loss. Long-term workplace
  studies found no drop in overall productivity (Ojo et al. 2018).
- People choose slow speeds when they work and walk. Office workers using a walk-and-work desk
  settled on about 1.8 km/h, 1.1 mph (Levine and Miller 2007). Most lab studies used about
  1.6 to 2.4 km/h on a flat treadmill. 2.5 km/h is at the top of that range, and none of the
  studies used an incline for desk work.
- 2.0 km/h flat is light activity, about 2 METs. That is still well above sitting, and walking
  at a desk roughly doubles energy use compared with sitting (Levine and Miller 2007).

**Verdict:** change to 2.0 km/h and 0 %. The evidence for the exact speed is thin; slower and flat
is the safer choice for typing. **In the app:** "A slow, flat walk keeps typing and
thinking steady."

## Brisk (new)

**New:** 30 min at 5.0 km/h, flat.

- The WHO 2020 guidelines ask adults for 150 to 300 minutes of moderate activity a week (Bull et
  al. 2020). The other programs are light or short, so they count for little of that. Shorter
  bouts count too: the 2020 guidelines dropped the old rule that only bouts of 10 minutes or
  more count.
- Walking at about 4.8 km/h is about 3.5 METs on level ground (Ainsworth et al. 2011), and
  moderate intensity starts at 3 METs. About 100 steps a minute is the usual marker of moderate
  walking for adults (Tudor-Locke et al. 2019). 5.0 km/h is a little above that, and still a
  walk on the deck. Five of these a week reach 150 minutes.
- This is not a desk-work pace: by the treadmill-desk studies above, typing will suffer, so it
  suits calls, reading or a break.

**In the app:** "Thirty brisk minutes count toward a weekly 150 of moderate activity."

## VO2 max (new)

**New:** 5 min at 4.5 km/h, then four times (4 min at 6.0 km/h and 10 % incline, 3 min at
4.0 km/h flat), then 3 min at 3.0 km/h. 36 minutes in all.

- The Norwegian 4x4 is the best-studied interval format for raising VO2 max: four 4-minute
  efforts at 85 to 95 % of maximum heart rate, with 3 minutes of easy movement between them.
  Wisløff et al. (2007) found it raised VO2 max about 46 % more than moderate continuous
  training in people with heart failure. A meta-analysis of interval training found it improved
  VO2 max more than continuous moderate training, with the biggest gains from intervals of 3 to
  5 minutes (Bacon et al. 2013; Weston et al. 2014).
- The deck tops out at 8 km/h and 10 % incline, so the effort comes from the incline: by the
  ACSM equation, 6.0 km/h at 10 % is about 9 METs, against about 3 METs for the easy walks
  between. The estimate is a guide only. It ignores the belt's own friction and your weight.
  Whether it reaches 85 to 95 % of your maximum heart rate depends on your fitness, so judge it
  by effort: hard but steady, with a few words still possible. The 4 minutes are the dose, so a
  fit runner may need a faster speed (edit the program) and a beginner a lower incline.
- The warm-up and cool-down follow the same protocol. No step starts a still belt: the belt
  moves for the whole program, so it starts with one press, like any program.
- Two or three sessions a week with a day between is what the studies used. This is hard
  exercise: anyone with heart disease or risk factors should check with their doctor first.

**Heart rate zones (#124):** each climb holds 85 to 95 % of your maximum heart rate, the
intensity the 4x4 studies prescribe.

- With HEART_LINK reading a sensor, each climb trims its speed by 0.1 km/h at most every 20 s,
  within 1 km/h of 6.0 km/h, to keep the pulse in the zone. Without a reading the climbs run
  their set speed, exactly as before.
- The zone is a percentage of the maximum heart rate set in System, Heart rate; until one is
  set it uses 190 bpm (220 minus age at 30). A measured or age-based maximum of your own makes
  the zone right for you.
- A pulse takes a minute or two to climb at the start of each effort, so each zone step leaves
  the speed alone for its first 2 minutes of running (#157), and again after a pause and GO.
  After a step change in treadmill speed, heart rate follows with a time constant of about
  58 s (57.6 s mean in 24 men, the same for steps up and down; Hunt et al. 2015). Two time
  constants cover about 86 % of the change and leave half of each 4 minute climb to trim in;
  waiting three (about 3 minutes) would leave one. Without the wait the trim pushed the speed up
  early in each climb, then pulled it back once the pulse caught up.
- With the Labs switch ZONE_TILT on, a zone step can trim its incline instead of (or as well as)
  its speed (#126), for a walk where the speed should stay put. Incline moves one deck level at
  a time, at most every 60 s, within 2 % of the step's incline and only to levels the deck runs
  (DECK_PROFILE). By the ACSM walking equation, 1 km/h at 3 to 4 km/h costs about the same as
  1.5 to 2 % of grade, so 2 % matches the speed trim's 1 km/h. One level (about 1 % on most
  decks) is a bigger step than 0.1 km/h, so it waits about one heart rate time constant to show
  before the next. Trimming both moves the incline first and the speed once the incline is at
  its limit, and goes back the same way.
- Existing installs get the zones only on a VO2 max that was never edited; an edited one keeps
  what you set.

**In the app:** "Four hard climbs train VO2 max, and the last walk ends on heart rate." The
cool-down ends at or under 60 % of maximum heart rate once its 3 minutes are done (see
Cool-down on heart rate).

## Interval walk (new)

**New:** 5 min at 4.0 km/h, then five times (3 min at 6.0 km/h and 5 % incline, 3 min at
3.5 km/h flat). 35 minutes in all.

- Interval walking training (IWT, often called Japanese walking) alternates 3 minutes of fast
  walking with 3 minutes of slow walking. In Nemoto et al. (2007), 246 middle-aged and older
  adults (mean age 63) were assigned to no training, continuous walking at about 50 % of their
  peak aerobic capacity, or IWT: 5 or more sets a day of 3 minutes above 70 % of peak aerobic
  capacity for walking and 3 minutes at about 40 %, on 4 or more days a week, for 5 months.
  The IWT group gained more thigh strength and peak aerobic capacity than the continuous
  group, and its resting systolic blood pressure fell more (P = .01).
- How it differs from VO2 max: the effort is lower (about 6 to 7 METs on the brisk steps
  against about 9 on VO2 max's climbs), the blocks are equal (3 minutes brisk, 3 minutes easy,
  against 4 hard and 3 easy), it is meant for 4 or more days a week (VO2 max: two or three),
  and it was tested in middle-aged and older adults. It suits anyone who finds VO2 max too
  hard, or who wants a harder walk than Brisk on more days of the week.
- On the deck: by the ACSM equation, 6.0 km/h at 5 % is about 6.4 METs and 3.5 km/h flat about
  2.7 METs. The trial set each person's fast pace from their own peak aerobic capacity, which
  the app can't measure, so judge the brisk steps by effort: a fast walk where talking takes
  effort. A fitter walker can raise the speed or incline (edit the program), and a beginner
  can lower them.
- Every step moves the belt and nothing repeats, so no step starts a still belt: like VO2 max,
  starting the program is the one press.

**Blood pressure.** Trials found lower resting blood pressure after months of training, both
for walking in general and for interval walking. A trial result is an average, not a promise
for one person, and Neon Strider shows no blood pressure figures or claims.

- A Cochrane review of 73 trials (5,763 people) found that walking probably lowers systolic
  blood pressure (moderate certainty) and may lower diastolic blood pressure (low certainty;
  Lee et al. 2021). Brisk is that kind of training.
- A network meta-analysis of 270 trials (15,827 people) found that aerobic training, interval
  training, resistance training and isometric training all lowered resting blood pressure;
  isometric training ranked first for systolic pressure (Edwards et al. 2023).
- IWT lowered resting systolic blood pressure more than continuous walking at a moderate
  pace (Nemoto et al. 2007, above).
- Workday: in Duran et al. (2023) every break dose lowered systolic blood pressure over
  one 8-hour day of sitting, against sitting through (see Workday). That is a one-day lab
  result, not a training effect.
- Not cited: the press coverage of Nemoto et al. gives the IWT group's drop as 9 and 5 mmHg
  (systolic and diastolic). The abstract doesn't state those figures and the full paper could
  not be checked, so they are left out.

**In the app:** "Five rounds alternate three brisk minutes with three easy ones."

## Cool-down on heart rate (new)

**New (#233):** a COOL_DOWN step can end on heart rate. It walks its minutes, then walks on
until the pulse has stayed under a target for 30 s. The target is a share of the maximum heart
rate in System, Heart rate (60 % unless changed), set per step in the editor as % of max or bpm.
Without a reading the step ends on its minutes, as before.

**Is heart rate a fair sign a cool-down is done?** In part. It is the one recovery measure the
app can read, and it falls faster after an active cool-down than a passive one, but it is not
the whole recovery.

- What a cool-down does: in a review of the evidence, an active cool-down did little for
  soreness, injury or next-day performance. Heart rate came down faster with one: by 3 minutes
  into an active cool-down it was lower than after resting, and in one case it was back to rest
  after an active cool-down while still raised 30 minutes after a passive one. Whether a
  cool-down prevents fainting or heart events after exercise is not known (Van Hooren and Peake
  2018). The ACSM's own advice is 5 to 10 minutes of easy walking so heart rate and blood
  pressure come down gradually and blood does not pool in the legs (ACSM, undated handout).
- Why keep walking at a desk: standing still after exercise lets blood pool in the legs, and
  blood pressure can drop far enough for light-headedness. The calf muscle pump of easy walking
  keeps blood returning to the heart and is one of the best ways to prevent it; heat makes it
  worse (Romero, Minson and Halliwill 2017). Unloaded pedalling during recovery kept blood
  pressure higher than sitting still (Lei et al. 2023), and standing recovery lowered it more
  than seated (Lu et al. 2024). A desk program ends with you standing, so the walk is the point.
- Heart rate runs ahead of the rest: the fast fall in the first minute is the vagal brake coming
  back, the slower tail is sympathetic outflow easing off over up to 90 minutes, and the
  widened blood vessels that lower blood pressure last hours (Romero et al. 2017). So a pulse
  under the target says the heart has eased off, not that blood pressure and temperature have.
- Heat and fluid keep it up: after 90 minutes of exercise in 40 °C heat without drinking, heart
  rate and core temperature stayed higher than with fluids (Macartney et al. 2020). In a warm
  room a pulse-only end may run long. There is no time cap: SKIP and END are always there.
- Not cited: the issue's lead that core temperature recovers slower than heart rate is a
  preprint (Research Square rs-10884677) whose text could not be read, so it is left out.

**Minutes as a minimum: yes.** The muscle pump and the gradual fall in blood pressure need the
walk itself, whatever the pulse says. After an easy walk the pulse may already be under the
target, and a pulse-only end would finish at once. So the step's minutes always run, the 5 to
10 minutes the ACSM advises being a good minimum, and the pulse only adds time to them.

**The target: % of maximum heart rate.**

- The ACSM bands by share of maximum heart rate are very light under 57 %, light 57 to 63 %,
  moderate 64 to 76 % and vigorous 77 to 95 % (Garber et al. 2011; the CDC gives the same 64 to
  76 % for moderate). A cool-down is done with the work when the pulse is back in the light band
  or under it, so 60 % sits inside light with a few bpm of margin from moderate. At the default
  190 bpm maximum that is 114 bpm.
- % of max is what the heart zones already use, so one maximum (System, Heart rate) sets both,
  and a measured maximum makes both right for you. The editor can show it in bpm.
- Not resting heart rate: the app has no resting value, and walking keeps the pulse well above
  rest. A target near resting would never be reached on the belt. Not relative to the step
  before either: after an easy walk a drop of so many beats may never come, and after a hard one
  it comes while the pulse is still high.
- The editor allows 50 to 76 %. Over 76 % is vigorous, not a cool-down. Under 50 %, deep in the
  very light band, an easy walk may never get there and the step would walk on until SKIP.

**How long under it: 30 s.** After a step change in treadmill speed, heart rate follows with a
time constant of about 58 s, the same going down as up (Hunt et al. 2015, see VO2 max). A pulse
that is truly falling through the target keeps falling, so 30 s, about half a time constant,
costs little. It keeps a short dip (a stray optical reading, a few seconds off the belt's pace)
from ending the step. Wrist sensors were off by 1 to 8 % on average against a reference monitor at
3.2 to 6.4 km/h, mostly reading 1 to 7 bpm high (Thiebaud et al. 2018), so a single reading a
few beats under the target means little and half a minute of them means more.

- A reading that stops (a watch that stops broadcasting) restarts the 30 s, and with no
  reading at all once the minutes are done the step ends on its minutes. A pause also restarts
  it, since standing still lowers the pulse for a reason that has nothing to do with recovery.
- Heart rate only decides when the step ends. It never starts, resumes or stops the belt: the
  next step or the program's end follows the same rules as a timed step.

**On by default: one target, 60 %, for every cool-down.** A COOL_DOWN step gets the target
when it is made (the editor ticks it when a step's kind is set to COOL_DOWN, and it can be
unticked), and the seeded VO2 max ends on it. The question was whether the target should
depend on what comes before: higher after hard climbs, lower after a light walk. The evidence
says no.

- What comes before changes how long the fall takes, not where it ends. After harder exercise
  the heart's autonomic recovery is slower, though the dose response is not clear (Michael,
  Graham and Davis 2017). That is the case a pulse end is for: the cool-down walks on for
  longer after a hard program, at the same 60 %.
- A higher target after hard work would end too soon. The 4x4 that VO2 max follows walks its
  3 minute rests at about 70 % of maximum (Helgerud et al. 2007), so a cool-down that ended at
  70 % would end at the pace of a rest between climbs, still moderate by the ACSM bands
  (Garber et al. 2011). 60 % is under the rests and inside the light band.
- A lower target after hard work could keep the step walking on for a long time. The slow,
  sympathetic part of the fall lasts up to 90 minutes (Romero et al. 2017), and walking itself
  holds the pulse above rest, so a target deep in the very light band may never come on the belt.
- After a light walk the pulse is often already under 60 %. The step then ends on its minutes,
  which is what a cool-down after easy walking needs (minutes as a minimum, above). A program of
  light walks gains nothing from a target of its own.
- Fixed bpm rules do not fit either: equipment manuals and fitness guides give "under 100" or
  "under 110 bpm", which ignore age and fitness and are not ACSM advice. 60 % of the maximum in
  System, Heart rate is 114 bpm at the 190 bpm default, near those numbers, and moves with your
  own maximum.

**Verdict:** keep one default, 60 % of maximum, for every COOL_DOWN step. The seeded VO2 max
gets it (Interval walk ends on a rest, with no cool-down). An unedited VO2 max on an existing
install gets it too (migration 11); an edited one and any program you made are left as they are.

**In the app:** "The cool-down walks on until heart rate stays under the target."

## Idle nudge

**Was:** after 60 minutes of PC use without walking. **Now:** 30 minutes.

- The glucose benefit in Duran et al. (2023) only appeared with breaks every 30 minutes, not
  every 60. Dunstan et al. (2012) and Dempsey et al. (2016) used 20 and 30 minutes. Every-60
  breaks still lowered blood pressure, so 60 was not wrong, just weaker.
- The WHO guidelines say to sit less but found the evidence too thin to set a limit (Bull et al.
  2020). So 30 minutes follows the trials, not a guideline.
- Walking 2 minutes resets the timer, which matches the shortest breaks that helped (Dunstan et
  al. 2012).

**Verdict:** change to 30 minutes. If you never changed it, it moves from 60 to 30; any other
value you set is kept. The range stays 15 to 240.

## Snooze

**Was:** 15 minutes. No study tests a snooze. Idle time keeps counting while it is snoozed,
so the next card still comes about half an interval later, which keeps the breaks close to
every 30 minutes.

**Now (#422):** the first snooze is 15 minutes, the second 30, then 60 each time, until a walk
or time away resets the seated time. A simulated desk day from the owner's settings had 16
cards and 12 snoozes, one every 15 minutes; the longer snoozes cut that repeat demand. The
card also shows once: left unanswered for a minute it folds into a yellow seated-time readout
on the HUD. Fresh Brewed Design's decision, not a study's; the 30 minute threshold stays.

## Walk-away timings

**Was and is:** pause after 10 minutes without keyboard or mouse input (2 to 30), a 30-second
warning, and an instant pause on lock. Kept. These are safety and comfort settings (watching a
video while walking is normal), not health advice, so the literature has nothing to correct.

## Eye breaks (OCULAR_IMPLANT)

**Interval and length (was and is):** a 90-second break every 20 minutes of screen use. Kept.

- The 20-20-20 rule (20 seconds every 20 minutes) is a mnemonic with little clinical support
  (Boulet 2016). Focusing needs about 90 seconds or more to relax, hence the 90-second default.
- 20-second breaks did little for eye strain symptoms in a controlled reading task (Johnson &
  Rosenfield 2023). With reminders, following the rule for two weeks eased symptoms but changed no
  tear-film signs (Talens-Estarelles et al. 2023).
- For myopia, the animal data point to breaks of 5 minutes or more each hour (Pucker & Gawne
  2023), so the break length goes up to 300 seconds.

**Blink cue (was):** one beep every 10 seconds. That number had no source, and it is slower than
people already blink at a screen (about 14 to 15 a minute, one every 4 seconds; Chu et al. 2014).
A prompt to blink more often misses the problem anyway: screen reading lowers blink rate against
looking around, but what sets screens apart from paper is the share of incomplete blinks, where
the lids never meet (Chu et al. 2014: 7.0 % on screen against 4.3 % on paper; Argilés et al.
2015: incomplete blinks rose only with electronic reading). Coles-Brennan et al. (2019) review
how fewer and incomplete blinks drive tear evaporation and screen eye strain.

**Blink cue (now):** a guided full blink, repeated through the break. Close the eyes and hold,
squeeze gently, open, each step 2 seconds, so one full blink every 6 seconds.

- Blinking exercises cut incomplete blinks. Kim et al. (2021) had 41 people with dry eye do a
  10-second blinking exercise every 20 minutes for four weeks: incomplete blinks fell from 54 %
  to 34 % of blinks, and symptoms and tear breakup time improved. Their every-20-minutes timing
  matches the break interval.
- Wolffsohn et al. (2025) compared routines in 98 people with dry eye, using an app. The best was
  15 close-squeeze-open cycles, each step held for 2 s, three times a day. Adding the squeeze
  step lowered symptom frequency compared with close-and-open only, and the best routine
  reduced incomplete blinks (p < 0.001) and conjunctival staining in a second group of 28. The
  benefits faded within two weeks of stopping, so the exercise works while it is kept up, which
  a break that repeats it suits.
- At 2 seconds a step, the default 90-second break is exactly 15 full blinks, the trial's
  routine. Two of those breaks an hour come to about six sessions over a working morning, more
  than the trial's three a day, with the same per-session dose.
- Cue style: tones, one short beep (or pair) per step. Speech would say a word 45 times in a
  90-second break, which nags; a soft sine beep stays calm, needs no voice or language on the PC
  and sends nothing anywhere. The card shows the same step as text (`SHUTTER // CLOSE`,
  `SQUEEZE`, `OPEN`) for when Tones is off.

**Verdict:** change the 10-second blink beep to a guided full blink (2 s close and hold, 2 s
squeeze, 2 s open). Caveats: both blinking trials were small and short, in people with dry eye,
and one was run by the app's own developers (disclosed in the paper). No trial has tested the
exercise inside a screen break, so this follows the trial's dose, not a tested break routine.

## Sources

- Ainsworth BE, Haskell WL, Herrmann SD, et al. (2011). 2011 Compendium of Physical Activities:
  a second update of codes and MET values. *Medicine & Science in Sports & Exercise* 43(8):
  1575–1581. https://doi.org/10.1249/MSS.0b013e31821ece12
- American College of Sports Medicine (undated). *A road map to effective muscle recovery*
  (consumer handout). https://www.acsm.org/docs/default-source/files-for-resource-library/a-road-map-to-effective-muscle-recovery.pdf
- American College of Sports Medicine (2021). *ACSM's Guidelines for Exercise Testing and
  Prescription*, 11th ed. Wolters Kluwer. (Walking metabolic equation.)
- Argilés M, Cardona G, Pérez-Cabré E, Rodríguez M (2015). Blink rate and incomplete blinks in
  six different controlled hard-copy and electronic reading conditions. *Investigative
  Ophthalmology & Visual Science* 56(11): 6679–6685. https://pubmed.ncbi.nlm.nih.gov/26517404/
- Bacon AP, Carter RE, Ogle EA, Joyner MJ (2013). VO2max trainability and high intensity
  interval training in humans: a meta-analysis. *PLoS ONE* 8(9): e73182.
  https://doi.org/10.1371/journal.pone.0073182
- Boulet C (2016). The "20/20/20 rule": when good intentions and axiomatic habit displace best
  practices. *Canadian Journal of Optometry* 78(4).
  https://openjournals.uwaterloo.ca/index.php/cjo/article/view/448
- Buffey AJ, Herring MP, Langley CK, Donnelly AE, Carson BP (2022). The acute effects of
  interrupting prolonged sitting time in adults with standing and light-intensity walking on
  biomarkers of cardiometabolic health in adults: a systematic review and meta-analysis.
  *Sports Medicine* 52(8): 1765–1787. https://doi.org/10.1007/s40279-022-01649-4
- Bull FC, Al-Ansari SS, Biddle S, et al. (2020). World Health Organization 2020 guidelines on
  physical activity and sedentary behaviour. *British Journal of Sports Medicine* 54(24):
  1451–1462. https://doi.org/10.1136/bjsports-2020-102955
- Chu CA, Rosenfield M, Portello JK (2014). Blink patterns: reading from a computer screen versus
  hard copy. *Optometry and Vision Science* 91(3): 297–302. https://pubmed.ncbi.nlm.nih.gov/24413278/
- Coles-Brennan C, Sulley A, Young G (2019). Management of digital eye strain. *Clinical and
  Experimental Optometry* 102(1): 18–29. https://doi.org/10.1111/cxo.12798
- Dempsey PC, Larsen RN, Sethi P, et al. (2016). Benefits for type 2 diabetes of interrupting
  prolonged sitting with brief bouts of light walking or simple resistance activities.
  *Diabetes Care* 39(6): 964–972. https://doi.org/10.2337/dc15-2336
- DiPietro L, Gribok A, Stevens MS, Hamm LF, Rumpler W (2013). Three 15-min bouts of moderate
  postmeal walking significantly improves 24-h glycemic control in older people at risk for
  impaired glucose tolerance. *Diabetes Care* 36(10): 3262–3268.
  https://doi.org/10.2337/dc13-0084
- Dunstan DW, Kingwell BA, Larsen R, et al. (2012). Breaking up prolonged sitting reduces
  postprandial glucose and insulin responses. *Diabetes Care* 35(5): 976–983.
  https://doi.org/10.2337/dc11-1931
- Dupont F, et al. (2019). Health
  and productivity at work: which active workstation for which benefits: a systematic review.
  *Occupational and Environmental Medicine* 76(5): 281–294.
  https://doi.org/10.1136/oemed-2018-105397
- Duran AT, Friel CP, Serafini MA, Ensari I, Cheung YK, Diaz KM (2023). Breaking up prolonged
  sitting to improve cardiometabolic risk: dose-response analysis of a randomized crossover
  trial. *Medicine & Science in Sports & Exercise* 55(5): 847–855.
  https://doi.org/10.1249/MSS.0000000000003109
- Edwards JJ, Deenmamode AHP, Griffiths M, et al. (2023). Exercise training and resting blood
  pressure: a large-scale pairwise and network meta-analysis of randomised controlled trials.
  *British Journal of Sports Medicine* 57(20): 1317–1326. https://doi.org/10.1136/bjsports-2022-106503
- Engeroff T, Groneberg DA, Wilke J (2023). After dinner rest a while, after supper walk a mile?
  A systematic review with meta-analysis on the acute postprandial response to exercise before
  and after meal ingestion in healthy subjects and patients with impaired glucose tolerance.
  *Sports Medicine* 53(4): 849–869. https://doi.org/10.1007/s40279-022-01808-7
- Garber CE, Blissmer B, Deschenes MR, et al. (2011). Quantity and quality of exercise for
  developing and maintaining cardiorespiratory, musculoskeletal, and neuromotor fitness in
  apparently healthy adults: guidance for prescribing exercise. *Medicine & Science in Sports &
  Exercise* 43(7): 1334–1359. https://doi.org/10.1249/MSS.0b013e318213fefb (bands as reproduced
  in Cochrane review CD012700, table 3, https://pmc.ncbi.nlm.nih.gov/articles/PMC6481524/table/CD012700-tbl-0003,
  and by the CDC, https://www.cdc.gov/physicalactivity/basics/measuring/heartrate.htm)
- Helgerud J, Høydal K, Wang E, et al. (2007). Aerobic high-intensity intervals improve VO2max
  more than moderate training. *Medicine & Science in Sports & Exercise* 39(4): 665–671.
  https://doi.org/10.1249/mss.0b013e3180304570
- Hunt KJ, Fankhauser SE, Saengsuwan J (2015). Identification of heart rate dynamics during
  moderate-to-vigorous treadmill exercise. *BioMedical Engineering OnLine* 14: 117.
  https://doi.org/10.1186/s12938-015-0112-7
- John D, Bassett D, Thompson D, Fairbrother J, Baldwin D (2009). Effect of using a treadmill
  workstation on performance of simulated office work tasks. *Journal of Physical Activity and
  Health* 6(5): 617–624. https://doi.org/10.1123/jpah.6.5.617
- Johnson S, Rosenfield M (2023). 20-20-20 rule: are these numbers justified? *Optometry and
  Vision Science* 100(1): 52–56. https://pubmed.ncbi.nlm.nih.gov/36473088/
- Kim AD, Muntz A, Lee J, Wang MTM, Craig JP (2021). Therapeutic benefits of blinking exercises in
  dry eye disease. *Contact Lens and Anterior Eye* 44(3): 101329.
  https://doi.org/10.1016/j.clae.2020.04.014
- Lei TH, Fujii N, Zhang X, et al. (2023). The effects of high-intensity exercise training and
  detraining with and without active recovery on postexercise hypotension in young men.
  *Physiological Reports* 11(24): e15862. https://doi.org/10.14814/phy2.15862
- Lee LL, Mulvaney CA, Wong YKY, Chan ES, Watson MC, Lin HH (2021). Walking for hypertension.
  *Cochrane Database of Systematic Reviews* 2: CD008823.
  https://doi.org/10.1002/14651858.CD008823.pub2
- Levine JA, Miller JM (2007). The energy expenditure of using a "walk-and-work" desk for office
  workers with obesity. *British Journal of Sports Medicine* 41(9): 558–561.
  https://pmc.ncbi.nlm.nih.gov/articles/PMC2465387/
- Lu X, Goulding RP, Mündel T, et al. (2024). Interactive effects of exercise intensity and
  recovery posture on postexercise hypotension. *American Journal of Physiology: Regulatory,
  Integrative and Comparative Physiology* 326(6): R567–R577. https://doi.org/10.1152/ajpregu.00036.2024
- Macartney MJ, Meade RD, Notley SR, Herry CL, Seely AJE, Kenny GP (2020). Fluid loss during
  exercise-heat stress reduces cardiac vagal autonomic modulation. *Medicine & Science in Sports
  & Exercise* 52(2): 362–369. https://doi.org/10.1249/MSS.0000000000002136
- Michael S, Graham KS, Davis GM (2017). Cardiac autonomic responses during exercise and
  post-exercise recovery using heart rate variability and systolic time intervals: a review.
  *Frontiers in Physiology* 8: 301. https://doi.org/10.3389/fphys.2017.00301
- Nemoto K, Gen-no H, Masuki S, Okazaki K, Nose H (2007). Effects of high-intensity interval
  walking training on physical fitness and blood pressure in middle-aged and older people.
  *Mayo Clinic Proceedings* 82(7): 803–811. https://doi.org/10.4065/82.7.803
- Ojo SO, Bailey DP, Chater AM, Hewson DJ (2018). The impact of active workstations on workplace
  productivity and performance: a systematic review. *International Journal of Environmental
  Research and Public Health* 15(3): 417. https://doi.org/10.3390/ijerph15030417
- Podrekar N, Kozinc Ž, Šarabon N (2020). The effects of cycle and treadmill desks on work
  performance and cognitive function in sedentary workers: a review and meta-analysis. *Work*
  65(3): 537–545. https://doi.org/10.3233/WOR-203108
- Pucker AD, Gawne TJ (2023). Fighting myopia with intermittent nearwork breaks: 20 seconds every
  20 minutes might not be enough time. *Optometry and Vision Science* 100(1): 31–32.
- Reynolds AN, Mann JI, Williams S, Venn BJ (2016). Advice to walk after meals is more effective
  for lowering postprandial glycaemia in type 2 diabetes mellitus than advice that does not
  specify timing: a randomised crossover study. *Diabetologia* 59(12): 2572–2578.
  https://doi.org/10.1007/s00125-016-4085-2
- Romero SA, Minson CT, Halliwill JR (2017). The cardiovascular system after exercise. *Journal
  of Applied Physiology* 122(4): 925–932. https://doi.org/10.1152/japplphysiol.00802.2016
- Talens-Estarelles C, Cerviño A, García-Lázaro S, Fogelton A, Sheppard A, Wolffsohn JS (2023).
  The effects of breaks on digital eye strain, dry eye and binocular vision: testing the
  20-20-20 rule. *Contact Lens and Anterior Eye* 46(2): 101744.
- Thiebaud RS, Funk MD, Patton JC, et al. (2018). Validity of wrist-worn consumer products to
  measure heart rate and energy expenditure. *Digital Health* 4: 2055207618770322.
  https://doi.org/10.1177/2055207618770322
- Tudor-Locke C, Aguiar EJ, Han H, et al. (2019). Walking cadence (steps/min) and intensity in
  21–40 year olds: CADENCE-adults. *International Journal of Behavioral Nutrition and Physical
  Activity* 16: 8. https://doi.org/10.1186/s12966-019-0769-6
- Van Hooren B, Peake JM (2018). Do we need a cool-down after exercise? A narrative review of
  the psychophysiological effects and the effects on performance, injuries and the long-term
  adaptive response. *Sports Medicine* 48(7): 1575–1595. https://doi.org/10.1007/s40279-018-0916-2
- Weston KS, Wisløff U, Coombes JS (2014). High-intensity interval training in patients with
  lifestyle-induced cardiometabolic disease: a systematic review and meta-analysis. *British
  Journal of Sports Medicine* 48(16): 1227–1234. https://doi.org/10.1136/bjsports-2013-092576
- Wisløff U, Støylen A, Loennechen JP, et al. (2007). Superior cardiovascular effect of aerobic
  interval training versus moderate continuous training in heart failure patients: a randomized
  study. *Circulation* 115(24): 3086–3094. https://doi.org/10.1161/CIRCULATIONAHA.106.675041
- Wolffsohn JS, Travé Huarte S, Bahra I, Finch C, Anyaegbu U, Garcia-Porta N, Kingsnorth A (2025).
  Optimisation of blinking exercises for dry eye disease. *Contact Lens and Anterior Eye* 48(5):
  102453. https://doi.org/10.1016/j.clae.2025.102453
