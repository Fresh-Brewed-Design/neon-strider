import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ActionStatus } from "../../components/ActionStatus";
import { errorText, useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { Section, Toggle } from "./fields";

/** "Start with Windows" (on macOS a login item): a per-user login entry, off until the user turns it on. */
export function StartupSection() {
  const mac = useAppSnapshot()?.platform.os === "macos";
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const change = useAction();

  useEffect(() => {
    let live = true;
    invoke<boolean>("get_launch_at_login")
      .then((v) => live && setEnabled(Boolean(v)))
      .catch((e) => live && setLoadError(errorText(e)));
    return () => {
      live = false;
    };
  }, []);

  const toggle = (next: boolean) => {
    setLoadError(null);
    void change.run(async () => setEnabled(Boolean(await invoke<boolean>("set_launch_at_login", { enabled: next }))));
  };

  const error = change.error ?? loadError;

  return (
    <Section
      id="sys-startup"
      title="Startup"
      description={
        mac
          ? "Neon Strider starts in the menu bar, with no window, when you log in."
          : "Neon Strider starts in the tray, with no window, when you sign in to Windows."
      }
    >
      <div>
        <Toggle
          label="Launch at login"
          description={
            mac
              ? "Adds a login item for your account. No administrator password needed."
              : "Adds a per-user startup entry. No administrator rights needed."
          }
          checked={enabled === true}
          onChange={toggle}
        />
        <ActionStatus error={error && `Could not change the startup setting: ${error}`} className="mt-1" />
      </div>
    </Section>
  );
}
