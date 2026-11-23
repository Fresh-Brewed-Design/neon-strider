import Toybox.BluetoothLowEnergy;
import Toybox.Lang;
import Toybox.System;

// The WRIST_LINK service and its State characteristic (docs/wrist-link.md).
// Keep these in step with src-tauri/src/wrist_link.rs.
const SERVICE_UUID = BluetoothLowEnergy.stringToUuid("00b165d6-40c2-4f94-8df9-0f69431f21af");
const STATE_UUID = BluetoothLowEnergy.stringToUuid("00b165d6-40c2-4f94-8df9-0f69431f21b0");
// The standard Running Speed and Cadence service. The PC served it as GHOST_POD
// until #297; the match is kept for app versions from before.
const RSC_UUID = BluetoothLowEnergy.stringToUuid("00001814-0000-1000-8000-00805f9b34fb");
// The PC's WRIST_LINK beacon: manufacturer data for company 0xFFFF (the
// Bluetooth SIG's "no company" ID) starting "NSTR". Keep in step with
// BEACON_COMPANY and BEACON_MARKER in src-tauri/src/wrist_link.rs. The version
// byte after the letters is not checked, so a later version still matches.
const BEACON_PREFIX = [0xff, 0xff, 0x4e, 0x53, 0x54, 0x52]b;

// Scan results at least this strong get their raw bytes in the log too. The
// PC sat next to the watch at -46 to -48 dBm in the first scan log, and the
// room's other devices were -64 dBm and weaker, so -60 keeps the log to the
// devices near the treadmill.
const LOG_RAW_RSSI = -60;

// How long LINK may wait for the connection before the field gives up and
// scans again. pairDevice has no timeout of its own: Garmin's docs only say
// the BLE subsystem "will begin to search for the device". The PC advertises
// several times a second, and connecting plus finding three small services
// takes a few seconds, so 20 s is several times what a working link needs.
// It is also what one Connect IQ forum report waited before calling a
// pairDevice that never answered stuck.
const LINK_TIMEOUT_MS = 20000;

// Scan results written to the watch's log: enough to cover a room's devices
// a few times over, small enough that the log file stays short.
const LOG_LIMIT = 150;

// The link to the PC. The pairing does not survive the field restarting, so
// it scans again each time. Nothing here writes to the PC except the
// notification switch on State.
class BeltLink extends BluetoothLowEnergy.BleDelegate {
    // What the field shows: the last State read, and why there is none.
    var speedKmh as Float or Null = null;
    var belt as Number = 0;
    var status as String = "SCAN";

    // When pairDevice was called (System.getTimer), or null when not waiting.
    private var _linkSince as Number or Null = null;
    // Scan results seen since the field started, shown as SCAN n so a test
    // tells "the watch hears nothing" apart from "it hears devices, not the PC".
    private var _seen as Number = 0;

    function initialize() {
        BleDelegate.initialize();
        BluetoothLowEnergy.setDelegate(self);
        var profile = {
            :uuid => SERVICE_UUID,
            :characteristics => [{
                :uuid => STATE_UUID,
                :descriptors => [BluetoothLowEnergy.cccdUuid()]
            }]
        };
        BluetoothLowEnergy.registerProfile(profile);
    }

    function onProfileRegister(uuid as BluetoothLowEnergy.Uuid, status as BluetoothLowEnergy.Status) as Void {
        if (status == BluetoothLowEnergy.STATUS_SUCCESS) {
            scan();
        } else {
            self.status = "ERR";
        }
    }

    function scan() as Void {
        _linkSince = null;
        self.status = "SCAN " + _seen;
        BluetoothLowEnergy.setScanState(BluetoothLowEnergy.SCAN_STATE_SCANNING);
    }

    // Called once a second from the field's compute(): a LINK that has
    // waited LINK_TIMEOUT_MS stops as BLOCKED. It does not unpair and retry:
    // on a watch that also has the PC as its foot pod, unpairing dropped the
    // shared link and knocked the foot pod off every 20 s (hardware test,
    // 2026-11-20).
    function check() as Void {
        var since = _linkSince;
        if (since == null || System.getTimer() - since < LINK_TIMEOUT_MS) {
            return;
        }
        _linkSince = null;
        self.status = "BLOCKED";
    }

    function onScanResults(scanResults as BluetoothLowEnergy.Iterator) as Void {
        // In one batch WRIST_LINK wins over the foot pod, and the foot pod
        // over the beacon: the foot pod's advertisement is the GATT server's
        // own, so it is the surest way to connect.
        var rsc = null;
        var beacon = null;
        for (var result = scanResults.next(); result != null; result = scanResults.next()) {
            if (!(result instanceof BluetoothLowEnergy.ScanResult)) {
                continue;
            }
            _seen += 1;
            if (_linkSince == null && self.status.find("SCAN") == 0) {
                self.status = "SCAN " + _seen;
            }
            var match = matchOf(result);
            logResult(result, match);
            if (match == 'W') {
                link(result, "LINK W");
                return;
            }
            if (match == 'R' && rsc == null) {
                rsc = result;
            }
            if (match == 'M' && beacon == null) {
                beacon = result;
            }
        }
        if (rsc != null) {
            link(rsc as BluetoothLowEnergy.ScanResult, "LINK R");
        } else if (beacon != null) {
            link(beacon as BluetoothLowEnergy.ScanResult, "LINK M");
        }
    }

