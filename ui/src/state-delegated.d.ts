import type { DecisionProfile, DecisionScore, RiskProfile, RiskCalculation, FeeTableRow, CommissionProjection, ExchangeAccount, DashboardStats, TradeLedgerRecord, TradeJournalRecord, OpenOrder, SlotState, OverviewMatrix } from './types';

declare module './state.svelte' {
    interface AppStore {
        sessionActive: boolean; sessionMode: string; sessionCurrency: string;
        sessionExchange: string; sessionCapital: number; sessionInstanceCount: number;
        sessionId: number | null;
        sessionLoading: boolean; sessionChecked: boolean;
        sessionError: string | null;

        currentPosition: string; entryPriceVal: string;
        analysisPhase: string;
        currentLevel2Mode: string;
        switchMode(mode: string): void;

        activeDecisionProfileId: number; decisionProfiles: DecisionProfile[];
        calculatedDecisionScore: DecisionScore | null; decisionLoading: boolean;
        activeRiskProfileId: number; riskProfiles: RiskProfile[];
        riskDirection: 'LONG' | 'SHORT'; riskEntryPrice: string; riskStopLoss: string;
        riskTakeProfit: string; riskCalculation: RiskCalculation | null;
        riskCalculating: boolean; useDynamicAtr: boolean; atrValue: number;

        commissionDirection: 'LONG' | 'SHORT'; commissionEntry1: string;
        commissionEntry2: string; commissionSL1: string; commissionSL2: string;
        commissionTP1: string; commissionTP2: string; commissionCapitalSplit: number;
        commissionOrderType: 'maker' | 'taker'; commissionProjection: CommissionProjection | null;
        commissionLoading: boolean; feeTable: FeeTableRow[]; feeTableLoading: boolean;

        exchangeAccounts: ExchangeAccount[]; exchangeActiveCount: number;
        exchangeMaxAccounts: number;
        exchangeFormDraft: { exchange: string; account_name: string; api_key: string; api_secret: string; passphrase: string; referred_uid: string; is_active: boolean };

        openOrders: OpenOrder[];
        activePlan: Record<string, unknown> | null;
        activeConsoleOpen: boolean;
        activeConsoleTab: 'positions' | 'orders' | 'history' | 'plan';
        activeDurations: SlotState[];
        paperBreakEvenTrailEnabled: boolean;

        apiKeyConfigured: boolean; rulesContent: string;
        globalCandlesConfig: { duration_seconds: number };
        globalIndicatorsConfig: Record<string, number>;
        indicatorRegistry: import('./types').IndicatorMeta[];
        emaFastLabel: string; emaMediumLabel: string; emaSlowLabel: string; emaLongLabel: string;
        rsiLabel: string; adxLabel: string; atrLabel: string; macdLabel: string;

        dashboardStats: DashboardStats | null;
        dashboardPeriod: string; dashboardOrigin: string;
        tradeLedgerRecords: TradeLedgerRecord[]; tradeJournalRecords: TradeJournalRecord[];

        overviewMatrix: OverviewMatrix | null;
        fetchOverview(): Promise<void>;
        startOverviewPolling(intervalMs?: number): void;
        stopOverviewPolling(): void;

        fetchSessionStatus(): Promise<void>;
        initSession(
            currency: string,
            exchange: string,
            mode?: 'observe' | 'paper' | 'live',
            capital?: number,
        ): Promise<{ success: boolean; error?: string }>;
        quitSession(): Promise<any>;
        fetchDecisionProfiles(): Promise<void>;
        createDecisionProfile(name: string, longT: number, shortT: number): Promise<void>;
        deleteDecisionProfile(id: number): Promise<void>;
        updateDecisionProfileThresholds(id: number, longT: number, shortT: number): Promise<void>;
        addProfileIndicator(profileId: number, name: string, weight: number, overrideStatus: string): Promise<void>;
        updateProfileIndicator(profileId: number, indicatorId: number, weight: number, overrideStatus: string): Promise<void>;
        deleteProfileIndicator(profileId: number, indicatorId: number): Promise<void>;
        fetchRiskProfiles(): Promise<void>;
        createRiskProfile(name: string, capital: number, riskPct: number, leverage: number): Promise<void>;
        deleteRiskProfile(id: number): Promise<void>;
        calculateRisk(overrides?: { capital?: number; leverage?: number; commissionPct?: number }): Promise<void>;
        fetchFeeTable(): Promise<void>;
        calculateCommissionProjection(): Promise<void>;
        fetchExchangeKeys(): Promise<void>;
        addExchangeKey(): Promise<void>;
        deleteExchangeKey(id: number): Promise<void>;
        fetchTradeLedger(): Promise<void>;
        fetchTradeJournal(limit?: number): Promise<void>;
        updateJournalNotes(id: number, notes: string, score: number): Promise<void>;
        fetchSystemStatus(): Promise<void>;
        fetchObservabilityBuffers(symbol: string): Promise<void>;
        fetchTrades(): Promise<void>;
        exportJournalCSV(): void;
        exportJournalJSON(): void;
    }
}
