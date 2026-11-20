import Toybox.BluetoothLowEnergy;
import Toybox.Lang;

// The WRIST_LINK service and its State characteristic (docs/wrist-link.md).
// Keep these in step with src-tauri/src/wrist_link.rs.
const SERVICE_UUID = BluetoothLowEnergy.stringToUuid("00b165d6-40c2-4f94-8df9-0f69431f21af");
const STATE_UUID = BluetoothLowEnergy.stringToUuid("00b165d6-40c2-4f94-8df9-0f69431f21b0");

// The link to the PC. The pairing does not survive the field restarting, so
// it scans again each time. Nothing here writes to the PC.
class BeltLink extends BluetoothLowEnergy.BleDelegate {
    // What the field shows: the last State read, and why there is none.
    var speedKmh as Float or Null = null;
    var belt as Number = 0;
    var status as String = "SCAN";

    private var _device as BluetoothLowEnergy.Device or Null = null;

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
        self.status = "SCAN";
        BluetoothLowEnergy.setScanState(BluetoothLowEnergy.SCAN_STATE_SCANNING);
    }

    function onScanResults(scanResults as BluetoothLowEnergy.Iterator) as Void {
        for (var result = scanResults.next(); result != null; result = scanResults.next()) {
            if (result instanceof BluetoothLowEnergy.ScanResult && advertisesService(result)) {
                BluetoothLowEnergy.setScanState(BluetoothLowEnergy.SCAN_STATE_OFF);
                self.status = "LINK";
                BluetoothLowEnergy.pairDevice(result);
                return;
            }
        }
    }

    // The PC may not fit the 128-bit UUID in its advertisement next to the foot
    // pod (an open question in #82); the log of a test shows if it is missing.
    private function advertisesService(result as BluetoothLowEnergy.ScanResult) as Boolean {
        for (var uuids = result.getServiceUuids(), u = uuids.next(); u != null; u = uuids.next()) {
            if (u.equals(SERVICE_UUID)) {
                return true;
            }
        }
        return false;
    }

    function onConnectedStateChanged(device as BluetoothLowEnergy.Device, state as BluetoothLowEnergy.ConnectionState) as Void {
        if (state != BluetoothLowEnergy.CONNECTION_STATE_CONNECTED) {
            _device = null;
            self.speedKmh = null;
            scan();
            return;
        }
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