    // LINK W: the advertisement carried the WRIST_LINK UUID. LINK R: it
    // carried the foot pod's RSC UUID. LINK M: it carried the PC's beacon
    // marker (a legacy advertisement the PC sends while WRIST_LINK is on).
    private function link(result as BluetoothLowEnergy.ScanResult, label as String) as Void {
        BluetoothLowEnergy.setScanState(BluetoothLowEnergy.SCAN_STATE_OFF);
        self.status = label;
        _linkSince = System.getTimer();
        try {
            BluetoothLowEnergy.pairDevice(result);
        } catch (e) {
            // No free connection, or the pairing failed outright.
            _linkSince = null;
            self.status = "PAIRERR";
        }
    }

    // Diagnostics: one line per scan result, up to LOG_LIMIT, written to
    // GARMIN\Apps\LOGS\NeonStriderBelt.TXT on the watch when that file exists.
    // It shows what the watch hears: each device's name, signal and advertised
    // service UUIDs, so a test can tell why the PC isn't matched.
    // A strong result also logs its raw bytes in hex, so the log shows what
    // the PC really sends.
    private var _logged as Number = 0;
    private function logResult(result as BluetoothLowEnergy.ScanResult, match as Char or Null) as Void {
        if (_logged >= LOG_LIMIT) {
            return;
        }
        _logged += 1;
        var rssi = result.getRssi();
        var line = "scan " + _logged + " name=" + result.getDeviceName() + " rssi=" + rssi + " uuids=";
        for (var uuids = result.getServiceUuids(), u = uuids.next(); u != null; u = uuids.next()) {
            line += u.toString() + " ";
        }
        var raw = result.getRawData();
        line += "raw=" + (raw != null ? raw.size() : 0) + "B";
        if (raw != null && rssi >= LOG_RAW_RSSI) {
            line += " hex=";
            for (var i = 0; i < raw.size(); i++) {
                line += raw[i].format("%02x");
            }
        }
        line += " match=" + (match != null ? match.toString() : "-");
        System.println(line);
    }

    // 'W' for the WRIST_LINK service, 'R' for the foot pod, 'M' for the
    // PC's beacon, null for none. The raw bytes are read too: Connect IQ's
    // own UUID list has been reported to come back empty for some
    // advertisements (Garmin forum bug report WERETECH-9123).
    private function matchOf(result as BluetoothLowEnergy.ScanResult) as Char or Null {
        var found = null;
        for (var uuids = result.getServiceUuids(), u = uuids.next(); u != null; u = uuids.next()) {
            if (u.equals(SERVICE_UUID)) {
                return 'W';
            }
            if (u.equals(RSC_UUID)) {
                found = 'R';
            }
        }
        if (found != null) {
            return found;
        }
        var raw = result.getRawData();
        return raw != null ? matchRaw(raw) : null;
    }

    // Walk the advertisement's sections: [length][type][data...]. Type 0x02
    // or 0x03 lists 16-bit service UUIDs, little endian (0x1814 is 14 18);
    // type 0xFF is manufacturer data, which carries the beacon.
    private function matchRaw(raw as ByteArray) as Char or Null {
        var found = null;
        var i = 0;
        while (i + 1 < raw.size()) {
            var len = raw[i];
            if (len == 0 || i + 1 + len > raw.size()) {
                break;
            }
            // The section is raw[i + 1] (type) to raw[end - 1]; its data
            // starts after the type.
            var type = raw[i + 1];
            var start = i + 2;
            var end = i + 1 + len;
            if (type == 0x02 || type == 0x03) {
                for (var j = start; j + 1 < end; j += 2) {
                    if (raw[j] == 0x14 && raw[j + 1] == 0x18) {
                        return 'R';
                    }
                }
            } else if (type == 0xff && end - start >= BEACON_PREFIX.size()) {
                var same = true;
                for (var k = 0; k < BEACON_PREFIX.size(); k++) {
                    if (raw[start + k] != BEACON_PREFIX[k]) {
                        same = false;
                        break;
                    }
                }
                if (same) {
                    found = 'M';
                }
            }
            i = end;
        }
        return found;
    }

    // A dropped connection keeps the pairing: the watch reconnects a paired
    // device by itself, and unpairing a device the watch also has as its foot
    // pod knocked the foot pod off (hardware test, 2026-11-20).
    function onConnectedStateChanged(device as BluetoothLowEnergy.Device, state as BluetoothLowEnergy.ConnectionState) as Void {
        _linkSince = null;
        if (state != BluetoothLowEnergy.CONNECTION_STATE_CONNECTED) {
            self.speedKmh = null;
            self.status = "LOST";
            return;
        }
        var service = device.getService(SERVICE_UUID);
        var state_char = service != null ? service.getCharacteristic(STATE_UUID) : null;
        if (state_char == null) {
            self.status = "NOSVC";
            return;
        }
        // Notifications, then one read for the value it holds now.
        var cccd = state_char.getDescriptor(BluetoothLowEnergy.cccdUuid());
        if (cccd != null) {
            cccd.requestWrite([0x01, 0x00]b);
        }
        state_char.requestRead();
        self.status = "OK";
    }

    function onCharacteristicRead(c as BluetoothLowEnergy.Characteristic, status as BluetoothLowEnergy.Status, value as ByteArray) as Void {
        if (status == BluetoothLowEnergy.STATUS_SUCCESS) {
            decode(value);
        }
    }

    function onCharacteristicChanged(c as BluetoothLowEnergy.Characteristic, value as ByteArray) as Void {
        decode(value);
    }

    // State: [belt u8][speed_kmh_x100 u16][incline u8][flags u8][last_seq u8][result u8].
    private function decode(value as ByteArray) as Void {
        if (value.size() < 7) {
            return;
        }
        self.belt = value[0];
        self.speedKmh = (value[1] | (value[2] << 8)) / 100.0;
        self.status = "OK";
    }
}
