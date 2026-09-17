import type { DecisionProfile, DecisionScore, RiskProfile, RiskCalculation, FeeTableRow, CommissionProjection } from '../types';

export class ProfileStore {
    activeDecisionProfileId = $state(1);
    decisionProfiles = $state<DecisionProfile[]>([]);
    calculatedDecisionScore = $state<DecisionScore | null>(null);
    decisionLoading = $state(false);

    activeRiskProfileId = $state(1);
    riskProfiles = $state<RiskProfile[]>([]);
    riskDirection = $state<'LONG' | 'SHORT'>('LONG');
    riskEntryPrice = $state('0'); riskStopLoss = $state('0'); riskTakeProfit = $state('0');
    riskCalculation = $state<RiskCalculation | null>(null);
    riskCalculating = $state(false);
    useDynamicAtr = $state(false); atrValue = $state(0);

    commissionDirection = $state<'LONG' | 'SHORT'>('LONG');
    commissionEntry1 = $state(''); commissionEntry2 = $state('');
    commissionSL1 = $state(''); commissionSL2 = $state('');
    commissionTP1 = $state(''); commissionTP2 = $state('');
    commissionCapitalSplit = $state(50);
    commissionOrderType = $state<'maker' | 'taker'>('taker');
    commissionProjection = $state<CommissionProjection | null>(null);
    commissionLoading = $state(false);
    feeTable = $state<FeeTableRow[]>([]); feeTableLoading = $state(false);

    async fetchDecisionProfiles() {
        try { const res = await fetch('/api/decision-profiles'); if (res.ok) { this.decisionProfiles = await res.json(); } } catch (_) {}
    }

    async createDecisionProfile(name: string, longT: number, shortT: number) {
        try { await fetch('/api/decision-profiles', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ profile_name: name, long_threshold: longT, short_threshold: shortT }) }); await this.fetchDecisionProfiles(); } catch (_) {}
    }

    async deleteDecisionProfile(id: number) {
        try { await fetch(`/api/decision-profiles/${id}`, { method: 'DELETE' }); await this.fetchDecisionProfiles(); } catch (_) {}
    }

    async updateDecisionProfileThresholds(id: number, longT: number, shortT: number) {
        try { await fetch(`/api/decision-profiles/${id}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ long_threshold: longT, short_threshold: shortT }) }); await this.fetchDecisionProfiles(); } catch (_) {}
    }

    async addProfileIndicator(profileId: number, name: string, weight: number, overrideStatus: string) {
        try { await fetch(`/api/decision-profiles/${profileId}/indicators`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ indicator_name: name, weight, override_status: overrideStatus }) }); await this.fetchDecisionProfiles(); } catch (_) {}
    }

    async updateProfileIndicator(profileId: number, indicatorId: number, weight: number, overrideStatus: string) {
        try { await fetch(`/api/decision-profiles/${profileId}/indicators/${indicatorId}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ weight, override_status: overrideStatus }) }); await this.fetchDecisionProfiles(); } catch (_) {}
    }

    async deleteProfileIndicator(profileId: number, indicatorId: number) {
        try { await fetch(`/api/decision-profiles/${profileId}/indicators/${indicatorId}`, { method: 'DELETE' }); await this.fetchDecisionProfiles(); } catch (_) {}
    }

    async evaluateDecision(profileId: number, symbol: string, latestSnapshot: Record<string, unknown> | null) {
        this.decisionLoading = true;
        try {
            const res = await fetch(`/api/decision-profiles/${profileId}/evaluate`, {
                method: 'POST', headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ symbol, latest_snapshot: latestSnapshot }),
            });
            if (res.ok) { this.calculatedDecisionScore = await res.json(); }
        } catch (_) {} finally { this.decisionLoading = false; }
    }

    async fetchRiskProfiles() {
        try { const res = await fetch('/api/risk-profiles'); if (res.ok) { this.riskProfiles = await res.json(); } } catch (_) {}
    }

    async createRiskProfile(name: string, capital: number, riskPct: number, leverage: number) {
        try { await fetch('/api/risk-profiles', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ profile_name: name, capital, max_risk_pct: riskPct, leverage }) }); await this.fetchRiskProfiles(); } catch (_) {}
    }

    async deleteRiskProfile(id: number) {
        try { await fetch(`/api/risk-profiles/${id}`, { method: 'DELETE' }); await this.fetchRiskProfiles(); } catch (_) {}
    }

    async calculateRisk(overrides?: { capital?: number; leverage?: number; commissionPct?: number }) {
        this.riskCalculating = true;
        try {
            const body: Record<string, unknown> = {
                profile_id: this.activeRiskProfileId, direction: this.riskDirection,
                entry_price: parseFloat(this.riskEntryPrice) || 0,
                stop_loss: parseFloat(this.riskStopLoss) || 0,
                take_profit: parseFloat(this.riskTakeProfit) || 0,
            };
            // v7.0: the Project Risk and Return drawer passes stateless
            // what-if overrides; the backend prefers them over the saved
            // profile without mutating the stored configuration.
            if (overrides?.capital != null && isFinite(overrides.capital)) body.capital = overrides.capital;
            if (overrides?.leverage != null && isFinite(overrides.leverage)) body.leverage = overrides.leverage;
            if (overrides?.commissionPct != null && isFinite(overrides.commissionPct)) body.commission_pct = overrides.commissionPct;
            const res = await fetch('/api/risk/calculate', {
                method: 'POST', headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body),
            });
            if (res.ok) { this.riskCalculation = await res.json(); }
        } catch (_) {} finally { this.riskCalculating = false; }
    }

    async fetchFeeTable() {
        this.feeTableLoading = true;
        try { const res = await fetch(`/api/risk/fee-table?order_type=${this.commissionOrderType}`); if (res.ok) { this.feeTable = await res.json(); } } catch (_) {} finally { this.feeTableLoading = false; }
    }

    async calculateCommissionProjection() {
        this.commissionLoading = true;
        try {
            const res = await fetch('/api/risk/commission-projection', {
                method: 'POST', headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    profile_id: this.activeRiskProfileId, direction: this.commissionDirection,
                    entry_1: parseFloat(this.commissionEntry1) || 0, entry_2: parseFloat(this.commissionEntry2) || 0,
                    stop_loss_1: parseFloat(this.commissionSL1) || 0, stop_loss_2: parseFloat(this.commissionSL2) || 0,
                    take_profit_1: parseFloat(this.commissionTP1) || 0, take_profit_2: parseFloat(this.commissionTP2) || 0,
                    capital_entry_1_pct: this.commissionCapitalSplit, order_type: this.commissionOrderType,
                }),
            });
            if (res.ok) { this.commissionProjection = await res.json(); }
        } catch (_) {} finally { this.commissionLoading = false; }
    }
}
