import { dispatch, SETTING_LIMITS } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { systemLabel } from "../../names";
import { useAppSnapshot } from "../../state/useAppState";
import { NumberField, Toggle } from "./fields";

const L = SETTING_LIMITS;

/** DAILY_QUOTA: an optional daily km target, drawn as a thin bar under TODAY. */
export function DailyQuotaSetting() {
  const s = useAppSnapshot();
  const action = useAction();
  if (!s) return null;
  const target = s.daily_quota.target_km;
  const save = (km: number | null) => dispatch({ type: "daily_quota_set", km });
  return (
    <>
      <div>
        <Toggle
          label={systemLabel("DAILY_QUOTA")}
          checked={target !== null}
          onChange={(on) => void action.run(() => save(on ? L.default_daily_quota_km : null))}
          description="TODAY shows the day's km against a daily target, with a thin bar."
          hint={["The bar fills as you walk and stays full past the target.", "Nothing pops up when the target is walked."]}
        />
        <ActionStatus pending={action.pending} error={action.error} className="mt-1" />
      </div>
      {target !== null && (
        <NumberField
          label="Daily target"
          unit="km"
          min={L.daily_quota_km.min}
          max={L.daily_quota_km.max}
          step={0.5}
          value={target}
          hint={`${L.daily_quota_km.min} to ${L.daily_quota_km.max} km. A new day starts the count at midnight.`}
          onCommit={(km) => save(Math.round(km * 10) / 10)}
        />
      )}
    </>
  );
}
