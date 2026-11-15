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
Known values: `0` stopped, `1`/`3` running, `2`/`10` paused, `4` starting.
Anything else is unknown: the engine keeps treating it as stopped (nothing
starts or ends a session on a guess), but the raw value is kept and the app
shows "Treadmill state unknown (code N)".

## Capturing the standby state
Open question: with its screen off the deck stays connected, but we don't yet
know whether it sends a status byte of its own, keeps sending `0`, or stops
sending frames. To find out:

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

If the screen-off state turns out to have its own byte, add it to
`status_from_byte` in `protocol.rs`.
