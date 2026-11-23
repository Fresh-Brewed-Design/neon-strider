# Changelog

## 1.56.1 (2026-11-23)

### Fixed

- A belt paused with the treadmill's own button now stays paused when the app raises the speed by itself in the same moment (Neural flow speeding back up, a call ending) or when a higher minimum speed is saved. GO, Engage or a speed set by hand resumes it.

## 1.56.0 (2026-11-23)

### New

- Neon Strider runs on macOS 13 or later as a menu bar app: the dashboard drops down from a menu bar icon that follows light and dark mode, the HUD floats over every Space and full-screen app, the shortcuts are ⌃⌥ plus a key, and the Strava login lives in the Keychain. There's no Mac download yet; it builds from source (docs/macos.md).

## 1.55.2 (2026-11-23)

### Fixed

- When Windows closes the treadmill link while it still reads as connected, Neon Strider now reconnects at once instead of writing into the dead link until the link goes silent for 10 s.

## 1.55.1 (2026-11-23)

### Changed

- GHOST_POD and its power switch are gone from System -> Labs. WRIST_LINK stays and now starts by its own switch.

## 1.55.0 (2026-11-22)

### New

- New WALKED stat (PC TIME WALKED): the share of today's PC time spent walking, as a percent and as time, for example 38 % and 2h 10m of 5h 40m.
- Pick WALKED on the HUD page. The Live page shows WALKED when a stat tile is free.
- Only time with keyboard or mouse use counts, so a day away from the PC has no figure. The count carries on through restarts and updates.

### Changed

- The belt buttons on Live and the HUD now say what they do in plain words under each codename: pause, resume, start and end walk.
- An empty Programs page now says there are no programs yet and offers New and ECHO on History.
- Programs on the dashboard and the HUD now say when the treadmill is not connected, with a Connect button.

### Fixed

- The app no longer gets stuck connecting or freezes its windows when Bluetooth stops answering after the PC wakes from sleep. Each Bluetooth step now gives up after a few seconds and the app tries again.
- When the treadmill stops sending its readings while the link still looks connected, the app now reconnects by itself after about 10 seconds. Reconnecting never starts the belt. A belt that is already moving carries on.
- The HUD can now only read the app state, send its own controls and set its click area. Only the dashboard can change settings, delete walks or talk to the treadmill directly.
- Connecting to Strava now checks that the browser sign-in belongs to the Connect you started and ignores any other link while it waits.
- The Live page's program card now fits the dashboard while a program waits for GO, with or without the step map.
- When Windows Credential Manager fails, the Strava login is no longer saved in the app database. The Strava section says the login could not be saved securely.
- Database backups no longer keep a copy of the Strava login. Older backups are cleaned at startup.
- Security fix in a library the app uses to read web addresses.

## 1.54.2 (2026-11-22)

### Changed

- Updates are now signed with a new, password-protected key. Install this version before the next one: later updates only verify with the new key, so a copy that skips this version needs one manual reinstall.

## 1.54.1 (2026-11-22)

### Fixed

- INSTALL on the HUD's update card opens the dashboard on the update notes again, and "What's new" stays open after an update.

## 1.54.0 (2026-11-22)

### New

- A COOL_DOWN step can end on heart rate: after its minutes the step walks on until your pulse holds at the target for 30 s.
- A step set to COOL_DOWN in the program editor gets a pulse target of 60 % of your max heart rate, the same for every cool-down. Untick the box to keep a cool-down timed.
- Change the pulse target per cool-down step, as a share of your max heart rate or in bpm.
- The seeded VO2 max now ends its cool-down on heart rate. A VO2 max you never edited gets the target and a new note, and programs you made or changed stay as they are.
- PULSE_RATE on the HUD and the Live page shows the target while the cool-down runs, and the program line reads `pulse to 114` while the step waits on the pulse.
- With no heart rate reading, the cool-down step ends on its minutes as before.

## 1.53.0 (2026-11-22)

### New

- STEP_MAP on the Live page lists every step of the running program, with the current step marked and finished steps dimmed, and charts speed and incline across the whole program with a yellow line at the current point of the program.
- STEP_MAP shows each step's kind, minutes, speed, incline and heart zone. A repeating program shows its cycle and the rest between cycles. A step with no end runs until END.
- STEP_MAP takes the place of the stats while a program runs. MAP on the program card switches between the two.
- The HUD shows the next step under the current one, e.g. `NEXT // REST 3/5 · 3 min · 3.5 km/h`.

## 1.52.0 (2026-11-22)

### New

- Program steps can have a kind: WARM_UP, WALK, INTERVAL, REST or COOL_DOWN. Pick one per step in the program editor.
- The HUD, the Live page and the tray menu show the running step's kind, counted within that kind, before the step number: `INTERVAL 3/5 · step 6/11`. Steps without a kind show as before.
- The example programs get kinds, so VO2 max and Interval walk show WARM_UP, INTERVAL, REST and COOL_DOWN as they run. A program you edited keeps its steps as they are.
- Changing only a step's kind keeps an example program's note, as a rename does.

## 1.51.0 (2026-11-22)

### New

- ECHO on a walk in History saves the walk as a new program named `ECHO // <date>`. Each steady stretch of speed and incline becomes one step, up to 12 steps, with the speeds in calibrated km/h.
- The ECHO program is only saved, never started. Open in Programs under the ECHO button goes straight to the editor, and the ECHO program runs from Programs like any other.
- A walk under 3 minutes, or one with no steady minute at one speed, gets a short message instead of a program.

## 1.50.4 (2026-11-22)

### Fixed

- The belt service reminder now sits in the Live page header, so the page no longer scrolls when the reminder shows.

## 1.50.3 (2026-11-22)

### Fixed

- The VEL and TILT steppers in the HUD now fit the panel at the largest Windows text sizes.

## 1.50.2 (2026-11-22)

### Fixed

- The HUD's STASIS and DUMP buttons no longer stick out past the VEL and TILT controls at larger Windows text sizes.

## 1.50.1 (2026-11-22)

### Fixed

