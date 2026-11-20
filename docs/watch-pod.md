# Watch pod (GHOST_POD)

Roadmap #55 asks for the watch and the walking pad to pause and resume together. Today the app is
only a Bluetooth central: it talks to the treadmill and advertises nothing. This page is what a
Garmin watch can and can't do with a pod the PC pretends to be (checked November 2026), and what
the first slice builds.

## What a Garmin watch pairs with

- **Running Speed and Cadence (RSC, service 0x1814), the foot pod profile.** Garmin watches find
  a Bluetooth foot pod natively (Sensors, Add, Foot pod). Bridges that put a treadmill's speed on
  a Garmin all present themselves as an RSC foot pod for this reason (TreadLink, an ESP32
  bridge; blebridge does the same over ANT+ as a stride sensor).
- **Fitness Machine Service (FTMS, 0x1826), treadmill data and control point: not in stock
  firmware.** Garmin forum threads up to 2025 report no native FTMS support for treadmills on
  Forerunner and fenix watches. A Forerunner 955 doesn't even list an FTMS treadmill. FTMS
  treadmill data reaches a Garmin only through third-party Connect IQ data fields.
- So GHOST_POD advertises RSC. Its fields are instantaneous speed (1/256 m/s), cadence
  (steps per minute), stride length (1/100 m) and total distance (1/10 m), sent as a
  notification once a second. The layout is in the Bluetooth SIG RSC Service spec 1.0.1.

## Watch to pad (pause on the watch pauses the belt): not possible with RSC

- The RSC profile has no message from the watch to the pod for pause or resume. The only thing
  a watch can write is the SC control point: set the total distance, start a calibration,
  change the sensor location. None of them means pause.
- Pausing on a Garmin stops the activity timer on the watch. The watch keeps the sensor
  connected and keeps reading it, so there is no indirect signal either (a disconnect would also
  happen for many other reasons, like walking out of range, so it couldn't stand for pause).
- FTMS would have a way (Machine Status and the control point's start, stop and pause), but
  Garmin doesn't speak FTMS to treadmills, so it never sends them.
- GHOST_POD logs every control point write and every subscribe and unsubscribe, so a test on the
  real watch will show if it does anything we don't expect when paused.
- The way it could work later: a small Connect IQ data field on the watch. Data fields get the
  watch's timer pause and resume events and can use Bluetooth, so one could write "pause" and
  "resume" to a custom GHOST_POD characteristic. That same link is what #56 (running Garmin
  structured workouts on the pad) needs. A resume from it would count as a press on the watch,
  and it would still have to respect the belt-safety hold (`hold_auto_start`).

## Pad to watch (pausing the belt pauses the watch): possible

- While the belt is paused or stopped, GHOST_POD reports 0 km/h and 0 steps per minute.
- Garmin's Auto Pause stops the timer when speed drops below a set value (Settings, Activities,
  the run profile, Auto Pause, When stopped or Custom). On an indoor run the speed comes from
  the foot pod when its speed source is set to Indoor or Always, so a 0 from the pod should
  pause the watch, and the watch resumes by itself when the speed comes back. This needs
  checking on Jeremy's watch: not every Garmin offers Auto Pause on indoor run profiles.

## Running power (roadmap #77)

GHOST_POD's power switch (off by default) adds a second service beside the foot pod: Cycling
Power (0x1818), the profile a Stryd pod also serves. Windows advertises both services together.

- **What it sends.** Cycling Power Measurement (0x2A63) once a second: flags 0, then the
  instantaneous power as a signed 16-bit count of watts. Cycling Power Feature (0x2A65) reads 0
  (no optional fields) and Sensor Location (0x2A5D) reads 1, top of shoe.
- **Where the watts come from.** The ACSM walking equation METABOLIC_BURN uses
  (`engine::walking_kcal_per_min`): calibrated speed, calibrated incline (a downhill counts as
  level) and the weight in System. The rate in kcal per minute becomes watts (x 4184 / 60), times
  25 % for muscle efficiency, so the number lands near what a running power pod reports. At
  75 kg and 4 km/h that is about 66 W on the flat and about 145 W at 10 %. A paused, stopped
  or still belt sends 0 W.
- **Running speeds read low.** METABOLIC_BURN uses the walking equation at every speed, and the
  watts follow it. A running equation would change the calorie count too, so it is left for later.
- **Switching power restarts the pod**, since Windows serves a fixed set of services. The watch
  reconnects to the foot pod by itself.

