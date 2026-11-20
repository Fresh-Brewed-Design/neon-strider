# Connect IQ watch app (scope)

Roadmap #55 wants a pause or resume on the watch to pause or resume the belt. #56 wants a
Garmin interval workout to drive the belt's speed, and to see the workout. Jeremy also asked
about speed and incline from the watch, and which kind of Connect IQ app this should be. This
page is what Garmin's docs and forums say (checked November 2026), what fits, and a phased plan.
Nothing here is built yet. GHOST_POD, the foot pod the PC already serves, is in watch-pod.md.

## Recommendation

- **Build a Connect IQ data field first, added to the watch's own Treadmill (or Indoor Run)
  activity.** It is the only app type that runs inside a native activity, so the native timer,
  the native structured workout and the GHOST_POD foot pod all keep working, and the data field
  is told about each of them. Why: it covers #55 and #56 with the least new code on the watch.
- **Talk to the PC over Bluetooth, to a new service the PC serves next to RSC**, if the watch
  allows a second link to the same PC (the first slice tests exactly this). Why: sub-second,
  no phone, no internet.
- **Speed and incline buttons on the watch are a separate, later choice.** A data field can't
  take button or touch input, so buttons mean either a lap-button trick or a full watch app
  that records its own activity (see the decisions below).

## The Venu X1

