// v11.12: the Welcome-gate acknowledgement is IN-MEMORY ONLY
// (`AppStore.sessionAcknowledged`). sessionStorage was tried first but it
// SURVIVES page reloads — a wizard-launched session would have silently
// resumed on reload instead of showing the mandatory Resume/Quit card.
// The in-memory flag gives exactly the required semantics: alive while the
// tab lives (multi-tab preserved), reset by any reload (mandatory card).

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
                this.sessionLoading = false; return true;
            }
        } catch (_) {}
        this.sessionLoading = false; return false;
    }
}
