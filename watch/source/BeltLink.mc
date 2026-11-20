import Toybox.BluetoothLowEnergy;
import Toybox.Lang;
import Toybox.System;

// The WRIST_LINK service and its State characteristic (docs/wrist-link.md).
// Keep these in step with src-tauri/src/wrist_link.rs.
const SERVICE_UUID = BluetoothLowEnergy.stringToUuid("00b165d6-40c2-4f94-8df9-0f69431f21af");
const STATE_UUID = BluetoothLowEnergy.stringToUuid("00b165d6-40c2-4f94-8df9-0f69431f21b0");
// The standard Running Speed and Cadence service GHOST_POD always advertises.
const RSC_UUID = BluetoothLowEnergy.stringToUuid("00001814-0000-1000-8000-00805f9b34fb");

// How long LINK may wait for the connection before the field gives up and
// scans again. pairDevice has no timeout of its own: Garmin's docs only say
// the BLE subsystem "will begin to search for the device". The PC advertises
// several times a second, and connecting plus finding three small services
// takes a few seconds, so 20 s is several times what a working link needs.
// It is also what one Connect IQ forum report waited before calling a
// pairDevice that never answered stuck.
const LINK_TIMEOUT_MS = 20000;

// The link to the PC. The pairing does not survive the field restarting, so
// it scans again each time. Nothing here writes to the PC except the
// notification switch on State.
class BeltLink extends BluetoothLowEnergy.BleDelegate {
    // What the field shows: the last State read, and why there is none.
    var speedKmh as Float or Null = null;
    var belt as Number = 0;
    var status as String = "SCAN";

    private var _device as BluetoothLowEnergy.Device or Null = null;
    // When pairDevice was called (System.getTimer), or null when not waiting.
    private var _linkSince as Number or Null = null;
    // The last LINK gave up: show NOCONN while scanning again.
    private var _gaveUp as Boolean = false;

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
        self.status = _gaveUp ? "NOCONN" : "SCAN";
        BluetoothLowEnergy.setScanState(BluetoothLowEnergy.SCAN_STATE_SCANNING);
    }

    // Called once a second from the field's compute(): gives up on a LINK
    // that has waited LINK_TIMEOUT_MS.
    function check() as Void {
        var since = _linkSince;
        if (since == null || System.getTimer() - since < LINK_TIMEOUT_MS) {
            return;
        }
        _gaveUp = true;
        forget();
        scan();
    }

    // Drop the pairing, so the BLE subsystem stops searching for the device.
    private function forget() as Void {
        var device = _device;
        _device = null;
        if (device != null) {
            BluetoothLowEnergy.unpairDevice(device);
        }
    }

    function onScanResults(scanResults as BluetoothLowEnergy.Iterator) as Void {
        // A WRIST_LINK advertisement wins over a foot pod one in the same batch.
        var rsc = null;
        for (var result = scanResults.next(); result != null; result = scanResults.next()) {
            if (!(result instanceof BluetoothLowEnergy.ScanResult)) {
                continue;
            }
            var match = matchOf(result);
            if (match == 'W') {
                link(result, "LINK W");
                return;
            }
            if (match == 'R' && rsc == null) {
                rsc = result;
            }
        }
        if (rsc != null) {
            link(rsc as BluetoothLowEnergy.ScanResult, "LINK R");
        }
    }

    // LINK W: the advertisement carried the WRIST_LINK UUID. LINK R: it
    // carried the foot pod's RSC UUID only (the PC leaves WRIST_LINK out of
    // its advertisement, so R is the usual one).
    private function link(result as BluetoothLowEnergy.ScanResult, label as String) as Void {
        BluetoothLowEnergy.setScanState(BluetoothLowEnergy.SCAN_STATE_OFF);
        self.status = label;
        _linkSince = System.getTimer();
        try {
            _device = BluetoothLowEnergy.pairDevice(result);
        } catch (e) {
            // No free connection, or the pairing failed outright.
            _linkSince = null;
            self.status = "PAIRERR";
        }
    }

    // 'W' for the WRIST_LINK service, 'R' for the foot pod, null for neither.
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
        return found;
    }

    function onConnectedStateChanged(device as BluetoothLowEnergy.Device, state as BluetoothLowEnergy.ConnectionState) as Void {
        _linkSince = null;
        if (state != BluetoothLowEnergy.CONNECTION_STATE_CONNECTED) {
            self.speedKmh = null;
            forget();
            scan();
            return;
        }
        _gaveUp = false;
        _device = device;
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