- The HUD header no longer spills past its edge at a narrow width or a larger text size.

## 1.50.0 (2026-11-22)

### New

- STRIDE_FIT in System, Calibration: set the fastest speed your walking stride still fits the deck, by feel. Each treadmill keeps its own, and it stays off until you save one. USE CURRENT fills in the belt's speed.
- The program editor flags any step faster than STRIDE_FIT with one line under the step. The step's speed stays as you set it.
- The Live page speed slider marks STRIDE_FIT with a FIT tick. The slider still goes to the deck's top speed.
- The HUD shows an ABOVE STRIDE_FIT line while the belt runs faster than STRIDE_FIT. The line stays until the belt slows, with no toast.
- Heart rate zones respect STRIDE_FIT. When a zone needs more effort at STRIDE_FIT, the incline goes up instead of the speed, up to 2 % more and never past the deck's top level. Easing off, the speed comes down to STRIDE_FIT first, then the extra incline.
- FIT_TO_DECK on the Programs page: a seeded program you haven't edited, with steps faster than STRIDE_FIT, can run those steps at STRIDE_FIT with more incline. The new incline keeps about the same effort by the ACSM walking equation. The confirm dialog shows every change first and says when the deck's top incline costs effort. FIT_TO_DECK never starts the program.

## 1.49.1 (2026-11-22)

### Changed

- The old calibration panel under ADVANCED // POINT_EDITOR is gone, with its belt logo count and hand point editor. GROUND_TRUTH in System, Calibration is now the only way to calibrate.
- A point's row in GROUND_TRUTH shows the factor the app really uses when a stored one is outside the limits.
- The belt loop field starts empty for a treadmill with no measured loop, instead of filling in an old app-wide length.

## 1.49.0 (2026-11-22)

### New

- New WALL_CLOCK card on the History page shows which hours of the day you usually walk. Each hour lights up by how many of your last 14 desk days had a walk in that hour, so days away from the PC never dim the card.

## 1.48.2 (2026-11-21)

### Fixed

- The HUD fits its readouts at larger Windows text sizes: a long distance (up to 123.45 km) keeps every digit, with the unit moving under it when space is tight, and the running strip, program buttons, speed and tilt steppers and the neural flow line wrap instead of running off the panel.
- Neon Strider now checks for updates every hour, so new versions show up and install by themselves without pressing check.

## 1.48.1 (2026-11-21)

- Remove the unused ERGO HUD toast type; Add a feature lookup script and check it before building

## 1.48.0 (2026-11-21)

### Changed

- The "session saved" HUD toast now also shows the walk's average speed and today's total distance.

## 1.47.0 (2026-11-21)

### New

- Right-click the tray icon to run the treadmill without the HUD. The menu shows the belt's state and the walk's time, ENGAGE or STASIS, DUMP, speed and incline steps, the running program's GO or HOLD, SKIP and END, your saved programs under RUN PROGRAM, and Open dashboard, Show HUD or Hide HUD, and Quit. Items that can't do anything right now are greyed out. A left click still opens the dashboard.

### Fixed

- The speed slider on the Live page no longer pauses a moving belt. Its lowest setting is the slowest speed, and only Pause or Stop take the belt to 0.
- Speed down presses on a belt at its slowest speed keep it there instead of pausing it.
- The first speed up press on a still belt goes straight to the slowest speed, and the next press one step above it.

## 1.46.0 (2026-11-21)

### Changed

- Heart rate zone steps now wait 2 minutes of walking for your pulse to settle before they trim the speed, and wait again after a pause and GO. PULSE_RATE shows `ZONE // SETTLING` meanwhile.
- ZONE_TILT in System, Labs (off by default) lets a heart zone step trim the incline instead of the speed, or both, chosen per step in the program editor. Incline moves one deck level at a time, within 2 % of the step's incline.

## 1.45.0 (2026-11-21)

### New

- GROUND_TRUTH has START FRESH: the saved points go aside and the guided walk starts from none. RESTORE brings them back if you give up on the new walk, and KEEP NEW deletes the old set once the new walk is done.

### Changed

- Calibration points are kept per treadmill. Your current points stay with the treadmill saved in the app, and a second treadmill starts uncalibrated, so it gets the "Calibrate this treadmill" offer.

## 1.44.0 (2026-11-21)

### New

- The HUD page can switch the Alt + Insert minify hotkey off, or move it to Alt + Pause or Alt + Scroll Lock, so it stops clashing with other apps such as JetBrains IDEs.

## 1.43.3 (2026-11-21)

### Fixed

- Security updates to the network and database libraries.

## 1.43.2 (2026-11-21)

### Fixed

- Engage pressed just after a status poll no longer sends the treadmill a second start.
- A status code the app doesn't know no longer ends the walk mid-session or counts the same belt time twice. While the treadmill shows one, the app sends it no automatic start, resume or speed change.

## 1.43.1 (2026-11-21)

### Fixed

- Neon Strider is now built with a different linker, which removes a build warning. The app itself works the same.

## 1.43.0 (2026-11-21)

### New

- New program, Interval walk: a 5-minute warm-up, then five rounds of 3 brisk minutes at 6.0 km/h and 5 % and 3 easy minutes at 3.5 km/h, 35 minutes in all. It is easier than VO2 max and meant for most days of the week. Every step keeps the belt moving, so one press runs the whole walk.

### Changed

- Micro-break folds into Workday. Workday is 5 easy minutes every 30 by default. Set the walk length with the step minutes and the spacing with Repeat every, for example 2 minutes every 20.
- An unedited Micro-break that nothing uses is removed. A Micro-break still picked on the sit-too-long card, run last or running stays as Micro-break. Programs you edited stay as they are.
- The notes under Workday, Micro-break, Post-meal, Focus, Brisk and VO2 max are reworded to read more plainly. A program you edited keeps its note.

## 1.42.0 (2026-11-21)

### New

- PULSE_RATE shows a program step's heart rate zone on the HUD and the Live page: the range in bpm, and whether your pulse is under, in or over it.
- The minified HUD strip shows PULSE_RATE and its spark chart while a heart rate sensor is picked.

