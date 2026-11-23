# FitShow Protocol Specification (Reverse Engineered)

This protocol was derived from hardware testing. The implementation is in `src-tauri/src/protocol.rs` (packet encoding and decoding) and `src-tauri/src/link.rs` (the treadmill link state).

## Packet Structure
- **Header**: `0x02`
- **Tail**: `0x03`
- **Checksum**: XOR of all bytes from index 1 (inclusive) to Tail (exclusive).
- **Total Length**: 15 bytes for control, variable for status polls.

## Command Subtypes (Byte 1)
- `0x51` (`CMD_STATUS`): Polling / Status. The PC sends the request (`status_request`); the machine answers with a telemetry frame (`parse_status`).
- `0x53` (`CMD_CONTROL`): Control (PC -> Machine), built by `control_packet`.

## Telemetry Payload (0x51 Response)
| Byte Index | Field | Unit | Logic |
|------------|-------|------|-------|
| 3 | Raw Speed | 0.1 km/h | `val / 10.0` |
| 4 | Incline | Level | `val` |
| 5,6 | Time | seconds | `(L) | (H << 8)` |
| 7,8 | Distance | meters | `(L) | (H << 8)` |
| 9,10 | Calories | 0.1 kcal | `((L) | (H << 8)) / 10.0` |
| 11,12 | Steps | Count | `(L) | (H << 8)` |

## Control Subtypes (Byte 2 of 0x53)
These are the variants of `protocol::Control`.
- `0x01` (`Ready`): Ready to Start
- `0x02` (`Target`): Target Control (Speed = idx 3, Incline = idx 4)
- `0x09` (`Start`): Start/Resume
- `0x0A` (`Pause`): Pause
- `0x03` (`Stop`): Stop

## Status Byte (Byte 2 of 0x51)
From traffic recordings of the Jogwell Ares 11 Elite (`MachineStatus` in brackets):

| Byte | Meaning | Engine |
|------|---------|--------|
| `0` | Stopped, idle. Short frame `02 51 00 51 03`. | Stopped |
| `1` | Workout ended: belt still, totals still shown, for about 3 s, then `0`. Seen after a stop and when a pause ends. | Stopped (the session is saved here) |
| `2` | Countdown after a start or resume. Short frames `02 51 02 <n>`, n = 3, 2, 1, about a second apart, then `3`. | Starting |
| `3` | Running. | Running |
| `4` | Stopping: the belt winds down after a stop (speed falling to 0), then `1`. | Stopping |
| `6` | Standby: screen off, or switched off with the remote. Frame `02 51 06 02`. The deck stays connected and keeps answering polls. | Stopped |
| `10` | Paused. The belt winds down in this status too (speed falling to 0). | Paused |

Recorded sequences: Engage `0, 2, 2, 2, 3`; pause `3, 10`; resume `10, 2, 2, 2, 3`;
stop `3, 4, 4, 4, 4, 1, 0`; a pause that ends (the deck's timeout or a stop at the
treadmill) `10, 1, 0`.

The deck answers READY (`02 53 01`) with `02 53 01 03` and starts its countdown;
the Start sent after it is refused. While counting down (`2`) or stopping (`4`)
it refuses a second READY, Start, Stop or Pause, so the app sends nothing but
polls then.

A refused control packet is answered with the nack `02 53 53 03`. Accepted ones
are echoed (`02 53 03 50 03` for a Stop, `02 53 02 <speed> <incline> ...` for a
target). In standby (`6`) the deck nacks every READY and Start: the app can't
wake it, so Engage (or GO, a program, a speed set by hand) sends nothing and
says the treadmill is off or asleep at once. Once it wakes (`6` to `0`) nothing
starts by itself; the user presses Engage again.

Anything else is unknown, `5` and `7` included (see the names below): the
engine keeps the last known status, so a walk carries on as one session and
nothing starts a new one on a guess (`OrchestratorState::take_status`). While
the byte lasts the app sends nothing automatic (no start, resume, pause or
target), only polls and a Pause or Stop the user asks for. The raw value is
kept, each new one is logged once (`Unknown deck status byte N`), and the app
shows "Treadmill state unknown (code N)".

## Capturing the standby state
Answered for the Jogwell Ares 11 Elite: when it goes to sleep (screen off) the
deck stays connected, keeps sending status frames, and reports byte `6`. The
log showed `deck status byte 6 (UNKNOWN), was 0` about 14 minutes after a stop,
and `deck status byte 6 (UNKNOWN), was none` on connecting to a sleeping deck.
It never appears during a walk. `status_from_byte` now maps `6` to stopped, so
the app no longer reports an unknown state while the deck sleeps.