- Connect IQ API level 6.0, 448 x 486 AMOLED, rectangular
  ([compatible devices](https://developer.garmin.com/connect-iq/compatible-devices/)).
- Touch first, with no menu button: `onMenu` never fires; apps use
  `View.setActionMenuIndicator()` and a swipe instead
  ([forum](https://forums.garmin.com/developer/connect-iq/f/discussion/423197/venu-x1-application-menu-controls),
  [bug report](https://forums.garmin.com/developer/connect-iq/i/bug-reports/ciq-documentation-does-not-make-it-clear-that-new-touch-first-devices-vivoactive-6-venu-x1-and-venu-4-do-not-support-onmenu-and-do-require-the-use-of-view-setactionmenuindicator-if-the-dev-wishes-to-implement-a-context-sensitive-menu)).
- The API reference lists the Venu X1 as supported for each of these:
  - `Toybox.BluetoothLowEnergy` (since API 3.1.0). Usable from data fields, watch apps,
    widgets, glances and background. The watch is the central; at most 3 registered profiles;
    "This pairing does not persist across application instances", so the app scans again each
    time it starts ([BluetoothLowEnergy](https://developer.garmin.com/connect-iq/api-docs/Toybox/BluetoothLowEnergy.html)).
  - `Toybox.Sensor`, usable from data fields and watch apps; since 5.1.0 an app can also take
    part in the native sensor pairing menu (`SensorDelegate`)
    ([Sensor](https://developer.garmin.com/connect-iq/api-docs/Toybox/Sensor.html),
    [SensorDelegate](https://developer.garmin.com/connect-iq/api-docs/Toybox/Sensor/SensorDelegate.html)).
  - `Toybox.ActivityRecording`: watch apps only (and background, glance), not data fields
    ([ActivityRecording](https://developer.garmin.com/connect-iq/api-docs/Toybox/ActivityRecording.html)).
  - Workout steps: `Activity.getCurrentWorkoutStep()` and `getNextWorkoutStep()` (API 3.2.0)
    return the step the native workout is on: duration type and value, target type, target low
    and high, intensity (warmup, active, rest, cooldown, recovery, interval)
    ([Activity](https://developer.garmin.com/connect-iq/api-docs/Toybox/Activity.html),
    [WorkoutStep](https://developer.garmin.com/connect-iq/api-docs/Toybox/Activity/WorkoutStep.html)).
    The docs say they throw "if called from a data field app"; Garmin acknowledged that as a doc
    error and developers confirm they work in data fields
    ([bug report](https://forums.garmin.com/developer/connect-iq/i/bug-reports/api-doc-for-getcurrentworkoutstep-and-getnextworkoutstep-confusingly-says-exception-thrown-if-called-from-a-data-field-app),
    [forum](https://forums.garmin.com/developer/connect-iq/f/discussion/238571/has-anyone-used-the-activity-workoutstep-details-available-in-3-2)).
  - `Toybox.Complications` (API 4.2.0): lets watch faces and glances show values other apps
    publish. Not needed here; at most a later "belt state" glance.
- Not found on Garmin's site (the device page is script-rendered): the data field memory limit
  for the Venu X1. The SDK's device file (`simulator.json`/`compiler.json`) has it.

## App types against the needs

| Need | Data field | Watch app | Widget or glance | Background |
|---|---|---|---|---|
| Runs during the native Treadmill activity | Yes | No, it is its own activity | No | No |
| Pause and resume in sync with the watch timer | Yes, timer callbacks | Its own timer only | No | No |
| Read the steps of a Garmin workout the watch runs | Yes | No (the native workout isn't running) | No | No |
| Records with the GHOST_POD foot pod | Yes, the native activity does | Its own session; foot pod use untested | No | No |
| Buttons and touch for speed and incline | No input at all | Yes | Yes, but not during an activity | No |
| Draw the workout | In its own field slot | Full screen | Glance only | No |
| Bluetooth to the PC | Yes | Yes | Yes | Yes, but only in short timed runs |

- **Data field.** Gets `onTimerStart`, `onTimerStop`, `onTimerPause`, `onTimerResume`,
  `onTimerLap`, `onWorkoutStarted` and `onWorkoutStepComplete`
  ([DataField](https://developer.garmin.com/connect-iq/api-docs/Toybox/WatchUi/DataField.html)).
  On a Garmin the Start/Stop press is `onTimerStop` and `onTimerStart`; Auto Pause is
  `onTimerPause` and `onTimerResume`. That split matters: only the presses may move the belt.
  Two traps from the docs: `onTimerStart` runs "immediately after startup" if the timer is
  already running when the field loads, and `onTimerPause` likewise. Neither is a press.
  It can also write custom FIT fields (belt speed, incline) that Garmin Connect charts.
- **Watch app (device app).** Full buttons and touch, its own `ActivityRecording` session and
  its own timer, and it shows up in the activity list like any Connect IQ app. It can't see a
  Garmin structured workout, so #56 would have to run Neon Strider programs instead.
- **Widget or glance.** Good for a quick remote outside an activity; can't run during one.
- **Background service.** Short timed runs (minimum five minutes apart); no use for live control.
- **Native activity type.** A third party can't add a native activity profile. The nearest
  things are a watch app (its own entry in the activity list) or a data field added to the
  native Treadmill profile. A watch app's FIT sport and sub-sport can say running/treadmill.
- **Native distance can't come from an app.** FIT fields an app writes with `nativeNum` don't
  replace the watch's own distance or speed in Garmin Connect; Garmin says this is by design
  ([forum](https://forums.garmin.com/developer/connect-iq/f/discussion/4854/fitcontributor-nativenum-functionality/1551751),
  [request](https://forums.garmin.com/developer/connect-iq/i/bug-reports/please-allow-nativenum-fit-fields-to-override-native-fields-in-garmin-connect)).
  So the native foot pod pairing stays the way the activity gets the belt's distance.

## How the watch and the PC talk

| | BLE to a PC service | Phone relay | Foot pod only |
|---|---|---|---|
| Path | Watch to PC directly | Watch, Garmin Connect app on the phone, HTTP to the PC | RSC as today |
| Latency | One BLE connection event (well under a second, expected) | A web request through the phone, seconds | 1 s pod updates |
| Needs | PC serves one more GATT service | Phone nearby with Garmin Connect running, the PC reachable on the LAN, an HTTP listener and a firewall rule | Nothing new |
| Watch to PC (pause, steps) | Yes | Yes, as web requests | No, RSC has no such message |
| PC to watch (belt state) | Notifications | Only by polling | Speed and cadence |
| Reconnect | The field rescans by service UUID at each start and after a drop | Each request is independent | The watch handles it |
| Battery | A second small link at about 1 Hz; small | Phone and watch radios busier | As today |

- **The open risk: one watch, two links to one PC.** The native foot pod pairing is already a
  BLE link from the watch to the PC's address. Forum reports say a sensor is connected
  "natively or with a CIQ app, not both", and that a CIQ app can't pair a MAC address the watch
  is already paired with
  ([forum](https://forums.garmin.com/developer/connect-iq/f/discussion/414732/can-a-connect-iq-data-field-connect-and-communicate-with-a-running-sensor-that-is-already-bonded-with-the-watch),
  [forum](https://forums.garmin.com/developer/connect-iq/f/discussion/372687/multiple-ble-nrf52-devices-in-the-same-time-ciq/1800232)).
  Those reports are about ANT and other BLE devices, not a Windows GATT server, and Windows may
  advertise with a randomized address
  ([Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/390918/unable-to-connect-ble-device-with-ble-privacy-enab)).
  Nobody has reported this exact case, so phase 0 tests it before anything else is built.
- **Phone relay.** On System 7 and later devices a data field may call `makeWebRequest` in the
  foreground ([forum](https://forums.garmin.com/developer/connect-iq/f/discussion/427067/early-makewebrequest-in-a-data-field/1994468));
  the request goes through the phone ([Communications](https://developer.garmin.com/connect-iq/api-docs/Toybox/Communications.html)).
  It is the fallback if phase 0 fails. Whether the phone app allows plain HTTP to a LAN address
  is unverified.
- **Foot pod only** can't carry anything from the watch (watch-pod.md), so it is not an option
  for #55's watch-to-pad half or for #56.

## Safety rules on the watch link

- A Start/Stop press on the watch (`onTimerStart` after the field has been running, or after a
  `onTimerStop`) is a press in that moment, so a resume from it may start the belt and releases
  `hold_auto_start` the same way GO does.
- Never a press: Auto Pause and Auto Resume (`onTimerPause`, `onTimerResume`), any timer event
  that fires at field startup, anything sent while disconnected (the field drops it rather than
  replaying it on reconnect), and every workout step change.
- A workout step that would take a still belt to moving waits with `ProgramWait`, like a
  Neon Strider program step does, until a press: the watch's Start/Stop, the deck's remote,
  GO or Engage.
- Pausing on the deck's remote keeps working. The pod reports 0 km/h, Auto Pause stops the
  watch timer, the field sees `onTimerPause` and ignores it, so nothing echoes back.
- Pause from the watch is always allowed; stopping or pausing never needs the hold.

## PC side

- A second GATT service on the same `GattServiceProvider` set as RSC. Proposed UUID
  `00b165d6-40c2-4f94-8df9-0f69431f21af`, with three characteristics (all little endian):
  - **Command** (write with response, watch to PC): `[seq u8][op u8][arg u8]`. `op`:
    `0x01` pause, `0x02` resume (a press), `0x03` speed up, `0x04` speed down,
    `0x05` incline up, `0x06` incline down, `0x07` start from still (a press, only if Jeremy
    picks "watch start starts the belt"). `seq` lets the PC drop a repeated write.
  - **Workout step** (write, watch to PC): `[seq u8][index u8][intensity u8][duration_type u8]
    [duration_value u32][target_type u8][target_low u32][target_high u32]`, the current step,
    then the same 15 bytes for the next step if there is one. Values are Garmin's raw
    `WorkoutStep` numbers; Rust converts (speed targets are believed to be mm/s, which the
    forum hasn't confirmed, so phase 2 checks it against a known workout).
  - **State** (read and notify, PC to watch): `[belt u8][speed_kmh_x100 u16][incline u8]
    [flags u8][last_seq u8][result u8]`. `belt`: 0 still, 1 moving, 2 paused, 3 waiting for
    GO. `flags`: bit 0 hold set, bit 1 program waiting. `result`: 0 done, 1 refused by the
    hold, 2 no treadmill.
- Engine: watch commands map to the existing `DeckAction`s (`Pause`, `Start`, `SpeedUp`,
  `SpeedDown`, `InclineUp`, `InclineDown`). A watch workout becomes a program source, a new
  `TargetSource` variant (`Watch`), so the manual lockout and the program rules apply unchanged.
- Labs switch, off by default, under GHOST_POD (working name `WRIST_LINK`). It shows the link
  state and the last command, with errors next to the switch.
- Sim tests in `sim.rs`: watch pause on a moving belt; watch resume after a reconnect with the
  hold set; a startup `onTimerStart` ignored; Auto Pause not echoed; a workout step from 0 km/h
  to moving waits for GO; a deck-remote pause mid-workout; a repeated `seq` ignored.
- Without the watch, the PC service can be exercised from a phone with nRF Connect (free),
  writing the bytes above.

## Toolchain

Nothing is installed yet. All of it is free.

- **Connect IQ SDK Manager** for Windows, which downloads the SDK and device files (the Venu X1
  among them) ([SDK](https://developer.garmin.com/connect-iq/sdk/)). Garmin doesn't publish the
  download sizes; the device files are the bulk of it. Signing in with a free Garmin account is
  needed for the device downloads.
- **A Java runtime** for the Monkey C compiler, and **VS Code with the Monkey C extension**
  ([VS Code extension](https://developer.garmin.com/connect-iq/reference-guides/visual-studio-code-extension/)).
- **A developer key**, generated locally by the extension. No Garmin approval; it signs every
  build. Keep it out of the repo.
- **Sideloading:** copy the built `.prg` into `GARMIN\Apps` over USB (the watch shows up as an
  MTP device, and may hide the file once copied). Sideloaded apps are removed with Garmin
  Express, not the phone app. A firmware can reject a stale sideload; a new app UUID and file
  name fixed it in one report
  ([forum](https://forums.garmin.com/developer/connect-iq/f/discussion/367718/sideload-prg-file-no-longer-working),
  [forum](https://forums.garmin.com/developer/connect-iq/f/showcase/435218/forerunner-965-firmware-27-09-rejects-sideloaded-watch-app-with--400-sdk-sample-works)).
- **Connect IQ Store:** free with a developer account, goes through review; only worth it if
  the app is meant for others.
- **The simulator** runs the data field on a Venu X1 screen, plays back a FIT file, and can
  press start, stop and lap. Workout steps in it are unreliable per the forum. Its Bluetooth
  needs a Nordic USB dongle, which costs money, so BLE is tested on the real watch.
- **The code** would live in a new `watch/` folder in this repo (Monkey C, a `manifest.xml`
  with the BluetoothLowEnergy and Fit permissions), built by hand at first. CI can come later.

## Phases

1. **Phase 0, the link test (smallest useful slice).** PC: serve the new service with only the
   State characteristic, behind the Labs switch, and log connects. Watch: a data field that
   scans for the service, connects while the GHOST_POD foot pod is paired, and shows the belt
   speed it reads. Answers the one open risk. If it fails, Jeremy picks a fallback (decision 1).
2. **Phase 1, pause and resume (#55).** Add Command `0x01` and `0x02`, the safety rules above,
   the sim tests, and a short "belt paused" flash in the field when a command is refused.
3. **Phase 2, workout steps (#56).** Send the current and next step on `onWorkoutStarted` and
   `onWorkoutStepComplete`; Rust runs them as the `Watch` program source; the dashboard and HUD
   show the step, its target and what comes next. Belt speed and incline go into custom FIT
   fields.
4. **Phase 3, controls (only if Jeremy wants them).** Whichever of decision 2 he picks.

## Open questions

- Can the Venu X1 hold the native foot pod link and a CIQ link to the same PC? (Phase 0.)
- Does Auto Pause work on the Venu X1's Treadmill profile with the foot pod as speed source?
  (Already open in watch-pod.md; the pad-to-watch half depends on it.)
- The data field's memory limit on the Venu X1, from the SDK's device file.
- The unit of speed targets in `WorkoutStep` (mm/s is the forum's guess).
- Garmin running workouts have no incline target that we know of; a grade target type exists
  in the API, but whether Garmin Connect's run workout editor offers it is unchecked.
- A workout's later steps aren't readable, only the current and next, so the PC can't draw the
  whole workout ahead of time.
- Whether the Windows GATT server can fit the new 128-bit service UUID into its advertisement
  next to RSC, or the field has to find the PC another way (its name).