## 1.41.10 (2026-11-20)

### Changed

- GROUND_TRUTH's guided walk checks only speeds the treadmill runs. When the treadmill skips the halfway speed between two calibrated ones, the check moves to the nearest speed the treadmill runs and the walk says so.
- A GROUND_TRUTH check the treadmill ran at another speed shows that speed on its chip and its line, matching the note.

## 1.41.9 (2026-11-20)

### Changed

- The install confirmation is now one short sentence naming the update, with the details on a smaller line below.

## 1.41.8 (2026-11-20)

### Fixed

- Fixed history showing the wrong "Yesterday" label for an hour after a daylight-saving clock change.

## 1.41.7 (2026-11-20)

### Fixed

- A pause on the treadmill's own buttons now pauses the running program too, so GO picks the step up where it stopped.

## 1.41.6 (2026-11-20)

### Fixed

- Changing calibration points while the belt runs no longer changes its speed. The new calibration applies at your next speed change or program step.

## 1.41.5 (2026-11-20)

### Fixed

- The HUD and the eye-break cards now fit when Windows text size is larger.

## 1.41.4 (2026-11-20)

### Changed

- Speed toasts on the HUD no longer fire when a steady treadmill wobbles by 0.1 km/h; at some speeds they used to, at others not.
- Treadmill settings saved outside the ranges in System, Treadmill (speeds, top incline level, incline per level and offset) now load at the nearest end of the range, so the belt is never asked for more than 25 km/h.

## 1.41.3 (2026-11-20)

### Changed

- Shorter, clearer text across the dashboard: each setting has a one-line description, with the details in a second line under it.
- Messages from Strava, GHOST_POD, WRIST_LINK, heart rate and calibration now name what they talk about.

## 1.41.2 (2026-11-20)

### Fixed

- A speed down press at the slowest speed no longer leaves the next speed up press doing nothing.
- The log now says why a speed press did not reach the treadmill.

## 1.41.1 (2026-11-20)

### Changed

- GROUND_TRUTH asks for the fastest setting you'll walk at as the treadmill shows it, with a stepper that starts at the deck's top and offers only speeds it runs.

### Fixed

- GROUND_TRUTH's RUN now resumes a belt paused at the treadmill, even at the speed it paused at, and starts a stopped one.
- A calibration run the treadmill ran at another speed than RUN asked is saved at the speed it ran, and the guided walk moves its slowest or top speed there and says so.
- While a calibration run is open, its card with TAP and STOP sits at the top of the Calibration section and scrolls into view at each new phase.
- During a guided walk, the walk's RUN is the one button on; the point rows hide their own RUN and ADD RUN.

## 1.41.0 (2026-11-20)

### New

- GROUND_TRUTH is out of Labs and always in System, Calibration. It asks one question, the fastest you'll walk, then times the deck's slowest speed and that top speed, and checks the speed halfway between. A check more than 2 % off becomes a point of its own and each half is checked again. The summary names the speeds calibrated and the error left. Every speed waits for your RUN.
- A treadmill that connects with no calibration gets a "Calibrate this treadmill" card. SKIP is remembered for that treadmill.
- The first GAIT_SYNC stride at a speed now replaces the stride a belt run borrowed, and later strides average only with measured ones.
- Saving a point mid-run no longer nudges the belt off the run's speed.
- The target list and RUN ALL are under ADVANCED // TARGET_LIST, and the old point editor under ADVANCED // POINT_EDITOR.

## 1.40.0 (2026-11-20)

### New

- GROUND_TRUTH: after the belt is timed at a speed, GAIT_SYNC measures your stride. Tap each footstep, left and right, with TAP, Space or Alt+Space; it ends by itself once the steps agree, saves the stride for that speed with UNDO, and never moves the belt.

## 1.39.0 (2026-11-20)

### New

- Set your max heart rate in System, Heart rate; program heart zones are a percentage of it (190 bpm until you set one).
- The program editor can give any walking step a heart zone, typed as % of max or in bpm. The zone trims the step's speed by up to 1 km/h to hold your pulse and never starts the belt.
- VO2 max now holds each 4-minute climb at 85 to 95 % of your max heart rate while a heart rate sensor is reading; without one the climbs run their set speed as before. A VO2 max you have edited is left as it is.

## 1.38.0 (2026-11-20)

### New

- LABS // DECK_SWEEP can sweep incline levels too: Sweep incline levels steps a walking belt through every incline level DECK_PROFILE has not learned yet, up to your Highest incline level (or the treadmill's own top level once learned), then puts the incline back. It never starts or resumes the belt, and stopping the belt, changing the speed or incline, a press on the treadmill's remote or starting a program ends it.
- Stop sweep now puts the belt back where it was before the sweep: at the speed it started at after a speed sweep, at the incline it started at after an incline sweep.
- PULSE_RATE draws a spark chart of the last 10 minutes of heart rate under the bpm, on the HUD and the Live page. A heart rate zone step shows its range as a faint band behind the line.

## 1.37.0 (2026-11-20)

### Changed

- GROUND_TRUTH saves a measured point by itself, averaging it into the point at that speed. The result says so in one sentence and offers UNDO, which puts the point back as it was before the run, and TAP AGAIN. SAVE POINT, AVERAGE IN and Discard are gone.

## 1.36.0 (2026-11-20)

### Changed

- GROUND_TRUTH times the belt by tapping: tap TAP, Space, or Alt+Space from any window each time the mark on the belt passes, and the run ends by itself once the times agree. Missed or doubled taps are dropped, and the result says in plain words how far the deck's display is off.
- GROUND_TRUTH asks the belt loop's length once for each treadmill, with a short guide to measuring it. The loops-and-steps form is gone.

## 1.35.0 (2026-11-20)

### New

- During a GROUND_TRUTH run or sweep the dashboard stays open and on top, even when you click another window, so you can tap and watch it while walking. A LOCKED ON TOP marker shows while it holds, with UNPIN to let it go; it lets go by itself when the run ends, after 30 minutes, or when the app restarts.

