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
Known values: `0` stopped, `1`/`3` running, `2`/`10` paused, `4` starting,
`6` asleep (screen off, treated as stopped).
Anything else is unknown: the engine keeps treating it as stopped (nothing
starts or ends a session on a guess), but the raw value is kept and the app
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
     frames stop while the screen is off.
   - `Engage: no answer from the deck in 8 s`: the status, unknown code and
     time since the last frame when Engage gave up.

If a deck's screen-off state has a byte other than `6`, add it to
`status_from_byte` in `protocol.rs`.

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
