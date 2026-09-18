// v11.12: per-tab Welcome-gate acknowledgement. sessionStorage is PER-TAB:
// each browser tab must deliberately (re)connect to a running session, and
// an already-open tab is never interrupted mid-work. Daemon restarts still
// surface the Recover/Discard card on every tab via the status poll.
const SESSION_ACK_KEY = 'qtp.sessionAcknowledged';

export function markSessionAcknowledged(): void {
    try { sessionStorage.setItem(SESSION_ACK_KEY, '1'); } catch { /* private mode */ }
}

export function clearSessionAcknowledged(): void {
    try { sessionStorage.removeItem(SESSION_ACK_KEY); } catch { /* private mode */ }
}

export function isSessionAcknowledged(): boolean {
    try { return sessionStorage.getItem(SESSION_ACK_KEY) === '1'; } catch { return false; }
}

export class SessionStore {
    sessionActive = $state(false);
    sessionCurrency = $state<string>('USDT');
    sessionExchange = $state<string>('Hyperliquid');
    sessionCapital = $state(1000);
    sessionMode = $state<'observe' | 'paper' | 'live'>('observe');
    sessionInstanceCount = $state(0);
    /** v10: persisted session number (monotonic). */
    sessionId = $state<number | null>(null);
    /// v11.6 crash recovery: the previous session was not finalized.
    sessionInterrupted = $state(false);
    interruptedSession = $state<{
        id: number;
        mode: string | null;
        exchange: string | null;
        currency: string | null;
        started_at_ms: number;
        instance_count: number;
    } | null>(null);
    sessionLoading = $state(false);
    sessionChecked = $state(false);
    sessionError = $state<string | null>(null);

    onSessionActivated: (() => void) | null = null;

    async fetchSessionStatus() {
        try {
            const res = await fetch('/api/session/status');
            if (res.ok) {
                const data = await res.json();
                const wasActive = this.sessionActive;
                this.sessionActive = data.active;
                if (this.sessionActive && !wasActive && this.onSessionActivated) this.onSessionActivated();
                this.sessionCurrency = data.currency || 'USDT';
                this.sessionExchange = data.exchange || 'Hyperliquid';
                if (data.capital) this.sessionCapital = data.capital;
                if (data.mode === 'observe' || data.mode === 'paper' || data.mode === 'live') {
                    this.sessionMode = data.mode;
                }
                this.sessionInstanceCount = data.instance_count || 0;
                if (data.session_id != null) this.sessionId = data.session_id;
                this.sessionInterrupted = data.interrupted === true;
                this.interruptedSession = data.interrupted_session ?? null;
            }
        } catch (_) { /* backend may not be ready yet */ } finally { this.sessionChecked = true; }
    }

    /// v11.6 crash recovery — RECOVER the interrupted session.
    async recoverInterrupted(): Promise<void> {
        const res = await fetch('/api/session/recover', { method: 'POST' });
        if (!res.ok) {
            const txt = await res.text().catch(() => '');
            throw new Error(txt || `recover failed (${res.status})`);
        }
        markSessionAcknowledged();
        await this.fetchSessionStatus();
        if (this.onSessionActivated) this.onSessionActivated();
    }

    /// v11.6 crash recovery — DISCARD the interrupted session.
    async discardInterrupted(): Promise<void> {
        const res = await fetch('/api/session/discard', { method: 'POST' });
        if (!res.ok) {
            const txt = await res.text().catch(() => '');
            throw new Error(txt || `discard failed (${res.status})`);
        }
        this.sessionInterrupted = false;
        this.interruptedSession = null;
    }

    async initSession(
        currency: string,
        exchange: string,
        mode: 'observe' | 'paper' | 'live' = 'observe',
        capital?: number,
    ): Promise<{ success: boolean; error?: string }> {
        this.sessionLoading = true; this.sessionError = null;
        try {
            const body: Record<string, unknown> = { currency, exchange, mode };
            if (capital != null && capital > 0) body.portfolio_capital_usd = capital;
            const res = await fetch('/api/session/init', {
                method: 'POST', headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body),
            });
            const data = await res.json();
            if (res.ok && data.success) {
                const wasActive = this.sessionActive;
                this.sessionActive = true; this.sessionCurrency = currency;
                this.sessionExchange = exchange; this.sessionMode = mode;
                if (capital != null && capital > 0) this.sessionCapital = capital;
                markSessionAcknowledged();
                if (!wasActive && this.onSessionActivated) this.onSessionActivated();
                this.sessionLoading = false; return { success: true };
            }
            this.sessionError = data.error || 'Session initialization failed';
        } catch (e: any) { this.sessionError = e.message || 'Network error'; }
        this.sessionLoading = false;
        return { success: false, error: this.sessionError || undefined };
    }

    async quitSession(): Promise<boolean> {
        this.sessionLoading = true;
        try {
            const res = await fetch('/api/session/quit', { method: 'POST' });
            const data = await res.json();
            if (res.ok && data.success) {
                this.sessionActive = false; this.sessionCurrency = 'USDT';
                this.sessionExchange = 'Hyperliquid';
                this.sessionMode = 'observe'; this.sessionInstanceCount = 0;
                clearSessionAcknowledged();
                this.sessionLoading = false; return true;
            }
        } catch (_) {}
        this.sessionLoading = false; return false;
    }
}