## 1.34.1 (2026-11-20)

### Fixed

- WRIST_LINK: while it is on, the PC also sends a short Bluetooth beacon the BELT data field can find, and the field reads the foot pod's advertisement byte by byte. A dropped link no longer unpairs the PC on the watch.

## 1.34.0 (2026-11-20)

### New

- History: an opened walk saved with heart rate now draws it on the chart as a yellow line with its own BPM axis, with a gap where the watch had no reading.

## 1.33.0 (2026-11-20)

### New

- History CSV export now has average and maximum heart rate columns (bpm) at the end, empty for walks without a heart rate reading.

## 1.32.0 (2026-11-20)

### New

- LABS // DECK_SWEEP in System -> Diagnostics, off until you switch it on: one press steps a walking belt through every speed DECK_PROFILE has not learned yet, up to your Top speed, then back to your speed. It never starts or resumes the belt, and stopping the belt, changing the speed or starting a program ends it.

## 1.31.1 (2026-11-20)

### Fixed

- GHOST_POD keeps the watch's foot pod link when WRIST_LINK or the power switch is turned on or off. Before, either switch restarted the pod, and the watch lost the foot pod until it was reconnected by hand.
- The log names each GHOST_POD service with its UUID and says in words what Windows did with its advertisement. It also records every watch or phone that connects, subscribes or disconnects.
- A heart rate sensor that is not broadcasting yet, such as a Garmin watch outside an activity, now shows HEART_LINK // WAITING with what to do, instead of FAULT. It links by itself once the watch starts broadcasting. FAULT is kept for real errors such as Bluetooth being off.

## 1.31.0 (2026-11-20)

### New

- Programs can now hold a heart rate zone with a linked heart rate sensor: the belt speed is trimmed by 0.1 km/h at most every 20 s, up to 1 km/h either side of the step's speed, to keep your pulse in the zone. It only trims a belt that is already running and never starts it. Setting zones up comes in a later release.

## 1.30.0 (2026-11-20)

### New

- DECK_PROFILE learns incline levels too: it files the incline the deck echoes or refuses, keeps the incline stepper, program steps and settings to levels the deck takes, and shows what it learned in System -> Diagnostics.

## 1.29.0 (2026-11-20)

### New

- System -> Treadmill offers DECK_PROFILE's floor and ceiling in place of looser min and max speed settings (USE DECK LIMITS), and has a new Slowest speed field. A limit you tightened yourself stays as you set it.

## 1.28.0 (2026-11-20)

### New

- When the deck changes or refuses a speed or tilt you set on the Live page, the DECK_ADJUSTED or DECK_REFUSED note now shows under that control, as it does on the HUD.

### Changed

- DECK_PROFILE is now kept per treadmill: each deck's learned speeds are saved under its Bluetooth identity and switch with the connection, so a second treadmill no longer starts from the first one's speeds. The profile learned so far goes to the treadmill you connect first.

## 1.27.0 (2026-11-20)

### New

- Walks saved with heart rate now send it to Strava, in the automatic upload and in the file from "Save for Strava". Walks without heart rate upload as before.

## 1.26.0 (2026-11-20)

### New

- With a heart rate sensor linked (HEART_LINK), each walk now saves its heart rate every second. History shows the walk's average and maximum heart rate.

## 1.25.0 (2026-11-20)

### New

- Labs, GROUND_TRUTH: add a target speed with ADD_SPEED or remove one, so the list matches the speeds you walk at. The list is saved with your settings, and RUN ALL walks it. A saved speed outside the treadmill's range stays listed but is skipped.

## 1.24.0 (2026-11-20)

### New

- WRIST_LINK (Labs, off by default): the PC serves a second Bluetooth service next to GHOST_POD so a Connect IQ data field on the watch can read the belt state. This is the link test only; nothing the watch sends moves the belt yet.

## 1.23.0 (2026-11-20)

### New

- OCULAR_IMPLANT breaks now guide full blinks: close and hold, squeeze gently, open, 2 seconds each, 15 times in a 90-second break. The HUD card shows each step as `SHUTTER // CLOSE`, `SQUEEZE` and `OPEN`, and with Tones on each step has its own soft beep. The timing comes from a blinking-exercise trial; the sources are in docs/research.md.

## 1.22.0 (2026-11-20)

### New

- Heart rate (HEART_LINK): pick a Bluetooth heart rate sensor, such as a Garmin watch with Broadcast Heart Rate on, in System -> Heart rate. It is off until you pick one and runs apart from the treadmill link. Heart rate shows as a new stat, PULSE_RATE, on the HUD and dashboard. It is not saved with sessions or sent to Strava yet, and it never moves the belt.

## 1.21.0 (2026-11-20)

### New

- DECK_PROFILE: the app learns which speeds the treadmill runs from its replies while you walk. The HUD stepper, the Live slider, the program editor and the speed settings then keep to speeds the deck takes, and its real slowest and fastest speeds. System -> Diagnostics can forget what it learned.

## 1.20.0 (2026-11-20)

### Changed

- GROUND_TRUTH: ADD RUN times a saved speed again and averages the new run into the saved point, with every run weighing the same, and each row shows how many RUNS it holds. Editing a point by hand starts its count over at one.

## 1.19.0 (2026-11-20)

### New

- GROUND_TRUTH, the guided calibration, can now work from a step count alone for a belt with no mark to count: tick NO_MARK, enter your steps, and it sets the stride while the distance factor stays as it is.

## 1.18.0 (2026-11-20)

### New

- GHOST_POD can send running power to your watch (System -> Labs, GHOST_POD power, off by default). The PC also shows up as a Bluetooth power sensor, and the watts come from the same estimate as METABOLIC_BURN, so walking on an incline reads as more power. A paused belt sends 0 W. Your watch may only take running power from a Stryd: that's the first thing to try.

## 1.17.0 (2026-11-20)

### New

