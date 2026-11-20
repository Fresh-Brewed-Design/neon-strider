# WRIST_LINK

A Connect IQ data field on the watch reads the belt from the PC (roadmap #82; the plan and
research are in the scope page `docs/connect-iq.md` on branch `improve/2026-11-19-connect-iq-scope`).
It builds on GHOST_POD (watch-pod.md): the PC serves a second Bluetooth service next to the foot pod.

## Phase 0 (built): the link test

Labs -> WRIST_LINK (needs GHOST_POD on; off by default). The PC serves service
`00b165d6-40c2-4f94-8df9-0f69431f21af` with one characteristic, **State**
(`00b165d6-40c2-4f94-8df9-0f69431f21b0`, read and notify, PC to watch, little endian):

| Bytes | Field |
|---|---|
| 0 | `belt`: 0 still, 1 moving, 2 paused (3 waiting for GO comes with phase 2) |
| 1-2 | `speed_kmh_x100` (u16) |
| 3 | `incline` |
| 4 | `flags`: bit 0 = the app holds its automatic start |
| 5 | `last_seq` (0 until commands exist) |
| 6 | `result` (0 until commands exist) |

State is sent once a second. The log (`GHOST_POD:` / `WRIST_LINK:` lines) records subscriptions,
reads and advertisement status. Nothing the watch sends is read, so nothing can move the belt.

The State UUID is this PR's choice; the plan only fixed the service UUID. Command and Workout step
come with phases 1 and 2.

### How the PC serves it

- **One Windows provider per service.** The foot pod (RSC), power and WRIST_LINK each have their own
  `GattServiceProvider`. Switching WRIST_LINK or power adds or removes that one provider, and the
  foot pod keeps advertising and keeps its watch. (Before, the switch restarted every service, and
  the watch lost its foot pod link until it was reconnected by hand.)
- **WRIST_LINK is not in the advertisement.** On the first hardware test Windows aborted it. A
  check on the PC (`cargo test peripheral_providers_are_independent -- --ignored`, plus probes)
  showed why: an advertised 16-bit service reports Aborted and then Started a moment later (the
  foot pod does this every time), but an advertised 128-bit service reports Aborted and never
  starts. The adapter supports extended advertising, but a discoverable Windows advertisement also
  carries the PC's name, and the flags, a 128-bit UUID and an 8-letter name already fill the
  31 bytes of a legacy advertisement. So WRIST_LINK is served discoverable and not connectable.
  Microsoft's GATT server guide says Windows puts a service's UUID in the advertisement only when it
  is both. Microsoft's BluetoothLE sample says discoverable is what lets a connected device find
  the service. The field connects through the foot pod's RSC advertisement and finds WRIST_LINK
  once connected. The phone test below checks that.
- **What the log says** (`GHOST_POD:` lines). It shows the adapter's advertising limits, and each
  service by name and UUID with its advertisement status in words and its Bluetooth error.
  `WRIST_LINK (00b165d6-...) status after start: Created` is normal for a service that isn't
  advertised. A central shows up as `central <address> connected (first seen: ...)` at its
  first read, write or subscription (Windows has no event for a bare connection), and
  `central <address> disconnected (...)` when it leaves. Each subscription change logs the count,
  the characteristic, the service and who is subscribed.

### Test without the watch (phone, nRF Connect)

nRF Connect for Mobile is free from Nordic Semiconductor (Android or iPhone).

1. In the app, switch on Labs, GHOST_POD and WRIST_LINK. On the watch, turn the foot pod sensor off
   for now, so the phone is the only device connecting.
2. In nRF Connect, open Scanner and scan. The PC shows under its Bluetooth name, advertising
   Running Speed and Cadence (`0x1814`). The WRIST_LINK UUID is not in the advertisement.
3. Tap Connect. The log should show `central <phone address> connected`.
4. On the phone, the service list should hold Running Speed and Cadence and
   `00b165d6-40c2-4f94-8df9-0f69431f21af`. **If the `00b165d6` service is missing, stop here**: the
   watch can't find it either. Send the log lines from switching WRIST_LINK on to the end.
5. Open `00b165d6-...`, read the `...21b0` characteristic (seven bytes), and turn its notifications
   on (the triple arrow). The log should show `1 subscribed to 00b165d6-...21b0 on WRIST_LINK`.
