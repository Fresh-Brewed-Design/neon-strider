import Toybox.Application;
import Toybox.Lang;
import Toybox.WatchUi;

// The Neon Strider belt data field (WRIST_LINK phase 0, roadmap #82): it finds
// the PC's WRIST_LINK service, connects while the foot pod is paired, and shows
// the belt speed it reads. It sends nothing to the PC.
class NeonStriderApp extends Application.AppBase {
    function initialize() {
        AppBase.initialize();
    }

    function getInitialView() as [Views] or [Views, InputDelegates] {
        return [new BeltField()];
    }
}