- OCULAR_IMPLANT eye breaks fit into a running program's rests (the rest between cycles, a 0 km/h step, or a wait for GO). A break due while a program walks waits for its next rest when that rest is near, and a short rest takes a break that is nearly due, so a walk is paused less often. Switch it under System -> Desk with "Take breaks at program rests" (on by default).
- Eye breaks also stop counting while the PC's display is off.

## 1.16.0 (2026-11-20)

### New

- Labs, GROUND_TRUTH: RUN ALL walks every calibration speed in turn with a progress strip, and each speed waits for your RUN. The run panel has its own STOP.

## 1.15.3 (2026-11-20)

### Fixed

- After a speed or incline change on the treadmill's remote or its own buttons, the HUD, shortcut and dashboard + and - now step from what the deck is running, and a press of one no longer puts the other back to the value from before the remote.

## 1.15.2 (2026-11-20)

### Fixed

- Calibration points outside the factor limits (0.2 to 3.0) are refused in the old wizard, the point editor and when saving, with the reason shown. A point stored outside the limits shows the value that is applied.

## 1.15.1 (2026-11-20)

### Fixed

- The HUD note after the deck refuses or changes a target now shows tilt in percent, like the TILT stat, and writes KM/H in capitals both times.

## 1.15.0 (2026-11-20)

### New

- The HUD now reads the treadmill's echo of each speed or incline you set. When the deck took something other than what was asked (it lifts speeds under its floor, say) or refused it, a short `DECK_ADJUSTED` or `DECK_REFUSED` note shows under the VEL and TILT controls for a few seconds, and the asked-versus-got values go to the log.

## 1.14.0 (2026-11-20)

### New

- OCULAR_IMPLANT breaks now pause the belt if you are walking (a running program pauses at its step). When the break is over the HUD shows OCULAR_IMPLANT // RECALIBRATED: RESUME starts you walking again, STAY STILL keeps the belt paused. It never restarts by itself. Turn off "Pause the belt for breaks" in System -> Desk to keep walking through them.

## 1.13.0 (2026-11-20)

### New

- Labs: a guided calibration wizard, GROUND_TRUTH, off until you switch it on in System, Diagnostics. It lists several speeds; RUN sets the belt to one, you count belt loops (and steps, for your stride) between COUNT and DONE, and the app works out that point. REPEAT re-times just one saved point, and stride (20 to 200 cm) and factor (0.2 to 3.0) ranges are wider.
- Labs: GHOST_POD, a switch (off by default) in System -> Labs that makes the PC a Bluetooth foot pod for your watch. It sends the belt speed, cadence and distance once a second, and 0 km/h while the belt is paused so the watch's Auto Pause can stop its timer. Pausing on the watch doesn't pause the belt yet.

## 1.12.0 (2026-11-20)

### New

- New DESK_SYNC card on the History page: "Walked on 9 of your last 10 desk days." A desk day is a day you used the PC, so days away from it never count against you. Days you walked before this update count as desk days.

## 1.11.0 (2026-11-20)

### New

- Eye breaks (off by default, under System -> Desk): after 20 minutes of screen use the HUD warns a minute ahead, then asks you to look into the distance for 90 seconds, with a blink tone every 10 seconds and a tone at the end. The belt is never touched. Both times and the tones can be changed.

### Fixed

- Calibration no longer saves a 0 km/h point when the belt has already stopped; it tells you to run it again.

## 1.10.0 (2026-11-20)

### New

- New *VO2 max* example program: a Norwegian 4x4 for the deck (warm-up, four 4-minute climbs at 6.0 km/h and 10 % with 3 easy minutes between, cool-down). It is added to your programs, and docs/research.md has the sources.

## 1.9.1 (2026-11-20)

### Fixed

- The slowest speed now follows your calibration: a speed set under the belt's real floor (e.g. 1.2 km/h when the deck's 0.6 runs that fast) runs at the floor instead of below it, and programs, the slider and the engage speed all use the same calibrated minimum.

## 1.9.0 (2026-11-20)

### New

- LIGHTS_OUT (System, Desk, off by default): when the PC's display turns off, the treadmill stops and any program ends, so the deck can go to sleep and its lights go out. Nothing starts again when the display comes back on.
- When an update is waiting, the pink dot now shows on the tray icon and on System's Updates tab as well as on the System page, and the tray tooltip ends in `UPDATE READY`. All three clear together when you install it or choose "Not now".

### Fixed

- While the treadmill is off, the app keeps retrying quickly (every 15 seconds) as long as you are using the PC, and slows down to every 2 minutes only once the PC has been idle for 5 minutes, so it no longer fills its log. Using the PC again, Connect, Retry now or scanning brings fast retries back.
- The HUD speed buttons no longer get stuck partway (around 3.8 km/h with some calibrations): every press now moves the belt one step, all the way up to your max speed and back down.

## 1.8.1 (2026-11-20)

### Changed

- The HUD nudge now says how long the belt has been idle (for example 32 MIN IDLE) instead of assuming you are seated, and its setting is called Idle nudge.

### Fixed

- INSTALL on the HUD's update card now reliably opens the dashboard on System -> Updates, instead of the window sometimes vanishing again before you could see it.

## 1.8.0 (2026-11-17)

### New

- Minify the HUD while you walk: the new button on its top row (or Alt + Insert) shrinks it to a strip with speed, time, distance, the program step, STASIS and DUMP. It stays that way, after a restart too, until you expand it again. Cards that need an answer still show.

## 1.7.0 (2026-11-17)

### New

- Two new example programs: Micro-break (2 minutes at 3.2 km/h every 20 minutes) and Brisk (30 minutes at 5.0 km/h). They are added after your own programs, unless you have deleted all of them or already have one with that name.

### Changed

- The example programs and the sit-too-long nudge now follow current research, with sources in docs/research.md.
- Post-meal walks at 5 % instead of 10 %, and Focus at 2.0 km/h on the flat instead of 2.5 km/h at 5 %. If you never edited them they change; an edited program is left alone.
- The sit-too-long nudge comes after 30 minutes instead of 60. If you never changed it, it moves to 30; a time you set yourself is kept.
- Each example program shows one sentence on why it is built that way. Renaming keeps it; changing the steps removes it.

