import Toybox.Activity;
import Toybox.Lang;
import Toybox.WatchUi;

// Shows the belt speed the PC reports over WRIST_LINK, or why there is none.
class BeltField extends WatchUi.SimpleDataField {
    private var _link as BeltLink;

    function initialize() {
        SimpleDataField.initialize();
        label = "BELT";
        _link = new BeltLink();
    }

    function compute(info as Activity.Info) as Numeric or Duration or String or Null {
        if (_link.speedKmh == null) {
            return _link.status;
        }
        var kmh = _link.speedKmh as Float;
        var tag = _link.belt == 2 ? " PAUSED" : "";
        return kmh.format("%.1f") + tag;
    }
}
