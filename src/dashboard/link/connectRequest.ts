// The HUD's "pick a treadmill" request (`connection.connect_dialog_at_ms`):
// the dashboard opens its connect dialog once per new request. Tested in
// connectRequest.test.ts.

/** Where the dashboard remembers the last request it followed. */
export const CONNECT_SHOWN_KEY = "dashboard.connectDialogShownAt";

/** `at` is a request this dashboard hasn't followed yet (`seen` is the last one it did). */
export function isNewConnectRequest(at: number | null, seen: string | null): at is number {
  return at !== null && String(at) !== seen;
}