6. Start the belt. Byte 0 goes to 1 and bytes 1-2 follow the speed (little endian, km/h x 100).
7. Switch WRIST_LINK off and on in the app. The phone loses only the `00b165d6` service; the
   Running Speed and Cadence service and the phone's connection stay. Then disconnect the phone.

### Test on the watch

`watch/` is the data field (Monkey C; it builds with SDK 9.3.0 for the Venu X1). Build it with
`monkeyc -f watch/monkey.jungle -d venux1 -o watch/bin/NeonStriderBelt.prg -y <developer key>`
(the key is kept out of the repo), copy the `.prg` into `GARMIN\Apps` over USB, and add the
**BELT** field to the Treadmill activity's data screen.

What the field shows:

| Text | Meaning |
|---|---|
| `SCAN` | Looking for the PC. |
| `LINK W` | Matched an advertisement with the WRIST_LINK UUID and asked the watch to connect. |
| `LINK R` | Matched the foot pod's RSC advertisement (the usual one, since WRIST_LINK isn't advertised) and asked the watch to connect. |
| `NOCONN` | A LINK waited 20 s without connecting. The field dropped that pairing and scans again; it goes back to `LINK` at the next match. |
| `NOSVC` | Connected, but the device has no WRIST_LINK service. |
| `PAIRERR` | The watch refused the pairing outright (no free connection, for example). |
| `OK`, then the speed | Connected and reading State. |

### Why the watch stayed on LINK (best hypothesis, November 2026)

The first hardware test never got past `LINK`, with the foot pod sensor on or turned off. Two
things are likely, and the next test tells them apart:

1. **The watch's own foot pod pairing claims the PC's address.** A Connect IQ app can't open its
   own link to a sensor the watch has paired natively. A Garmin forum answer says a sensor "can
   only be connected natively or with a CIQ app, not both", and in another thread a
   `pairDevice` that never answered started working once the watch's existing pairing with that
   device was removed. Turning the sensor off keeps the pairing, which fits the field staying on
   `LINK`. `pairDevice` has no timeout of its own: the docs only say the BLE subsystem
   "will begin to search for the device", which is why the field now gives up after 20 s.
2. **The match was an advertisement Windows had already aborted.** The field matched whatever came
   first, and the WRIST_LINK advertisement was aborted. That can no longer happen, since WRIST_LINK
   isn't advertised, and `LINK R` or `LINK W` shows which one matched.

If removing the foot pod from the watch's sensors lets the field reach `OK`, (1) is the cause, and
Fresh Brewed Design picks the fallback from the plan: the field as the only link (it can record the
belt's speed and distance as its own FIT fields, but the watch's native distance then comes from
the wrist), a phone relay, or a second Bluetooth adapter on the PC. Connect IQ 5.1's sensor pairing
(`SensorDelegate`) might let the field stand in for the native foot pod; that is unexplored.

Sources: Garmin forums,
[a sensor can be connected natively or with a CIQ app, not both](https://forums.garmin.com/developer/connect-iq/f/discussion/414732/can-a-connect-iq-data-field-connect-and-communicate-with-a-running-sensor-that-is-already-bonded-with-the-watch),
[onConnectedStateChanged never called until the existing pairing was removed](https://forums.garmin.com/developer/connect-iq/f/discussion/332580/onconnectedstatechanged-is-never-called-after-a-ble-pairing-request/1771006),
[pairDevice returns a device and no callback comes (20 to 30 s)](https://forums.garmin.com/developer/connect-iq/f/discussion/443722/bledelegate-after-pairdevice-returns-a-valid-device-no-onconnectedstatechanged-is-called-back);
Garmin, [BluetoothLowEnergy API](https://developer.garmin.com/connect-iq/api-docs/Toybox/BluetoothLowEnergy.html);
Microsoft, [Bluetooth GATT server](https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/gatt-server),
[GattServiceProviderAdvertisementStatus](https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.genericattributeprofile.gattserviceprovideradvertisementstatus),
[GattServiceProviderAdvertisingParameters](https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.genericattributeprofile.gattserviceprovideradvertisingparameters),
[BluetoothLE sample, server scenario](https://github.com/microsoft/Windows-universal-samples/blob/main/Samples/BluetoothLE/cs/Scenario3_ServerForeground.xaml.cs).
