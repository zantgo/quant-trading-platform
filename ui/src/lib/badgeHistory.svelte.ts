// badgeHistory — per-layer badge state trails (v11.5).
//
// Every layer badge (L1 Metrics per instance×slot, L2 Alignment … L6
// Recommendation per instance, L7 Overview global) keeps a LITERAL ring of
// its last 5 states — newest first, position 0 = the CURRENT sample. The
// rendering (BadgeTrail.svelte) shows positions 1..4 as progressively
// smaller/fainter "ghost text" after the live badge.
//
// Entries are produced by the SAME builder functions the live badges use,
// so a trail can never disagree with the badge beside it.
//
// Persistence: debounced writes to namespaced localStorage
// (`qtp.badgeHistory.v1`) — history survives F5 and new tabs. Restore
// happens at module init; the next completed candle appends.

export interface BadgeHistoryEntry {
    /** Primary state label (e.g. "STRONG BULL", "LONG", "MODERATE"). */
    label: string;
    /** State hue (the live badge's color). */
    color: string;
    /** UTC timestamp of the sample (epoch ms). */
    ts: number;
}

export const BADGE_HISTORY_CAP = 5;
const STORAGE_KEY = 'qtp.badgeHistory.v1';
const PERSIST_DEBOUNCE_MS = 2000;

type HistoryMap = Map<string, BadgeHistoryEntry[]>;

const history: HistoryMap = new Map();
let persistTimer: ReturnType<typeof setTimeout> | null = null;

function isBrowser(): boolean {
    return typeof window !== 'undefined' && typeof localStorage !== 'undefined';
}

/** Module-init restore. Tolerates corrupted/oversized payloads. */
function restore(): void {
    if (!isBrowser()) return;
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (!raw) return;
        const parsed = JSON.parse(raw) as Record<string, unknown>;
        if (typeof parsed !== 'object' || parsed === null) return;
        for (const [key, value] of Object.entries(parsed)) {
            if (!Array.isArray(value)) continue;
            const entries: BadgeHistoryEntry[] = [];
            for (const e of value) {
                if (
                    e && typeof e === 'object' &&
                    typeof (e as BadgeHistoryEntry).label === 'string' &&
                    typeof (e as BadgeHistoryEntry).color === 'string' &&
                    typeof (e as BadgeHistoryEntry).ts === 'number'
                ) {
                    const { label, color, ts } = e as BadgeHistoryEntry;
                    entries.push({ label, color, ts });
                }
            }
            if (entries.length > 0) {
                history.set(key, entries.slice(0, BADGE_HISTORY_CAP));
            }
        }
    } catch {
        // Corrupted storage — start clean.
    }
}
restore();

function persist(): void {
    if (!isBrowser()) return;
    if (persistTimer !== null) return; // debounced
    persistTimer = setTimeout(() => {
        persistTimer = null;
        try {
            const out: Record<string, BadgeHistoryEntry[]> = {};
            for (const [key, entries] of history) {
                if (entries.length > 0) out[key] = entries;
            }
            localStorage.setItem(STORAGE_KEY, JSON.stringify(out));
        } catch {}
    }, PERSIST_DEBOUNCE_MS) as unknown as ReturnType<typeof setTimeout>;
}

/** Push a sample (completed-candle / overview-poll cadence). Literal
 *  semantics: repeats are kept. Newest lands at index 0; the ring caps
 *  at 5 (oldest dropped rightmost). */
export function pushBadge(key: string, entry: BadgeHistoryEntry): void {
    if (!key || !entry || typeof entry.label !== 'string') return;
    const ring = history.get(key) ?? [];
    ring.unshift({ label: entry.label, color: entry.color, ts: entry.ts });
    if (ring.length > BADGE_HISTORY_CAP) ring.length = BADGE_HISTORY_CAP;
    history.set(key, ring);
    persist();
}

/** The full literal ring (newest first, index 0 = current sample). */
export function getBadgeHistory(key: string): BadgeHistoryEntry[] {
    return history.get(key) ?? [];
}

/** The trail = the PREVIOUS samples (positions 1..4); position 0 is the
 *  live badge and is rendered by the header/table as it is today. */
export function getBadgeTrail(key: string): BadgeHistoryEntry[] {
    return getBadgeHistory(key).slice(1);
}

/** Clear everything (tests + explicit operator reset). Also drops the
 *  persisted copy. */
export function clearBadgeHistory(): void {
    history.clear();
    if (isBrowser()) {
        try { localStorage.removeItem(STORAGE_KEY); } catch {}
    }
    if (persistTimer !== null) {
        clearTimeout(persistTimer);
        persistTimer = null;
    }
}

/** Reactivity version counter — components derive from this + their key
 *  so a push re-renders the trail. Svelte 5 $state in module scope. */
export const badgeHistoryVersion = $state({ v: 0 });

export function notifyBadgeChanged(): void {
    badgeHistoryVersion.v += 1;
}

// ── Key helpers ─────────────────────────────────────────────────────
// L1 Metrics  : `l1:<pairKey>:<slot>`  (per instance × timeframe)
// L2..L6      : `l2:<pairKey>` … `l6:<pairKey>` (per instance)
// L7 Overview : `l7:global`

export function l1Key(pairKey: string, slot: string): string {
    return `l1:${pairKey}:${slot}`;
}

export function layerKey(layer: 'l2' | 'l3' | 'l4' | 'l5' | 'l6', pairKey: string): string {
    return `${layer}:${pairKey}`;
}

export const L7_KEY = 'l7:global';