## 1.6.1 (2026-11-17)

### Fixed

- The HUD now collapses to its strip when the treadmill is switched off while the app still shows the link as up, instead of staying expanded on a stale RUNNING status.

## 1.6.0 (2026-11-16)

### New

- **Updates install by themselves**: once the belt has been still with no program for 10 minutes and the PC has had no input for 5, Neon Strider installs a waiting update and reopens. It checks again right before installing, so it never closes mid-walk. Switch it off under System -> Updates.

### Changed

- Neural flow sensitivity follows realistic typing rates: Medium starts easing off at 90 keys a minute (steady writing) and reaches the slowest speed at 260 (an average typing burst). Light typing, about a key a second, no longer slows the belt.

### Fixed

- Neural flow: once you stop typing, the belt is back at full speed about 12 s after your last key, however fast you were typing.
- Neural flow: the slowest speed is reached again on small belts and floors (a 90% floor at 0.2 km/h steps stopped one step short).
- A program or speed started after a stop now reaches its speed, even when it is the same speed as last time; it used to stay at the treadmill's own start speed (1.2 km/h).

## 1.5.1 (2026-11-16)

### Fixed

- The HUD strip shows today's distance in full, even past 100 km, and the link status (such as `RELINK 12S`) is no longer cut off.

## 1.5.0 (2026-11-16)

### Changed

- The sit-too-long card on the HUD now leads with ENGAGE, which starts a plain walk just like the HUD's Engage button; your saved programs follow it. With the treadmill off, the card says to switch it on (a program you pick still waits for it, as before).
- The card has one way to hide it: SNOOZE 15 MIN, which brings it back in 15 minutes and keeps counting the time you've sat. LATER and DISMISS are gone.

## 1.4.3 (2026-11-15)

### Changed

- Sending raw commands from System -> Diagnostics now asks in the app's own confirm box instead of a plain Windows dialog.

### Fixed

- The command cycler in System -> Diagnostics now sends proper treadmill frames (with checksum), lets you pick which byte cycles, shows the exact bytes it will send, and no longer double-sends when auto-step starts.

## 1.4.2 (2026-11-15)

### Changed

- Engage on a treadmill that is off or asleep now says so straight away, instead of trying for 8 seconds. Switch it on, then press Engage; it never starts by itself when it wakes.
- The belt maintenance reminder is now every 200 km or 3 months, the figure on the treadmill's sticker (Jogwell's manual says 100 miles), and is shown and edited in km.
- If you never changed the distance interval it moves to 200 km; an interval you set yourself is kept.

### Fixed

- Dump no longer flashes "starting, running" while the belt slows down: the HUD shows SPINNING DOWN, then STOPPED.
- Engage, Dump and the end of a workout no longer send the treadmill extra start, stop or pause commands it was refusing.

## 1.4.1 (2026-11-15)

### New

- **Record treadmill traffic** (System, Diagnostics, TRAFFIC_TAP): saves every command sent to the treadmill and everything it sends back, with timestamps, to a file in the log folder. It only listens, is off each time the app starts, and stops by itself after 4 hours or 20 MB. These recordings will drive a treadmill simulator for testing.

### Fixed

- Neural flow now brings the belt back up when you type less, even if you still type a few keys now and then: it starts within about 20 s and reaches full speed about a minute after you stop, instead of waiting for 45 s of near silence.

## 1.3.2 (2026-11-15)

### Changed

- Updated the Rust libraries the app is built on to their newest compatible versions.
- This fixes 12 of 14 known security advisories in those libraries (OpenSSL, bytes, time and an XML parser). Two remain, in the HTTP library's h2 and in the bundled SQLite, until those libraries get a larger upgrade.

## 1.3.1 (2026-11-15)

### Fixed

- Today's total no longer jumps down when you stop a walk that ran past midnight. A walk now counts toward the day it started, so after midnight today's total leaves it out instead of counting it and then dropping it.
- When the treadmill goes to sleep with its screen off, the HUD no longer shows `STATE ?6` and the Live page no longer says the treadmill state is unknown. It shows as standby, like any stopped belt.

## 1.3.0 (2026-11-15)

### New

