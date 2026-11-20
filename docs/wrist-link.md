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

### Test without the watch

In nRF Connect (phone), scan for this PC, connect, open the `00b165d6-...` service, read State and
turn notifications on. Start the belt: byte 0 goes to 1 and bytes 1-2 follow the speed.

### Test on the watch

`watch/` is the data field (Monkey C, untested: no SDK was available when it was written). Install
the Connect IQ SDK, build it for the Venu X1 with the VS Code Monkey C extension (its own developer
key, kept out of the repo), copy the `.prg` into `GARMIN\Apps` over USB, and add the **BELT** field to
the Treadmill activity's data screen. With GHOST_POD paired and WRIST_LINK on, the field shows
`SCAN`, then `LINK`, then the belt speed. This answers the open risk: can the Venu X1 hold the foot pod link
and this one to the same PC? It also shows whether the PC's advertisement carries the 128-bit UUID
(if the field stays on `SCAN`, it does not, and phase 0 finds the PC another way, for example by its name).

If it fails, Jeremy picks the fallback from the plan (phone relay, drop the native foot pod, or a
second Bluetooth adapter).
