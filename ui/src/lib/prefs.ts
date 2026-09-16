// prefs — tiny localStorage-backed preference layer (Phase 3).
//
// UI-only conveniences (chart overlay toggles, console open/tab,
// sidebar / workspace-panel open flags) deliberately stay OUT of the
// URL: they are per-operator chrome, not shareable navigation state.
// Everything is namespaced under the `qtp.` prefix, JSON-serialized,
// and every access is try/catch-guarded so a corrupted value, a
// privacy-mode browser, or a non-DOM environment (SSR / tests) simply
// falls back to the caller's default.

const PREFIX = 'qtp.';

export function loadPref<T>(key: string, fallback: T): T {
    try {
        if (typeof localStorage === 'undefined') return fallback;
        const raw = localStorage.getItem(PREFIX + key);
        if (raw === null || raw === undefined) return fallback;
        return JSON.parse(raw) as T;
    } catch (_e) {
        // Corrupted JSON or storage access denied — fall back.
        return fallback;
    }
}

export function savePref(key: string, value: unknown): void {
    try {
        if (typeof localStorage === 'undefined') return;
        localStorage.setItem(PREFIX + key, JSON.stringify(value));
    } catch (_e) {
        // Quota exceeded / storage unavailable — preferences are
        // best-effort; never let a save break the UI.
    }
}