- **HUD during stasis**: when the belt is still, the HUD can shrink to a small strip (link
  state, today's total, expand and Engage) or hide until the belt moves again. Cards that need
  an answer (walk-away, update, maintenance, a program waiting for GO) still show. HUD -> When
  the belt is still; the default shows everything, as before.

## 1.2.2 (2026-11-15)

### Changed

- Updated the app's JavaScript libraries within their current major versions (React 19.3.1, Tailwind CSS 4.3.5, Framer Motion 12.43, Zustand 5.1), and the security audit of the libraries shipped in the app reports no known issues.

## 1.2.1 (2026-11-15)

### Fixed

- Lowering the speed to zero during a program now pauses the program, so RE-JACK resumes the step. Before, the belt started and stopped again a few seconds later.

## 1.2.0 (2026-11-15)

### New

- **Walk-away safety**: locking the PC pauses the belt. After 10 minutes (2 to 30) with no
  keyboard or mouse input, the HUD shows `WALK-AWAY // PAUSING IN 30s` with STAY, then pauses.
  It never resumes by itself: RESUME or END on the HUD. Not while an app uses the microphone.
  System -> Desk.
- **Live tray readout**: the tray tooltip shows speed, distance, today's total and the running
  program; the icon is lit while the belt runs and grey when the treadmill is offline.
- **Drag the HUD** by its NEON STRIDER logo. It snaps to screen edges and corners, moves to
  another monitor, and remembers the spot. Double-click the logo, or Reset on the HUD page, to
  send it back to its corner.

### Changed

- **Typing-aware pace (Neural flow)** reworked: counts key presses only (not the mouse or
  shortcuts), eases off gradually after 10 s of steady typing, recovers after about 45 s
  without typing, and changes speed at most every 15 s. Works on manual walks as well as
  programs. Two settings: Slowest speed (default 90%) and Sensitivity. Floors under 90% move to
  90%. The HUD shows `NEURAL_FLUIDITY // 94%` while it is easing off.
- The HUD finds its monitor by position and size as well as name, so it stays put when a driver
  update renames a display.

### Fixed

- Engage did nothing, silently, when the treadmill's screen was off. It now says so under the
  button after 8 seconds and stops re-sending the start. Unknown treadmill status codes are
  shown and logged instead of being treated as idle.

### Security

- npm security fixes within the current major versions: zod 4.6.7 (memory exhaustion on
  oversized input), vite 7.3.8, postcss 8.5.33, vitest and @vitest/ui 4.1.12, plus patched
  transitive packages. `npm audit --omit=dev` is clean; 7 moderate lint-only advisories remain
  in typescript-eslint's minimatch, which has no fixed 8.x release yet.

### New

- **Auto-update**: Neon Strider checks GitHub when it starts and once a day. System -> Updates
  shows the new version's release notes, including every version you skipped, with Install and
  Not now (Check now looks at once); the HUD shows `UPDATE READY // v1.3.0` once per version.
  Install downloads and verifies the signed update, then closes, installs and reopens by
  itself; "What's new" shows the notes once after the update.
- Installing an update never pauses or stops the belt. The treadmill keeps going while the app
  is closed for a few seconds (the HUD and shortcuts are unavailable then, so use the
  treadmill's own controls if needed); after the reconnect the walk carries on as the same
  session, and a running program carries on at the same step with its timer on time.
- The belt never starts by itself after a restart, reconnect or update: a program that finds
  the belt still comes back paused and waits for GO.
- GitHub releases now carry that version's notes from the changelog.
- The HUD's link label (RELINK, BT OFFLINE, LINK FAILED) is now a button while the treadmill is offline: click it to reconnect to your saved treadmill straight away instead of waiting for the next retry. It reads LINKING… while it works, and a failure shows right under it.
- With no treadmill saved yet, the same label opens the dashboard on its connect dialog, so you can pick one straight from the HUD.

### Changed

- Neon Strider is now open source under the MIT license, with a new README. Calibration now starts empty on new installs; existing calibration points are kept.
- A program never starts a still belt by itself. After each rest of a repeating program (and
  after a 0 km/h step), the next walk waits for you: the HUD shows
  `REST COMPLETE // PRESS GO` with the GO button lit, the dashboard's program card says the
  same, and one toast marks the moment. The step timer waits too, and the program stays ready
  until GO, SKIP or END. A belt that kept moving through the rest carries on as before.
- Engage (and Alt + Home) on a paused program is now its GO, instead of starting the belt at
  the slowest speed for a moment; the button reads GO. Like Engage, GO says so if the
  treadmill doesn't answer.
- A program waiting for GO survives the treadmill switching itself off and reconnecting.
- SKIP during a rest still starts the next walk straight away: that press is the go-ahead.
- A program waiting for GO, resting or paused is kept across an app restart for up to 4 hours
  (a walking one still for 10 minutes), so one left waiting over lunch is still there.
- The sit-too-long card stays back while a program waits for GO; the time still counts.
- Starting the belt with the treadmill's own buttons or remote while a program is paused or
  waiting for GO counts as GO: the program carries on with its step and sets the step's speed
  on the moving belt, instead of the app pausing the belt again. This works after a walk-away
  pause too (the same PC lock doesn't pause it again). During a rest it becomes a manual walk,
  as before, now also in the first seconds of the rest.
- When a start at the treadmill resumes a paused or waiting program, the HUD shows one toast,
  `DECK START // WORKDAY RESUMED` (with the program's name). GO in the app and a start at the
  treadmill during a rest stay as they were.

### Fixed

- When the PC's Bluetooth sees no devices at all, the app says so (turn Bluetooth off and on, or restart) instead of asking whether the treadmill is switched on.
- Ending a paused program never moves the belt; you press Engage when ready. END on a program
  that is paused, waiting for GO or on a 0 km/h step (and SKIP on its last step, and a program
  that finishes or rests on a still belt) leaves the belt still; Engage or RE-JACK then starts
  it at the speed and incline you had before the program. Ending a program while the belt is
  moving still goes straight back to your earlier speed.
- After a program ends on a still belt, the target speed on the HUD and the dashboard's Live
  page shows the speed Engage will start the belt at (say 3.5) instead of 0.0. Nothing moves
  until you press Engage; nudging the speed steps from that number and starts the belt, as
  setting a speed does on any still belt.
- After a program ends on a still belt, changing the tilt before you press Engage (a nudge,
  the slider or a tilt preset) sets the tilt Engage starts at, the one the HUD shows, and
  still moves nothing. Before, Engage went back to the tilt from before the program.
- A pause could turn into a start if the treadmill reported it had paused between the two packets of the pause command. The app now decides pause or start once per command.
- The System page no longer scrolls by itself while a background status (a Strava retry countdown, a failed update check) is showing.
- A tilt calibration change (incline multiplier or offset) now applies at once: the HUD, the dashboard, programs and the walk's recorded incline use it straight away instead of after the next reconnect. The treadmill's incline doesn't move when you change it; only the percentage shown for it does.

## 1.1.0 (2026-11-13)

### New

- **Weekly summary**: last week against the week before (distance, time, active days,
  sessions, calories), plus this week so far, with a HUD card you can dismiss.
- **Strava: indoor walks.** Every upload is marked as an indoor (trainer) walk.
- **Strava: shoes.** Pick a pair from your Strava profile (System → Strava); it goes on every
  upload. The list is cached and refreshes on connect or with Refresh.
- **Strava: mute from feed.** Hide every treadmill walk from your home feed (the default),
  only walks under a set number of minutes (default 20), or none.
- **Strava: title and description templates** with placeholders (`{distance_km}`,
  `{duration}`, `{avg_speed}`, `{avg_incline}`, `{calories}`, `{steps}`, `{walks}`, `{date}`,
  `{time}`), a live preview and Reset to default. The History upload and the TCX preview use
  them too. New default titles read `NEON STRIDER // 3.42 KM UPLINK`.
- **Strava: skip duplicates.** If another app already put the same walk on Strava (a walk, run
  or hike, or any trainer activity, starting within 5 minutes), nothing is uploaded and the
  walk is marked as on Strava.

- **Sit-too-long nudge**: after 60 minutes (15 to 240) of PC use without walking, a HUD card
  (`STASIS_ALERT`) with one-tap buttons for up to three programs, LATER (snooze 15 minutes)
  and DISMISS. If the treadmill is off, the program you pick is remembered and starts when it
  connects (within 15 minutes). Walking 2 minutes, or being away for 15, resets the count.
  Counts only the time of the last keyboard or mouse input. System → Desk.
- **Slow down on calls**: when an app starts using the microphone while the belt runs, it
  eases to a quiet speed (default 2.0 km/h; never speeds up) with a
  `CALL DETECTED // QUIET MODE` toast, and goes back 10 seconds after the call ends
  (`CALL ENDED // RESTORED`) unless you changed the speed. Works with running programs. System → Desk.
- **Lifetime odometer**: total distance and belt hours across every session, and the date
  tracking began, in System → Maintenance, plus an `ODO` HUD widget (`LIFETIME_ODOMETER`).
- **Maintenance by belt hours**: count the service interval in belt hours (default 50) instead
  of distance. Distance (Jogwell's 100 miles) stays the default.

- **Link and Strava status you can see.** The treadmill link shows what it is doing: SCANNING,
  LINKING 2/3, RECONNECT 12s (with Retry now), NO DATA (linked but the deck sent nothing for
  10 s: "Treadmill asleep?"), BT OFF, LINK FAILED, with the last error in the tooltip. The
  connect dialog shows scanning, "No treadmills found", "Bluetooth is off" and a per-device
  error with Retry. The HUD header reads `RELINK 12S`, `NO SIGNAL`, `BT OFFLINE`,
  `LINK FAILED`. Strava shows its step (Preparing, Sending, Strava is processing, Adding
  shoes), "Waiting for you to approve in the browser (4:12 left)" with Cancel, the 15-minute
  back-off countdown with Retry now, and a status pill in the section header. History rows
  spin while uploading and show why a failed upload failed.
- **Errors appear next to the button that caused them.** Buttons show a working state at once
  and put the result (or the failure) on a line under themselves: Strava Connect, Sync now,
  Refresh shoes, template save, History upload, the connect dialog, Live Retry now, Engage and
  Dump, and the maintenance fields. Rejected HUD commands show a short `SYS // ...` line inside
  the card that holds the button. Anything else that is rejected raises a toast instead of
  failing silently. Background Strava failures also raise a toast.
- **Strava errors in plain words.** Strava's error replies are turned into one sentence
  (never raw JSON), with what to do: reconnect, check your internet, wait for the rate limit.
  An inactive API application (Strava now requires a subscription on the account that owns
  it) pauses auto-sync instead of retrying every 15 minutes, and offers Open Strava API
  settings.
- **Save for Strava**: no API needed. In History (per walk) and System → Strava (latest walk)
  it writes the walk's TCX file, with your title and description templates, to
  `Downloads\Neon Strider\<date> <title>.tcx`, opens strava.com/upload/select and shows the
  file in Explorer. The walk is marked as saved, with Mark as on Strava once you have uploaded it.
  Works with no account connected.

### Changed

- Strava now asks for two more permissions (`activity:read`, `profile:read_all`) for the
  duplicate check and the shoe list. Existing connections keep uploading and muting, but show a
  notice until you disconnect and connect again.

## 1.0.0 (2026-11-13)

The first release. Compared with the 0.1 prototype, almost everything was rebuilt.

### New

- **HUD overlay** rebuilt: one legible panel (no text under 11 px) in any screen corner and on
  any monitor, with live stats, start/pause/stop, speed and tilt steppers, and program buttons.
  Clicks pass through everywhere except the panel.
- **Dashboard** with five pages (Live, Programs, History, HUD, System) replacing the old
  three-column layout, settings modal and separate archive window. It reopens where you left
  it, and the Live page fits without scrolling.
- **Saved programs**: named routines (seeded with Post-meal, Workday and Focus) that start
  immediately, pause, skip and end, and put the treadmill back the way it was when they
  finish. Programs survive an app restart.
- **Background Strava sync** with token refresh, merged walks, an optional daily rollup and a
  manual upload button. Credentials live in Windows Credential Manager.
- **Belt maintenance reminder** on Jogwell's schedule (every 3 months or 100 miles,
  editable), shown on the HUD until you mark the belt serviced.
- **Calories** from the ACSM walking equation using your body weight, speed and incline.
- **Pin to HUD** from any stat on the Live page.
- **History**: year calendar, inline speed/incline replay, CSV export.
- **Start with Windows** (optional), single instance, per-user installer (no admin).
- **Logs** in a rotating file (System → Diagnostics → Open log folder).
- **Daily database backups** (newest 14 kept) and a copy before any database upgrade.
- New app and tray icon.

### Fixed

- Auto-reconnect never ran after a dropped Bluetooth link.
- Pressing DUMP re-sent the stop command forever and starved status polling.
- Programs and automation ticked twice, so timed steps ran short.
- Resuming at the same speed after a pause didn't restart the belt.
- Strava uploads were a single point (sample timestamps were zeroed), auto-sync never fired,
  and logins expired after 6 hours.
- Session averages were saved as 0 or a single reading; existing sessions are repaired once.
- Incline was recorded as a hardware level but read as a percentage.
- A walk started from the treadmill's own buttons could be attached to an older session.
- Global shortcuts were applied twice while the dashboard was open.
- The dashboard closed on blur even when pinned.
- The HUD panel could be pushed off-screen by a running program.

### Changed

- Rust now owns all app state; the windows are views of one snapshot and send typed commands.
- Neural flow counts activity only while a program runs with it enabled, whether or not the
  dashboard is open. It never records which keys are pressed.
- The webview runs under a strict content security policy, and each window gets only the
  permissions it uses.