### Will a Garmin take running power from a sensor that is not a Stryd?

Probably not natively (checked November 2026; not yet tried on Jeremy's watch):

- Garmin's native running power comes from the wrist or a Garmin running dynamics accessory.
  Forum threads on the Forerunner 955 and in the Connect IQ developer forum say third-party
  running power, Stryd included, is not used for the native Power field. Stryd's own help pages
  record Stryd power through its Stryd Zones Connect IQ data field, not the native field.
- A watch does list a Power sensor under Sensors & Accessories, Add New, and both ANT+ and
  Bluetooth sensors pair there. A Forerunner 965 thread reports that an external power meter
  on the Run profile worked, then stopped after a firmware update, while Stryd's Connect IQ
  fields kept working.
- So the likely outcome: the watch pairs GHOST_POD as a Power sensor, the native running
  Power field keeps showing wrist power, and a Connect IQ data field that reads a power meter
  (Stryd Zones or RunPower, for example) shows and records GHOST_POD's watts. Nobody reports
  trying a generic Bluetooth power sensor on a run yet, so the hardware test settles it.

## Windows

- The pod uses the Windows GATT server (`GattServiceProvider`), which needs an adapter that
  supports the peripheral role. The app checks `IsPeripheralRoleSupported` and says so in plain
  English when it doesn't.
- Windows advertises with the PC's own Bluetooth name; an app can't choose it (the GATT server
  has no name setting, and the advertisement publisher refuses a local name). So the watch can't
  list the pod as `NEON STRIDER`: it shows the PC's name. Renaming the PC (Settings, System,
  About) is the only way to change it, and that renames it everywhere.
- Most adapters can be a central (to the treadmill) and a peripheral (to the watch) at once,
  but not all. If the treadmill link drops when the pod starts, switch the pod off.

## Sources

- Garmin forums, "FTMS support is coming?" (fenix 7):
  https://forums.garmin.com/outdoor-recreation/outdoor-recreation/f/fenix-7-series/397753/ftms-support-is-coming
- Garmin forums, "Bluetooth FTMS support" (Forerunner 165):
  https://forums.garmin.com/sports-fitness/running-multisport/f/forerunner-165-series/380293/bluetooth-ftms-support
- TreadLink, FTMS treadmill to Garmin via RSC foot pod emulation:
  https://github.com/nathanmarlor/treadlink
- blebridge, FTMS treadmill to a Forerunner as an ANT+ stride sensor:
  https://github.com/roethigj/blebridge
- FTMS data fields for Garmin watches (Connect IQ, Bluetooth from a data field):
  https://absolutebollockscreations.com/apps/ftms/
- Garmin manual, Using Auto Pause:
  https://www8.garmin.com/manuals/webhelp/legacy/EN-GB/GUID-6D3747A8-EB3E-4C32-9C06-C8AC081D0C84.html
- Garmin manual, Setting the foot pod mode (Indoor, Always):
  https://www8.garmin.com/manuals/webhelp/tactix/EN-US/GUID-6C1F6323-5E34-4BB7-B776-49AE47FB3CD9.html
- Garmin forums, third-party running power meters not supported natively (Forerunner 955):
  https://forums.garmin.com/sports-fitness/running-multisport/f/forerunner-955-series/300113/why-are-3rd-party-running-powermeters-not-supported-for-native-running-power-stryd
- Garmin forums, Stryd power data (Connect IQ developers):
  https://forums.garmin.com/developer/connect-iq/f/discussion/424662/stryd-power-data
- Garmin forums, running power meter connection no longer working (Forerunner 965):
  https://forums.garmin.com/sports-fitness/running-multisport/f/forerunner-965/397429/running-powermeter-connection-no-longer-working
- Stryd help, Garmin compatibility, and third-party data fields:
  https://help.stryd.com/en/articles/6879256-garmin-compatibility-with-stryd and
  https://help.stryd.com/en/articles/6879633-stryd-and-third-party-data-fields-on-the-garmin-watch
- Garmin manual, pairing sensors (Forerunner 970):
  https://www8.garmin.com/manuals-apac/webhelp/forerunner970/EN-SG/GUID-C8997CD9-0A75-41A6-9052-86BCBF531A53-3451.html
- Bluetooth SIG, Cycling Power Service 1.1 (measurement, feature, sensor location).
- Bluetooth SIG, Running Speed and Cadence Service 1.0.1:
  https://files.bluetooth.com/download/rscs_v1-0-1
