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

- **The beacon.** While WRIST_LINK is on, the PC also sends a legacy advertisement (31 bytes at
  most) from Windows' advertisement publisher, with `UseExtendedAdvertisement` off. It carries one
  manufacturer data section: company `0xFFFF` and the bytes `4e 53 54 52 01` ("NSTR" and version
  1). On air that is `08 ff ff ff 4e 53 54 52 01`, 12 bytes with the flags Windows adds. Company
  0xFFFF is the Bluetooth SIG's ID for tests and devices without a company ID, and the four letters
  keep it apart from other test beacons. Windows refuses service data and UUID lists in a
  publisher's advertisement (they are reserved to the system), so manufacturer data is the only
  form left. The field matches `ff ff 4e 53 54 52` and ignores the version byte. Windows picks the
  beacon's address and whether it is connectable; the publisher has no setting for either, and
  it is not anonymous, so the PC's address is in it. The log says `WRIST_LINK beacon starting:
  legacy advertisement ...` and then `WRIST_LINK beacon Started` (or why not). A beacon that
  fails leaves the WRIST_LINK service up.
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
| `SCAN n` | Looking for the PC; `n` counts the scan results heard so far. |
| `LINK W` | Matched an advertisement with the WRIST_LINK UUID and asked the watch to connect. |
| `LINK R` | Matched the foot pod's RSC advertisement (0x1814, in Connect IQ's UUID list or in the raw bytes) and asked the watch to connect. |
| `LINK M` | Matched the PC's WRIST_LINK beacon (the marker below) and asked the watch to connect to that address. |
| `BLOCKED` | A LINK waited 20 s without connecting. The field stops there and keeps the pairing: unpairing dropped the watch's own foot pod link to the PC (hardware test, 2026-11-20). |
| `LOST` | Was connected and the link dropped. The pairing stays, so the watch reconnects by itself. |
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

### Why the field stayed on SCAN (hardware test, 2026-11-20)

With the foot pod forgotten on the watch, the field never matched. Its scan log (150 results in a
minute) held many devices, some at -90 dBm, but no result carried 0x1814, and no result was
longer than 31 bytes. A phone with nRF Connect did see the PC advertising 0x1814. The strongest
results were 31 bytes with no UUIDs at -46 dBm (most likely Windows' own beacon) and 8 bytes with
no UUIDs at -47 dBm. Three explanations fit, and the field now covers each one:

1. **The foot pod's advertisement is extended.** No result over 31 bytes suggests Connect IQ
   reports only legacy advertisements; Garmin's docs say nothing either way. Windows doesn't say
   which format a GATT server uses, and its advertising parameters have no legacy setting (Windows
   11 24H2 added secondary PHY settings, which exist only for extended advertising). Against it:
   on this adapter, a 128-bit UUID made the GATT advertisement abort, which an extended one would
   have room for. The beacon is legacy for certain, which is what `LINK M` tests.
2. **Connect IQ drops the UUID list.** A Garmin bug report (WERETECH-9123) says `getServiceUuids`
   and `getRawData` come back incomplete for some advertisements, and once when the 16-bit UUID
   list is the last section. The 8-byte result at -47 dBm could be the foot pod's advertisement
   (flags plus the 0x1814 list is 7 bytes). The field now reads the raw bytes for 0x1814 too, and
   logs the raw bytes in hex for every result at -60 dBm or stronger.
3. **The PC advertises from a private address.** Another Garmin bug report (CIQQA-4786, fenix 8
   and Instinct 3, acknowledged) says the Connect IQ scan drops connectable advertisements from a
   resolvable private address of a device the watch has no bond with. If Windows advertises the
   GATT server that way, the beacon may come from the same kind of address and be dropped too.
   The phone shows the address: one that changes between scans, or whose first digit is 4 to 7,
   is a resolvable private one.

What a check on the PC showed: the beacon starts on this adapter (`cargo test legacy_beacon_probe
-- --ignored --nocapture`: status Started). A PC doesn't hear its own advertisements, so the
probe couldn't show their format or address; the phone and the watch's log can.

What the next test tells apart:

- `LINK R` then `OK`: (2) was the cause; the raw bytes found 0x1814.
- `LINK M` then `OK`: the beacon works as the way in.
- `LINK M` then `BLOCKED`: the watch hears the beacon but can't connect through it. Windows
  sends it non-connectable or from another address; the log's hex and the phone's address show which.
- Still `SCAN`, and the log has no `match=M` line: the watch drops the PC's advertisements
  altogether, which points at (3).

If the field can't get in, the fallbacks: a second Bluetooth adapter on the PC for WRIST_LINK
alone (a USB dongle with its own address and legacy advertising), a phone relay, or the field
as the only link (it records the belt's speed and distance as its own FIT fields).

Sources: Garmin forums, bug reports
[fenix 8 BLE scan never returns advertisements from peripherals using a resolvable private address](https://forums.garmin.com/developer/connect-iq/i/bug-reports/fenix-8-ble-scan-never-returns-advertisements-from-peripherals-using-a-resolvable-private-address)
and
[Connect IQ BLE advertising layer parsing bug](https://forums.garmin.com/developer/connect-iq/i/bug-reports/connect-iq-ble-advertising-layer-parsing-bug-prevent-many-devices-to-function-properly-paired-or-not-paired-toybox-bluetoothlowenergy-scanresult-getrawdata-getservicedata-getserviceuuids-are-broken);
Microsoft,
[BluetoothLEAdvertisementPublisher](https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.advertisement.bluetoothleadvertisementpublisher)
(the reserved section types),
[UseExtendedAdvertisement](https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.advertisement.bluetoothleadvertisementpublisher.useextendedadvertisement)
(off by default),
[UseLowEnergyUncoded1MPhyAsSecondaryPhy](https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.genericattributeprofile.gattserviceprovideradvertisingparameters.uselowenergyuncoded1mphyassecondaryphy).