Another deck may use a different byte. To check what yours sends:

1. Let the deck's screen turn off while Neon Strider is connected.
2. Press Engage. After about 8 s the app says the treadmill didn't respond.
3. Open the log: System, Diagnostics, **Open log folder**
   (`%LOCALAPPDATA%\com.freshbreweddesign.neonstrider\logs\neon-strider.log`).
4. Look for these lines around that time:
   - `deck status byte N (...)`: logged whenever the byte changes, including
     `(UNKNOWN)` values, with the previous value.
   - `no status frames for N s` / `status frames arriving again`: whether
     frames stop while the screen is off. After 10 s of silence the app also
     drops the link and reconnects (`No status frames for N s on a link that
     reads as up`), which sends the deck nothing. A write Windows refuses
     because it closed the link (`HRESULT(0x80000013)`, "The object has been
     closed.") drops it at once the same way (`Windows closed the treadmill
     link; dropping it to reconnect.`).
   - `Engage: no answer from the deck in 8 s`: the status, unknown code and
     time since the last frame when Engage gave up.

If a deck's screen-off state has a byte other than `6`, add it to
`status_from_byte` in `protocol.rs`.

## What the deck answers in standby
From a command cycler run on the Jogwell Ares 11 Elite in standby (`6`,
2026-11-15):

- All 256 control bytes `53 XX` got the bare echo `02 53 53 03`, the answer
  the deck gives to any command it does not handle.
- All 256 command bytes `XX 00` left the deck in standby (`02 51 06 02`).
- Handled in standby: `51` (status), `50 00` (device info, answered
  `02 50 00 30 00 f5 01 94 03`) and `60 00` (answered with its argument;
  its meaning is unknown).

## Names from qdomyos-zwift
The open-source qdomyos-zwift project publishes names for the FitShow status
byte and command families. They are recorded here as names only; no code is
taken from it.

| Status byte | Name there | In this app |
|-------------|------------|-------------|
| `0` | normal | Stopped |
| `1` | end | Stopped (workout ended) |
| `2` | start | Starting (countdown) |
| `3` | running | Running |
| `4` | stop | Stopping |
| `5` | error | unknown, not handled |
| `6` | safety | Stopped (standby on this deck) |
| `7` | study | unknown, not handled |
| `10` | paused | Paused |

Command families (byte 1): `0x50` to `0x54`. This app sends `0x51` (status)
and `0x53` (control); `0x50` is the device info the deck answers in standby.

Bytes `5` and `7` are still unhandled: the engine keeps the last known status
while one lasts (see "Status Byte").

## Traffic recordings
System, Diagnostics, Show diagnostics, **TRAFFIC_TAP: Record treadmill traffic**
writes every packet sent to the treadmill and every notification it sends
back (`src-tauri/src/traffic.rs`). Recordings go in
`%LOCALAPPDATA%\com.freshbreweddesign.neonstrider\logs\traffic\`, one file per
recording named after the local start time
(`traffic-2026-11-15_09-30-12.jsonl`). The switch is off at every start, and a
recording stops by itself after 4 hours or 20 MB (a walk makes about 2 MB an
hour: the status poll runs every 600 ms).

The file is JSON Lines (format 1), one object per line, `type` first:

```
{"type":"header","format":1,"app_version":"1.3.2","started_ms":1763195412000,"started_local":"2026-11-15T09:30:12+01:00"}
{"type":"tx","t_ms":600,"at_ms":1763195412600,"kind":"POLL_STATUS","hex":"02515103"}
{"type":"rx","t_ms":655,"at_ms":1763195412655,"uuid":"0000fff1-0000-1000-8000-00805f9b34fb","hex":"0251031e000c0100400005003a000003"}
{"type":"end","t_ms":3600000,"at_ms":1763199012000,"reason":"stopped","packets":11873}
```

- `t_ms`: milliseconds since the recording started (monotonic, for timings);
  `at_ms`: wall clock, epoch ms.
- `tx`: a packet written to the treadmill, logged once the write succeeded
  (failed writes are not in the file). `kind` is the engine's `PacketKind`
  label (`POLL_STATUS`, `CONTROL_SET`, `HANDSHAKE_START`, ...) or `RAW` for
  the command cycler. Polls are included.
- `rx`: every notification, whatever its characteristic (`uuid`), including
  frames the app could not decode.
- `hex`: the raw bytes, lowercase, no separators.
- `end`: `reason` is `stopped` (switched off), `duration_limit` or
  `size_limit`. A file without it ends where the app quit.

`traffic::TrafficLine` (serde) reads and writes these lines.
