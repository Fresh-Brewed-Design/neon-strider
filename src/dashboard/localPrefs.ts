// Small per-machine UI preferences (the open page, developer tools). Storage
// can be unavailable; then the defaults apply and nothing is remembered.

export function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writePref(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembering a preference is harmless.
  }
}
