// Shared TypeScript interfaces for the Trading Platform dashboard.

// Re-export statistics types for backward compatibility
export type { CoreStats, DailyActivity, DailyPnl, HourlyWinRate, WeekdayWinRate, DirectionBreakdown, DashboardStats, TradeLedgerRecord, TradeJournalRecord, StyleSegment, TraderStyleBreakdown, StreakMetrics, CalendarDay, PairStat, DailyCommission, FeePnlRatio, MonthlySummary } from './types/stats';

// ================================================================
// 1. Decision Profiles & Scoring
// ================================================================

export interface IndicatorRule {
    id: number;
    profile_id: number;
    indicator_name: string;
    weight: number;
    override_status: string;
}

export interface DecisionProfile {
    id: number;
    profile_name: string;
    long_threshold: number;
    short_threshold: number;
    indicators: IndicatorRule[];
}

export interface IndicatorResult {
    indicator_name: string;
    signal: string;
    weight: number;
    weighted_contribution: number;
    override_active: boolean;
}

export interface DecisionScore {
    profile_name: string;
    score: number;
    recommendation: string;
    momentum_bias: number;
    indicator_results: IndicatorResult[];
}

// ================================================================
// 2. Risk & Commission
// ================================================================

export interface RiskProfile {
    id: number;
    profile_name: string;
    /**
     * Decimal fields are serialized as strings from the backend
     * (`#[serde(with = "rust_decimal::serde::str")]`) to preserve full
     * precision. Parse via `parseFloat()` or `new Decimal(value)` before use.
     */
    capital: string;
    max_risk_pct: string;
    leverage: number;
    commission_pct: string;
    funding_rate_8h: string;
    spread: string;
}

export interface RiskCalculation {
    risk_capital: string;
    price_distance: string;
    position_size_units: string;
    position_notional: string;
    leverage_required: string;
    leverage_selected: number;
    margin_required: string;
    liquidation_price: string;
    risk_reward_ratio: string | null;
    estimated_profit: string;
    total_fees: string;
    net_pnl: string;
}

export interface FeeTableRow {
    exchange_fee_pct: number;
    leverage: number;
    capital: number;
    min_profit_pct_to_cover_fees: number;
    fees_in_dollars: number;
}

export interface FeeBreakdown {
    maker_fee_pct: number;
    taker_fee_pct: number;
    order_type: string;
    effective_fee_pct: number;
    entry_1_fees: number;
    entry_2_fees: number;
    total_fees: number;
    funding_rate_8h: number;
    funding_cost: number;
}

export interface EntryMetrics {
    entry_number: number;
    entry_price: number;
    stop_loss_price: number;
    take_profit_price: number;
    capital_allocated: number;
    capital_pct: number;
    position_size_units: number;
    position_notional: number;
    margin_required: number;
    risk_amount: number;
    potential_profit: number;
    fees: number;
    net_profit: number;
}

export interface CommissionProjection {
    direction: string;
    leverage: number;
    total_capital: number;
    total_position_notional: number;
    total_margin_required: number;
    weighted_avg_entry: number;
    effective_stop_loss: number;
    effective_take_profit: number;
    total_risk_amount: number;
    fee_breakdown: FeeBreakdown;
    entry_1: EntryMetrics;
    entry_2: EntryMetrics;
    max_gain_scenario: number;
    max_loss_scenario: number;
    max_gain_net_after_fees: number;
    max_loss_net_after_fees: number;
    trade_viable: boolean;
    viability_reason: string;
    min_profit_pct_to_cover_fees: number;
    required_price_move_pct: number;
}

// ================================================================
// 3. Ingestion & Telemetry
// ================================================================

/** Dual-representation normalized indicator DTO (mirrors the Rust NormalizedIndicatorValue). */
export interface IndicatorDto {
    raw_value: number;
    normalized: number;
    state_label: string;
    values?: Record<string, number> | null;
    signals?: IndicatorSignal[];
    confidence?: number;
}

/** Optional per-snapshot statistical context block (L1-native Monte Carlo / z-scores).
 *  Wire shape mirrors `core_domain::models::StatisticalContext` exactly:
 *  `{ close_z, rsi_z, macd_z, monte_carlo_expected, monte_carlo_stdev }`.
 *  Populated on live completed frames by the SIL statistics engine; consumed
 *  by no UI panel today (placeholder semantics). */
export interface StatisticalContext {
    close_z?: number | null;
    rsi_z?: number | null;
    macd_z?: number | null;
    monte_carlo_expected?: number | null;
    monte_carlo_stdev?: number | null;
}

/** Candle quality envelope — only present on completed snapshots. */
export type SequenceIntegrity = 'VALID' | 'OUT_OF_ORDER' | 'DUPLICATE';
export interface CandleQualityEnvelope {
    quality_score: number;
    is_valid: boolean;
    is_gap_filled: boolean;
    had_outliers_rejected: boolean;
    spike_detected: boolean;
    is_stale: boolean;
    sequence_integrity: SequenceIntegrity;
    gap_since_last: number;
    validated_at: number;
}

/** Liquidity activation subset (Phase 0/1/2/3 toggles). */
export interface LiquidityActivation {
    enabled: boolean;
    liquidation_feed: boolean;
    cluster_estimation: boolean;
    signals: boolean;
}

/** Optional activation block surfaced from the snapshot. */
export interface MetricsConfig {
    disabled_indicators: string[];
    disabled_signals: Array<[string, string]>;
    disabled_signal_kinds: string[];
    liquidity: LiquidityActivation;
    config_version: number;
}

/**
 * Top-level per-timeframe snapshot envelope (mirrors the Rust `MarketSnapshot`).
 * This is what the WebSocket broadcasts per `(symbol, timeframe_secs)`. Most
 * fields are optional because some only populate on completed candles.
 */
export interface MarketSnapshot {
    /** v11.9: wire duration label ("1s".."1d"). The WS handler stamps it
     *  on every frame; the frontend dispatcher reads it via the raw
     *  envelope before applying. */
    timeframe_label?: string | null;
    exchange?: string | null;
    symbol: string;
    timeframe_secs: number;
    timestamp: number;
    is_completed?: boolean;
    mid_price?: number | string | null;
    bid_price?: number | string | null;
    ask_price?: number | string | null;
    bid_size?: number | string | null;
    ask_size?: number | string | null;
    funding_rate?: number | string | null;
    open?: number | string | null;
    high?: number | string | null;
    low?: number | string | null;
    close?: number | string | null;
    volume?: number | string | null;
    average_volume?: number | string | null;
    indicators: IndicatorMap;
    /** Per-timeframe pipeline lifecycle (v6.5, 03-01-06). */
    pipeline_state?: CandlePipelineState;
    /** Per-indicator operational lifecycle (v6.5, 03-02-15). */
    indicator_lifecycle?: IndicatorLifecycleMap;
    context?: MarketContext | null;
    alignment?: AlignmentMatrix | null;
    analysis?: AnalysisMatrix | null;
    risk?: RiskMatrix | null;
    advisory?: AdvisoryMatrix | null;
    open_interest?: number | string | null;
    oi_delta_pct?: number | string | null;
    mark_price?: number | string | null;
    index_price?: number | string | null;
    mark_index_spread_pct?: number | null;
    prev_day_px?: number | string | null;
    statistical_context?: StatisticalContext | null;
    decision_context?: DecisionContext | null;
    opportunity?: OpportunityMatrix | null;
    liquidity_signals?: LiquiditySignal[];
    metrics_config?: MetricsConfig | null;
    risk_profile?: number | null;
    liquidity?: LiquidityFlow | null;
    cluster?: LiquidationClusterMatrix | null;
    /** Per-TF volume profile (audit M4: the wire field was consumed by
     *  websocket.svelte.ts but absent from this type). */
    volume_profile?: VolumeProfileSnapshot | null;
    quality_envelope?: CandleQualityEnvelope | null;
}

// ── Signals (mirror Rust shared::indicators::normalized signal model) ──
export type SignalKind =
    | 'Divergence' | 'Crossover' | 'Threshold' | 'Breakout' | 'BandTouch'
    | 'ZeroLineCross' | 'CompressionRelease' | 'LevelTest' | 'TrendFlip'
    | 'VolumeClimax' | 'StackChange' | 'PatternForming';
export type SignalDirection = 'Bullish' | 'Bearish' | 'Neutral';
export type SignalStatus = 'Potential' | 'Confirmed' | 'Active';
export interface SignalPoint { time: number; value: number; }
export interface IndicatorSignal {
    kind: SignalKind;
    direction: SignalDirection;
    status: SignalStatus;
    label: string;
    strength: number;
    age_bars?: number;
    /** v9: WEAK / MODERATE / STRONG / EXTREME per the strategy's `l1.signals.strength_buckets`. */
    strength_label?: string;
    points?: SignalPoint[] | null;
}

// ── Per-indicator operational lifecycle (v6.5, 03-02-15) ──
export type IndicatorLifecycleState = 'Loading' | 'Live' | 'Stale' | 'Failed';
/**
 * Feed classification (v6.6+). Mirrors
 * `core_domain::indicator_dtos::FeedState`. Default is `Live` when the
 * field is absent on the wire so older snapshots deserialize cleanly.
 * `WaitingFeed` indicates the lifecycle is Live but no value-map entry
 * exists yet (e.g. Bitget ticker channel hasn't delivered
 * `holdingAmount`); the dashboard renders this as `WAITING FEED ⏳`
 * distinct from `SILENT ⚡` which means "feed arrived and said zero".
 */
export type FeedState = 'Live' | 'WaitingFeed' | 'Silent' | 'Stale';
export interface IndicatorLifecycleStatus {
    state: IndicatorLifecycleState;
    bars_seen: number;
    bars_required: number;
    last_updated_at?: number | null;
    last_error?: string | null;
    stale_threshold_secs: number;
    /** v6.6+ — set to `WaitingFeed` when lifecycle is Live but no value
     *  arrived yet. Optional so older snapshots that omit the field
     *  continue to render as before. */
    feed_state?: FeedState;
    /** PRI-12 (v6.10.7) — real (non-synthetic) completed candles seen.
     *  `None` when the pipeline cannot distinguish provenance yet. */
    bars_seen_real?: number | null;
    /** Legacy v6.5 bit kept for the SILENT ⚡ path; true when the
     *  reading is silent (raw=0, no signals, no state_label). */
    silent?: boolean;
}
export type IndicatorLifecycleMap = Record<string, IndicatorLifecycleStatus>;

// ── Per-timeframe pipeline lifecycle (v6.5, 03-01-06) ──
export type CandlePipelineState =
    | 'INITIALIZING' | 'LOADING' | 'LIVE' | 'STALE' | 'FAILED';

// ── Market context + Metrics Panel (meta-intelligence) ──
export interface ContextDimension {
    score: number;
    confidence: number;
    label: string;
}
export interface MarketContext {
    trend: ContextDimension;
    momentum: ContextDimension;
    volatility: ContextDimension;
    volume: ContextDimension;
    liquidity: ContextDimension;
    regime: string;
    overall_score: number;
    overall_label: string;
}
export interface MonitorTimeframe {
    label: string;
    timeframe_secs: number;
    regime: string;
    overall_score: number;
    overall_label: string;
    confluence_score: number;
}
export interface MtfIndicatorRow {
    key: string;
    display_name: string;
    per_tf: number[];
    agreement: number;
}
export interface MtfConfirmation {
    trend_agreement_pct: number;
    structural_trend: string;
    rows: MtfIndicatorRow[];
}
export interface MonitorResponse {
    symbol: string;
    timeframes: MonitorTimeframe[];
    mtf: MtfConfirmation;
    market_context: MarketContext | null;
}

// ── Alignment Matrix (cross-timeframe MTF — 10 dimensions) ──
export interface TfAlignmentInfo {
    timeframe: string;
    timeframe_secs: number;
    trend_score: number;
    momentum_score: number;
    overall_score: number;
    regime: string;
    active_signals: number;
    price: number;
}

export interface AlignmentDimension {
    score: number;
    state: string;
    confidence: number;
}

export interface AlignmentMatrix {
    symbol: string;
    timeframes_present: number;
    dimensions: AlignmentDimension[];
    mtf_trend_alignment: number;
    mtf_momentum_alignment: number;
    mtf_volume_alignment: number;
    mtf_volatility_alignment: number;
    mtf_overall_score: number;
    mtf_overall_label: string;
    /** Effective blend weights `[["T", 0.55], …]` applied to the
     *  composite — mirrors the backend formula (v6.10.16 thin-participation
     *  reweight; empty/absent on legacy payloads → standard 50/30/10/10). */
    blend_weights?: Array<[string, number]>;
    timeframe_alignments: TfAlignmentInfo[];
    signal_cross_tf_count: number;
    trend_agreement_pct: number;
}

// ── Analysis Matrix (market interpretation — 10 components) ──
export type MarketBias = 'StrongBullish' | 'Bullish' | 'Neutral' | 'Bearish' | 'StrongBearish';
// Wire casing is PascalCase (analysis.rs has no serde rename on these
// enums): 'TrendingBull', 'Range', 'Accumulation', ...
export type MarketRegime = 'TrendingBull' | 'TrendingBear' | 'Range' | 'Accumulation' | 'Distribution' | 'Expansion' | 'Contraction' | 'Transition';
export type TrendAssessment = 'Weak' | 'Developing' | 'Healthy' | 'Strong' | 'Exhausted';
export type MomentumAssessment = 'Increasing' | 'Stable' | 'Weakening' | 'Exhausted' | 'Reversing';
export type StructureAssessment = 'Strong' | 'Healthy' | 'Weak' | 'Broken' | 'Unknown';
export type VolatilityAssessment = 'Compressed' | 'Normal' | 'Expanding' | 'Extreme' | 'Unstable';
export type VolumeAssessment = 'Weak' | 'Normal' | 'Strong' | 'Exceptional';
export type OpportunityType = 'TrendContinuation' | 'Breakout' | 'Pullback' | 'MeanReversion' | 'Reversal' | 'LiquiditySqueeze' | 'Scalp' | 'NoClearOpportunity';
export type QualityLevel = 'Poor' | 'Weak' | 'Average' | 'Good' | 'Excellent';
export type MarketPhase = 'Accumulation' | 'Markup' | 'Distribution' | 'Markdown' | 'Unknown';

export interface AnalysisMatrix {
    symbol: string;
    bias: MarketBias;
    /** UI-facing confidence in [0.0, 1.0]. Mirrors `state_confidence`
     *  on the wire (no serde rename — the literal JSON key is
     *  `state_confidence`). Added so consumers can read
     *  `analysis.confidence` directly.
     */
    confidence: number;
    /** Canonical backend field (JSON key `state_confidence`).
     *  Kept in sync with `confidence`; both refer to the same value. */
    state_confidence: number;
    /** Signed alignment score ∈ [−1, +1] (mtf_overall_score / 100). */
    market_bias_score?: number;
    market_regime: MarketRegime;
    trend_assessment: TrendAssessment;
    momentum_assessment: MomentumAssessment;
    structure_assessment: StructureAssessment;
    volatility_assessment: VolatilityAssessment;
    volume_assessment: VolumeAssessment;
    market_quality: QualityLevel;
    /** Numeric market-quality score in [0, 100] — distinct from
     *  categorical `market_quality` (`QualityLevel` enum). */
    market_quality_score: number;
    /** v6.12 numeric companions: the exact 0-100 alignment dimension
     *  scores each qualitative assessment is bucketed from — the
     *  disaggregated siblings of `market_quality_score`, rendered as
     *  badges on the Analysis panel. Absent on the empty sentinel. */
    trend_score?: number | null;
    momentum_score?: number | null;
    structure_score?: number | null;
    volatility_score?: number | null;
    volume_score?: number | null;
    /** v6.10.21 traceability: the L3 regime-input raw values the
     *  `rationale` quotes (representative first-TF-wins bbwp/adx). The
     *  pair-level matrix mirror is per-slot last-writer-wins, so these
     *  pin the exact inputs used regardless of the exporting slot. */
    representative_bbwp?: number | null;
    representative_adx?: number | null;
    /** Wyckoff-style market-cycle phase (L3). */
    market_phase: MarketPhase;
    market_interpretation: string;
    rationale: string;
    supporting_signals: string[];
    contradicting_signals: string[];
    timeframes_considered: number;
}

// ── Risk Matrix (risk evaluation — 9 dimensions) ──
export type RiskLevel = 'VeryLow' | 'Low' | 'Moderate' | 'High' | 'Extreme';
export type RiskState = 'Stable' | 'Increasing' | 'Elevated' | 'Critical' | 'Improving';

export interface RiskDimension {
    score: number;
    level: RiskLevel;
    state: RiskState;
    confidence: number;
    evidence: string[];
    /** ATR(14) ÷ top-of-book bid-ask spread (raw price units) — execution
     *  friction gauge. Present only on `execution_risk` (v6.11 L5). */
    volatility_to_spread_ratio?: number | null;
}

export interface RiskMatrix {
    symbol: string;
    market_risk: RiskDimension;
    volatility_risk: RiskDimension;
    /**
     * Phase 3: renamed from `liquidity_risk` for clarity. This dimension
     * covers execution liquidity / market depth, not positional
     * liquidation liquidity.
     */
    execution_liquidity_risk?: RiskDimension;
    structure_risk: RiskDimension;
    momentum_risk: RiskDimension;
    signal_risk: RiskDimension;
    execution_risk: RiskDimension;
    /** Phase 3: cascade risk — danger from forced liquidation cascades. */
    cascade_risk?: RiskDimension;
    overall_risk: RiskDimension;
}

// ── Advisory Matrix (human-facing guidance — 10 components) ──
export type DirectionalGuidance = 'StrongLong' | 'Long' | 'Neutral' | 'Short' | 'StrongShort' | 'AvoidDirectionalExposure';
export type MarketStance = 'Aggressive' | 'Constructive' | 'Neutral' | 'Cautious' | 'Avoid';
export type OpportunityClass = 'TrendContinuation' | 'Breakout' | 'Pullback' | 'MeanReversion' | 'Reversal' | 'LiquiditySqueeze' | 'Scalp' | 'NoClearOpportunity';
export type StrategyEnvironment = 'TrendFollowing' | 'Breakout' | 'MeanReversion' | 'HighVolatility' | 'LowActivity' | 'Unfavorable';
export type EntryGuidance = 'Immediate' | 'WaitForConfirmation' | 'Pullback' | 'Breakout' | 'NoEntryContext';
export type ExitGuidance = 'TrendWeakening' | 'MomentumExhaustion' | 'StructureBreakdown' | 'RiskIncreasing' | 'NoWarning';
export type ProtectionStrategy = 'StructureBased' | 'VolatilityBased' | 'ATRBased' | 'SRBased' | 'NoRecommendation';
export type TargetStrategy = 'ResistanceBased' | 'RRBased' | 'VolatilityBased' | 'TrailingMethod' | 'NoRecommendation';

export interface AdvisoryMatrix {
    symbol: string;
    directional_guidance: DirectionalGuidance;
    market_stance: MarketStance;
    opportunity_classification: OpportunityClass;
    strategy_environment: StrategyEnvironment;
    entry_guidance: EntryGuidance;
    exit_guidance: ExitGuidance;
    protection_strategy: ProtectionStrategy;
    target_strategy: TargetStrategy;
    confidence_assessment: number;
    /**
     * Stop-loss distance as a percentage on the wire (e.g. `2.5` = 2.5%
     * from entry; the backend clamps to `[0.5, 15.0]` percent). Surfaced
     * from the Rust `AdvisoryMatrix::stop_loss_distance_pct` (canonical
     * type-boundary handoff f64 at L6 → Decimal at TAE).
     */
    stop_loss_distance_pct: number;
    /**
     * Per-symbol cascade risk score (0..100) carried through from the
     * L5 Risk Matrix `cascade_risk.score`. Used by the L7 Overview
     * aggregation; surfaced here so the Recommendation tab can show
     * it next to the danger band.
     */
    cascade_risk_score: number;
    /**
     * Synoptic favorability of entering a position — high score =
     * dangerous. Wire key is `environment_favorability` (Rust
     * `crates/core-domain/src/advisory.rs::AdvisoryMatrix::environment_favorability`).
     * The Recommendation tab projects this as the green/amber/red
     * band on the safety-flags row.
     */
    environment_favorability: RiskDimension;
    /** Setup-efficiency metric: `market_quality_score ÷ overall_risk.score`
     *  (both unipolar 0-100; higher = better). `null` when overall risk is
     *  zero (v6.11 L6). */
    quality_to_risk_ratio?: number | null;
    /** v9: the strategy's risk-ceiling soft-block stamp (readiness floors
     *  at WATCH when breached). */
    risk_blocked?: boolean;
    final_recommendation: string;
}

// ── Decision Context (quantitative decision metadata — L6) ──
export interface DecisionContext {
    /** Confluence score in [-100, +100]. */
    score: number;
    /** Directional bias (same PascalCase as AnalysisMatrix.bias). */
    bias: MarketBias;
    /** Score-band confidence in [0.0, 1.0]. */
    score_confidence: number;
    /**
     * Synoptic entry danger — high score = dangerous to enter.
     * Wire shape (from Rust `crates/core-domain/src/decision_context.rs::DecisionContext.entry_danger`):
     * a `RiskDimension`-shaped object with `{ score, level, state, confidence }`.
     * Historically some legacy sends may arrive as bare `{ score: 0 }`; the
     * consumer (`decisionRank.ts`) reads `.score` defensively.
     */
    entry_danger: RiskDimension;
    /** Synthesised expected reward:risk ratio. */
    expected_reward_risk_ratio: number;
    /** Trade-readiness token: "READY" | "FORMING" | "WATCH" | "STAND_ASIDE". */
    trade_readiness: string;
    /** Indicators that contributed to the decision. */
    contributing_indicators: string[];
    /** Long-side normalized probability (0–100, integer). Source of truth for the "X% long" display. */
    long_probability?: number;
    /** Short-side normalized probability (0–100, integer). */
    short_probability?: number;
    /** Hold (no-position) normalized probability (0–100, integer). */
    hold_probability?: number;
    /** Net directional bias (long − short) in percentage points, range [-100, +100]. */
    net_bias_pct?: number;
    /** v6.10.19 (P6): the graded-lean floors adjusted this split — the
     *  directional read is structurally boosted (LEAN annotation). */
    lean_floor_applied?: boolean;
}

// ── Overview Matrix (global market synthesis — 9 components) ──
export type GlobalBias = 'STRONG_BULLISH' | 'BULLISH' | 'NEUTRAL' | 'BEARISH' | 'STRONG_BEARISH' | 'MIXED';
export type MarketBreadth = 'VERY_WEAK' | 'WEAK' | 'BALANCED' | 'POSITIVE' | 'STRONG_POSITIVE' | 'NEGATIVE' | 'STRONG_NEGATIVE';
export type SyncLevel = 'HIGHLY_SYNCHRONIZED' | 'SYNCHRONIZED' | 'MIXED' | 'FRAGMENTED' | 'HIGHLY_FRAGMENTED';
export type HealthLevel = 'POOR' | 'WEAK' | 'NEUTRAL' | 'HEALTHY' | 'STRONG';

export interface AssetRank {
    symbol: string;
    score: number;
    bias: string;
    confidence: number;
    regime: string;
    risk_level: string;
    /// v6.11+ — Mirrors `AlignmentMatrix.mtf_overall_score` for this
    /// symbol ∈ [-100, 100]. `0` when no alignment is available.
    mtf_score?: number;
    /// v6.11+ — Mirrors `AlignmentMatrix.mtf_overall_label`
    /// (`STRONG_BULL_MTF` / `WEAK_BULL_MTF` / `NEUTRAL_MTF` /
    /// `WEAK_BEAR_MTF` / `STRONG_BEAR_MTF` / `NO_DATA`).
    mtf_label?: string;
}

export interface RiskDistribution {
    low_pct: number;
    moderate_pct: number;
    high_pct: number;
    risk_environment: string;
}

export interface OverviewMatrix {
    global_market_bias: GlobalBias;
    market_breadth: MarketBreadth;
    regime_distribution: Record<string, number>;
    opportunity_distribution: Record<string, number>;
    risk_distribution: RiskDistribution;
    asset_ranking: AssetRank[];
    market_synchronization: SyncLevel;
    market_health: HealthLevel;
    global_summary: string;
    instance_count: number;
    active_symbols: string[];
    /// v6.9+ — Continuous signed breadth percentage ∈ [-100, 100].
    /// Source of the UI's −100% to +100% breadth gauge and the
    /// input to `market_breadth` and `market_synchronization`.
    breadth_pct?: number;
    /// v6.9+ — True when fewer than 3 symbols are active
    /// (`active_symbols.len() < 3`, overview.rs) — the I-10 STRONG_*
    /// display-demotion gate on the header/KPI/export.
    low_coverage?: boolean;
    /// v6.9+ — Cross-symbol aggregate of L5 `cascade_risk`.
    cascade_risk_index?: RiskDimension;
    /// v6.9+ — Market-wide danger index consumed by the PME safety
    /// veto. `0.6 × high_pct + 0.4 × sync_penalty`.
    systemic_risk_score?: number;
    /// v6.11+ — Count of assets per `AlignmentMatrix.mtf_overall_label`
    /// (`STRONG_BULL_MTF` / `WEAK_BULL_MTF` / `NEUTRAL_MTF` /
    /// `WEAK_BEAR_MTF` / `STRONG_BEAR_MTF` / `NO_DATA`). Mirrors
    /// the shape of `opportunity_distribution` (per-type counts,
    /// not a partition).
    alignment_distribution?: Record<string, number>;
    /// v6.11+ — Mean of all per-symbol
    /// `AlignmentMatrix.mtf_overall_score` ∈ [-100, 100]. The
    /// cross-timeframe counterpart to `breadth_pct` (which is
    /// cross-symbol). `0` when no alignments are available.
    alignment_consensus_index?: number;
    /// v6.11+ — Mean of all per-symbol
    /// `AlignmentMatrix.trend_agreement_pct` ∈ [0, 100]. Distinct
    /// from `market_synchronization` (cross-symbol, derived from
    /// `breadth_pct`).
    multi_tf_agreement_pct?: number;
    /// v7.2 parity — server-computed hero verdict (TRADE / WAIT /
    /// STAND_ASIDE). Single source for the GUI + CLI overview panels.
    hero?: OverviewHero | null;
    /// v7.2 parity — per-instance asset-ranking rows (price, signal,
    /// direction, R:R, confidence, MTF, risk, updated + the top-setup
    /// entry/target/stop columns). The GUI's 15-column table and the CLI
    /// renderer read the same rows.
    overview_rows?: OverviewRow[];
    /// v7.2 parity — signal-quality buckets (strong/moderate/weak).
    signal_quality?: SignalQuality | null;
    /// v7.2 parity — direction counts (long/short/neutral).
    direction_distribution?: DirectionDistribution | null;
    /// v7.2 parity — market-health sub-dimension bars.
    market_health_dims?: MarketHealthDims | null;
}

export type HeroVerdict = 'TRADE' | 'WAIT' | 'STAND_ASIDE';

export interface OverviewHero {
    verdict: HeroVerdict;
    actionable_count: number;
    candidate_count: number;
    best_symbol: string | null;
    best_score: number;
    best_direction: 'LONG' | 'SHORT' | 'NEUTRAL';
    best_confidence: number;
    best_rr: number;
    instance_count: number;
}

export interface OverviewRow {
    symbol: string;
    price: number;
    bias: string;
    signal: 'BUY' | 'SELL' | 'WAIT';
    direction: 'LONG' | 'SHORT' | 'NEUTRAL';
    rr: number;
    score: number;
    confidence: number;
    mtf_score: number;
    mtf_label: string;
    risk: number;
    /// Top-setup of the Opportunity Layer — resolved side of the displayed
    /// bracket (`LONG` / `SHORT` / `NEUTRAL`), server-computed once for the
    /// GUI + CLI parity contract (01-10 §5).
    setup_side?: string;
    /// Top-setup entry zone low/high bound (0 = N/A).
    entry_low?: number;
    entry_high?: number;
    /// Top-setup target (take-profit) zone low/high bound (0 = N/A).
    target_low?: number;
    target_high?: number;
    /// Top-setup stop-loss (invalidation) level (0 = N/A).
    invalidation?: number;
    updated_ts: number;
    active: boolean;
}

export interface SignalQuality {
    strong: number;
    moderate: number;
    weak: number;
}

export interface DirectionDistribution {
    long: number;
    short: number;
    neutral: number;
}

export interface HealthBar {
    label: string;
    value: number;
    available: boolean;
    contributing_instances: number;
}

export interface MarketHealthDims {
    bars: HealthBar[];
    active_instance_count: number;
}

// ── Indicator registry manifest (mirror Rust shared::indicators::registry) ──
export type IndicatorGroup =
    | 'Trend' | 'Momentum' | 'Volume' | 'Volatility' | 'Structure' | 'Regime' | 'Institutional' | 'DerivativesData';
export type IndicatorClass = 'Leading' | 'Hybrid' | 'Lagging';
export type RenderKind = 'Pane' | 'PriceOverlay' | 'PriceLevels' | 'Marker';
/**
 * How the indicator contributes to the directional confluence and the
 * UI Norm column. Mirrors `IndicatorNormalizationMode` in
 * `crates/market-analyzer/src/indicators/registry.rs`.
 *
 * - `Directional` — emits a real `[-1, 1]` score; UI shows the score.
 * - `ContextOnly` — non-directional gate; `normalized` is contractually
 *   0.0; UI shows `N/A` to honor the published contract.
 * - `EventOnly` — overlay (Hull MA); `normalized` is contractually 0.0
 *   and the value is read from `raw_value` or `values`; UI shows `N/A`
 *   and the value lives in the Raw column.
 */
export type IndicatorNormalizationMode = 'Directional' | 'ContextOnly' | 'EventOnly';
export interface IndicatorMeta {
    key: string;
    display_name: string;
    group: IndicatorGroup;
    class: IndicatorClass;
    render: RenderKind;
    directional: boolean;
    supports_divergence: boolean;
    signal_types: SignalKind[];
    default_weight: number;
    default_enabled: boolean;
    config_params: string[];
    value_format: string;
    value_source: string;
    color: string;
    guide_section: string;
    /** Whether this indicator recomputes on shadow (live) ticks. */
    updates_on_shadow?: boolean;
    /** How the indicator contributes to the UI Norm column. */
    normalization_mode?: IndicatorNormalizationMode;
    /** Candle bars required before a real reading (registry `bars_required`). */
    bars_required?: number;
    /** Registry data source (e.g. CandleBased, DerivativesWs, OrderBookWs). */
    data_source?: string | null;
    /** Signal capability flags (registry `signal_capability`). */
    signal_capability?: string[] | null;
}

export type IndicatorMap = Record<string, IndicatorDto>;

export function emptyIndicator(): IndicatorDto {
    return { raw_value: 0, normalized: 0, state_label: 'UNKNOWN', values: null };
}

/// v11.9: duration-keyed timeframe identity. A timeframe IS its duration
/// in seconds — there are no named slots. The supported pool is the
/// closed 14-duration set below; activation is any subset of 1..=14
/// durations. The derived label ("1s".."1d") travels on the wire as
/// `timeframe_label` and is stamped by the analyzer onto every snapshot.
export const DURATIONS: readonly number[] = [
    1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400, 43200, 86400,
] as const;

/// Canonical display label for a supported duration ("1s".."1d").
/// Durations outside the pool resolve to "<secs>s".
export function tfLabel(secs: number): string {
    switch (secs) {
        case 1: return '1s';
        case 3: return '3s';
        case 5: return '5s';
        case 15: return '15s';
        case 30: return '30s';
        case 60: return '1m';
        case 180: return '3m';
        case 300: return '5m';
        case 900: return '15m';
        case 1800: return '30m';
        case 3600: return '1h';
        case 14400: return '4h';
        case 43200: return '12h';
        case 86400: return '1d';
        default: return `${secs}s`;
    }
}

/// True when `secs` is a member of the supported pool.
export function isSupportedDuration(secs: number): boolean {
    return (DURATIONS as readonly number[]).includes(secs);
}

export interface TimeframeTelemetry {
    /// v11.9: authoritative duration identity in seconds (a member of
    /// `DURATIONS`). The duration IS the slot; the label is derived via
    /// `tfLabel(secs)`.
    slot: number;
    symbol: string;
    exchange: string;
    barDurationSec: number;
    indicators: IndicatorMap;
    /// P0 global-store mirror: live candle history kept warm by
    /// `websocket.svelte.ts` `ingestLiveSnapshot` / `appendLiveCandle`
    /// so tab-switch does not lose sub-minute history even when the
    /// module `candleCache` is cleared. Bounded to 1000, same as
    /// `HIST_BUFFER_MAX`. `undefined` until first completed candle.
    liveCandleCache?: import('./lib/indicatorHistory').CandleOHLCV[];
    /// P0 live-history count for staleness UI (e.g. warmup badge).
    liveHistoryCount?: number;
    priceText: string;
    volText: string;
    avgVolText: string;
    showPatterns: boolean;
    isCompleted: boolean;
    latestSnapshot: Record<string, unknown> | null;
    historyPrices: number[];
    /** v6.5: per-timeframe pipeline lifecycle (03-01-06). */
    pipelineState?: CandlePipelineState;
    /** v6.5: per-indicator operational lifecycle map (03-02-15). */
    indicatorLifecycle?: IndicatorLifecycleMap;
    /** Per-TF synthesis block from the analyzer (L1 MarketContext). */
    context?: MarketContext | null;
    /** Phase 1: per-candle liquidity flow (real liquidation events). */
    liquidity?: LiquidityFlow;
    /** Phase 2: estimated liquidation cluster matrix (per-TF candle cadence, 5-min TTL). */
    cluster?: LiquidationClusterMatrix;
    /** Phase 3: liquidity signals derived from flow + cluster. */
    liquiditySignals?: LiquiditySignal[];
    /** Volume profile snapshot — per-timeframe aggregated volume distribution. */
    volumeProfile?: VolumeProfileSnapshot;
    showEmas: boolean;
    showBb: boolean;
    showVwap: boolean;
    showVolume: boolean;
    showAdx: boolean;
    showAtr: boolean;
    showRsi: boolean;
    showMacd: boolean;
    showSqueeze: boolean;
    showBbwp: boolean;
    showFib: boolean;
    showRvol: boolean;
    showStochastic: boolean;
    showChandeMo: boolean;
    showSupertrend: boolean;
    showKeltner: boolean;
    showDonchian: boolean;
    showIchimoku: boolean;
    showPsar: boolean;
    showStddevChan: boolean;
    showObv: boolean;
    showCmf: boolean;
    showMfi: boolean;
    showHv: boolean;
    showAroon: boolean;
    showChoppiness: boolean;
    showLinregSlope: boolean;
    showZscore: boolean;
    showLiqHeatmap: boolean;
    /**
     * v7.0-prod — leverage tiers (integer × in [1, 100]) the operator
     * currently highlights on the liquidation heatmap overlay. Matching
     * clusters amplify in intensity; non-matching dim. Per-TF so that
     * micro may show one set while macro shows another.
     */
    heatmapLeverageTiers: number[];
    showVolumeProfile: boolean;
    showWilliamsR: boolean;
    showCci: boolean;
    showForceIdx: boolean;
    showFunding: boolean;
    showOpenInterest: boolean;
    showOiDelta: boolean;
    showOrderFlowDepth: boolean;
    showDerivativeRibbon: boolean;
    showPivotPoints: boolean;
    showSupportResistance: boolean;
    showSmcStructure: boolean;
    showSmcLiquidity: boolean;
    showFvgZones: boolean;
    showOrderBlocks: boolean;
    showAnchoredVwap: boolean;
    showSpread: boolean;
    showAwesome: boolean;
    emaFastVal: number;
    emaMediumVal: number;
    emaSlowVal: number;
    emaLongVal: number;
    rsiPeriodVal: number;
    macdFastVal: number;
    macdSlowVal: number;
    macdSignalVal: number;
    adxPeriodVal: number;
    atrPeriodVal: number;
    squeezePeriodVal: number;
    bbwpPeriodVal: number;
    bbwpLookbackVal: number;
    stochKPeriodVal: number;
    stochDPeriodVal: number;
    stochSPeriodVal: number;
    chandemoPeriodVal: number;
    supertrendPeriodVal: number;
    supertrendMultiplierVal: number;
    keltnerEmaPeriodVal: number;
    keltnerAtrPeriodVal: number;
    keltnerMultiplierVal: number;
    donchianPeriodVal: number;
    obvSmoothingVal: number;
    cmfPeriodVal: number;
    mfiPeriodVal: number;
    hvPeriodVal: number;
    aroonPeriodVal: number;
    chopPeriodVal: number;
    linregPeriodVal: number;
    zscorePeriodVal: number;
    macdExtremeHighVal: number;
    macdExtremeLowVal: number;
    macdContractionVal: number;
    adxTrendThresholdVal: number;
    adxExhaustionThresholdVal: number;
    adxSlopeLookbackVal: number;
    squeezeMinDurationVal: number;
    squeezeBbPeriodVal: number;
    squeezeBbStdDevVal: number;
    squeezeKcPeriodVal: number;
    squeezeKcAtrMultVal: number;
    atrMultiplierVal: number;
    atrTargetRRVal: number;
    volumeAvgPeriodVal: number;
    rvolInstitutionalVal: number;
    rvolClimaxVal: number;
}

/** All feature-panel view keys mountable inside an instance workspace. */
export type CurrentView = 'terminal' | 'monitor' | 'alignment' | 'opportunity' | 'risk' | 'analysis' | 'recommendation' | 'settings' | 'costs' | 'ledger';

export interface InstanceState {
    symbol: string;
    exchange: string;
    isConnected: boolean;
    /// Backend-assigned instance UUID (`inst_<hex>`). Used as the path
    /// parameter for `/api/instances/{instance_id}/...` endpoints.
    /// Populated lazily from `GET /api/instances` or `POST /api/instances`.
    instanceId?: string;
    /// Per-instance execution mode (observe | paper | live), fixed at
    /// launch. Populated from `GET /api/instances` via
    /// `syncInstanceIdsFromList`. Drives mode-aware tabs + banners.
    mode?: 'observe' | 'paper' | 'live';
    /// v11.9 — per-duration telemetry, keyed by the duration in seconds.
    /// The RECORD covers ALL 14 supported durations (total), but only the
    /// ACTIVE durations ever populate. Walk `activeDurations(pair)`, not
    /// the keys.
    terms: Record<number, TimeframeTelemetry>;
    /// v11.9 — the ACTIVE ladder: the configured subset of the supported
    /// duration pool, mirrored from `InstanceSummary.active_secs` on
    /// `GET /api/instances`. Durations beyond the set are INERT: no
    /// snapshots, no WS sockets. `undefined` until the payload arrives —
    /// consumers fall back to all 14 via `activeDurations(pair)`
    /// (lib/terms.ts).
    activeDurations?: number[];
    historyLatestClose: string;
    currentView: CurrentView;
    alignment: AlignmentMatrix | null;
    analysis: AnalysisMatrix | null;
    risk: RiskMatrix | null;
    advisory: AdvisoryMatrix | null;
    /** Decision-context synthesis (L6) — surfaces trade_readiness and
     *  confluence score across the most recent macro snapshot. Extracted
     *  from the WS frame by `applySnapshotToTimeframe` so the Watchlist
     *  Scanner can poll for the first `trade_readiness` value without
     *  having to read every TF's `latestSnapshot`. */
    decisionContext: DecisionContext | null;
    /** L4 opportunity matrix — entry/target/invalidation zones for both
     *  sides plus R:R, time horizon, confluent levels and the 8 evaluated
     *  setup profiles. Only the completed-candle WS frame carries this
     *  payload (`broadcast_live_snapshot` zeroes it for performance), so
     *  it is mirrored at the pair level by `applySnapshotToTimeframe`
     *  rather than read from `terms[1].latestSnapshot` (which gets
     *  overwritten by shadow ticks). */
    opportunity: OpportunityMatrix | null;
    /// Last candle-close timestamp (epoch seconds) whose WS frame was
    /// accepted for the pair-level matrix fields below. Used by the
    /// WebSocket handler as a monotonicity guard so the four slot streams
    /// can't race-write the shared `alignment` / `analysis` / `risk` /
    /// `advisory` / `decisionContext` / `opportunity` fields with stale
    /// or out-of-order payloads. `-Infinity` means "no frame accepted yet".
    /// Per-slot monotonicity guard (PRI-09, v6.10.7): one timestamp per
    /// slot (`micro|fast|slow|macro`), advanced only by completed frames
    /// that carry a matrix payload. The previous single cross-slot
    /// timestamp let a fast slot's matrix-less completed frames (e.g.
    /// sub-minute force-closes every second) pin the guard at wall-clock,
    /// starving every slower slot's matrix frames forever — the sub-minute
    /// matrix deadlock. `-Infinity` = "no matrix frame accepted for this
    /// slot yet".
    lastMatrixTimestampBySlot: Partial<Record<number, number>>;
    /// Last closed-candle close price across any slot that produced a
    /// completed frame. Powers geometry consumers (OpportunitiesPanel,
    /// RecommendationPanel) that need a stable mark price which doesn't
    /// flicker on shadow ticks. `null` until the first completed frame.
    lastCompletedClose: string | null;
    automationEnabled: boolean;
    automationIntervalMode: string;
    automationIntervalValue: number;
    automationIntervalUnit: string;
    priceLineMode: boolean;
    slowIntervalSecs: number;
    normalIntervalSecs: number;
    fastIntervalSecs: number;
    showEmaFast: boolean;
    showEmaMedium: boolean;
    showEmaSlow: boolean;
    showEmaLong: boolean;
    /** LiveTerminal active timeframe (one of the ACTIVE durations, in
     *  seconds) — persisted per-instance so sub-minute selection survives
     *  Charts↔Metrics tab switches and engine view unmounts. */
    activeTf?: number;
}

export interface ScaleInPortion {
    id: number;
    entry_price: number;
    size: number;
    allocated_usd: number;
    portion_number: number;
}

export interface TakeProfitTarget {
    id: number;
    target_price: number;
    size_fraction: number;
    is_hit: boolean;
}

export interface UserTrade {
    id: number;
    timestamp: number;
    symbol: string;
    direction: string;
    outcome: 'WIN' | 'LOSS';
    risk_multiplier: number;
    reward_multiplier: number;
}

// ================================================================
// 4. Supported Timeframe Spectrum (14-tier)
// ================================================================

export interface TimeframeOption {
    label: string;
    seconds: number;
}

export const TIMEFRAME_OPTIONS: TimeframeOption[] = [
    { label: '1 sec', seconds: 1 },
    { label: '3 sec', seconds: 3 },
    { label: '5 sec', seconds: 5 },
    { label: '15 sec', seconds: 15 },
    { label: '30 sec', seconds: 30 },
    { label: '1 min', seconds: 60 },
    { label: '3 min', seconds: 180 },
    { label: '5 min', seconds: 300 },
    { label: '15 min', seconds: 900 },
    { label: '30 min', seconds: 1800 },
    { label: '1 hrs', seconds: 3600 },
    { label: '4 hrs', seconds: 14400 },
    { label: '12 hrs', seconds: 43200 },
    { label: '1 day', seconds: 86400 },
];

// ================================================================
// Phase 1-3: Liquidity Intelligence types
// ================================================================

/**
 * Wire-format enum values are SCREAMING_SNAKE_CASE (the Rust side uses
 * `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` on every liquidity-related
 * enum). Older frontend code compared PascalCase strings — that was a bug;
 * these unions match the actual JSON the server emits.
 */
export type CascadeState = 'NONE' | 'DETECTED' | 'SUSTAINED' | 'EXHAUSTED';
export type LiquidationSide = 'LONG' | 'SHORT';
export type ClusterKind = 'ABOVE_CURRENT_PRICE' | 'BELOW_CURRENT_PRICE' | 'AT_CURRENT_PRICE' | 'DISTANT';
export type LeverageDistributionSource = 'DEFAULT_POWER_LAW' | 'FUNDING_ADAPTIVE' | 'CONFIG_OVERRIDE';

export interface LiquidityFlow {
    long_liquidations_usd: number;
    short_liquidations_usd: number;
    net_liquidation_usd: number;
    event_count: number;
    largest_event_usd: number;
    largest_event_price?: number;
    largest_event_side?: LiquidationSide;
    cascade_state: CascadeState;
    cascade_intensity: number; // 0..100
    /**
     * Price-bucketed USD notional over the rolling 24h window. Keyed by
     * packed `(bucket_index, side)` integer from the Rust accumulator.
     * Bins follow current mid at the time the event was ingested, so
     * bands migrate with the chart rather than being pinned to absolute
     * dollars. Empty when no liquidations have been observed for this
     * symbol (e.g. Hyperliquid without `hyperliquid_user_address` set).
     */
    recent_real_buckets?: Record<string, RealLiquidationBucket>;
}

/** One observed liquidation bucket from `LiquidityFlow.recent_real_buckets`.
 * `bucket_index` is `((price / mid_at_event_time) - 1) / bucket_size_pct`,
 * rounded to int — so `+50` with `bucket_size_pct = 0.001` means "5%
 * above mid". The same `bucket_index` always maps to the same
 * approximate price position while the mid is stable. */
export interface RealLiquidationBucket {
    bucket_index: number;
    side: LiquidationSide;
    price_low: number;
    price_high: number;
    peak_price: number;
    notional_usd: number;
    event_count: number;
    last_updated_ms: number;
}

export interface LeverageAssumptions {
    buckets: number[];
    weights: number[];
    funding_modulation_active: boolean;
    funding_extreme_pct: number;
    source: LeverageDistributionSource;
}

export interface LiquidationCluster {
    price_low: number;
    price_high: number;
    peak_price: number;
    notional_usd: number;
    dominant_leverage: number;
    distance_from_mid_pct: number;
    cluster_kind: ClusterKind;
    magnet_strength: number; // 0..100
}

export interface LiquidationClusterMatrix {
    symbol: string;
    generated_at_ms: number;
    valid_until_ms: number;
    mid_price: number;
    leverage_assumptions: LeverageAssumptions;
    short_clusters: LiquidationCluster[];
    long_clusters: LiquidationCluster[];
    cascade_asymmetry: number;       // [-1, +1]
    total_long_oi_usd: number;
    total_short_oi_usd: number;
    estimation_confidence: number;  // 0..1
}

/**
 * Wire-format SCREAMING_SNAKE_CASE values (matches Rust `LiquiditySignalKind`).
 * The frontend previously used PascalCase and silently miscategorized every
 * incoming liquidity signal — see `LiquidityPanel.svelte` for the fix.
 */
export type LiquiditySignalKind =
    | 'CASCADE_DETECTED'
    | 'CASCADE_SUSTAINED'
    | 'CASCADE_EXHAUSTED'
    | 'LIQUIDITY_VACUUM'
    | 'FUNDING_EXTREME'
    | 'OI_FUNDING_DIVERGENCE'
    | 'MAGNET_ACTIVATED'
    | 'CLUSTER_PRESSURE_HIGH'
    | 'CLUSTER_FORWARD_PRESSURE'
    | 'FUNDING_FLIP'
    | 'OI_PRICE_DIVERGENCE';

/** Wire-format LiquidityDirection (was PascalCase; old code broke styling). */
export type LiquidityDirection = 'BULLISH' | 'BEARISH' | 'NEUTRAL';

export interface LiquiditySignal {
    kind: LiquiditySignalKind;
    direction: LiquidityDirection;
    strength: number;     // 0..100
    confidence: number;   // 0..1
    evidence: string[];
}

// ================================================================
// Volume Profile (per-timeframe, computed by market-analyzer)
// ================================================================

export interface VolumeProfileBin {
    price_low: number;
    price_high: number;
    volume: number;
    buy_volume: number;
    sell_volume: number;
    is_poc: boolean;
    is_value_area: boolean;
}

export interface VolumeProfileSnapshot {
    symbol: string;
    /// v11.9: derived duration label ("1s".."1d").
    timeframe_label: string;
    timeframe_secs: number;
    bins: VolumeProfileBin[];
    poc_price: number;
    value_area_high: number;
    value_area_low: number;
    total_volume: number;
    range_low: number;
    range_high: number;
    num_bins: number;
    timestamp_ms: number;
}

export type QualityWindow = 'one_hour' | 'six_hour' | 'twenty_four_hour';

export interface ConnectionQualityReport {
    window: QualityWindow;
    window_start_ms: number;
    window_end_ms: number;
    uptime_pct: number;
    disconnect_count: number;
    avg_reconnect_ms: number;
    total_data_loss_secs: number;
    reconstructed_candles: number;
    score: number;
}

export interface ClockStatusResponse {
    within_threshold: boolean;
    drift_us: number | null;
    jitter_rms_us: number | null;
    last_poll_ms: number | null;
    breach_count: number;
    breach_action: string;
    ntp_servers: string[];
    sample_count: number;
    threshold_micros: number;
}

export type ExchangeConnectionState = 'Connecting' | 'Connected' | 'Disconnected' | 'Reconnecting' | 'Disabled';

export interface ExchangeStatus {
    name: string;
    state: ExchangeConnectionState;
    active_pairs: number;
    last_heartbeat_ms: number;
    total_reconnects: number;
    ws_url: string;
}

export interface ExchangeStatusReport {
    exchanges: ExchangeStatus[];
}

export interface PipelineReliabilityMetrics {
    coverage: number;
    gap_count: number;
    outliers_rejected: number;
    outliers_bypassed: number;
    out_of_order_dropped: number;
    total_candles_processed: number;
    reconstructed_candles: number;
    source_mix?: { db_warm?: number; rest_gap?: number; live?: number };
}

export interface ExchangeAccount {
    id: number;
    exchange: string;
    label: string;
    currency: string;
    testnet: boolean;
    created_at: string;
    is_active: boolean;
    referred_uid: string;
    last_sync_timestamp: number | null;
    api_key: string;
    account_name: string;
}

export interface SystemHeartbeat {
    observation_loop_latency_ms: number;
    ingest_skew_ms: number;
    system_heartbeat_latency_ms: number;
    wal_mode: boolean;
    active_pairs: number;
}

export interface DecisionMemoryRow {
    timestamp: number;
    symbol: string;
    decision_score: number;
    direction: string;
    readiness: string;
}

export interface CompletedTradesRow {
    timestamp: number;
    symbol: string;
    direction: string;
    entry_price: number;
    exit_price: number;
    pnl: number;
    roi_pct: number;
}

export type TriggerModeUnion = 'interval' | 'candle_close' | 'event_driven';

export type TriggerModeConfig =
    | { mode: 'interval'; seconds: number }
    | { mode: 'candle_close'; timeframe: string; count: number }
    | { mode: 'event_driven'; events: string[] };

// ── Opportunity Matrix (L4) ──
export type LevelSource = 'FIBONACCI' | 'VOLUME_PROFILE' | 'PIVOT_POINTS' | 'SUPPORT_RESISTANCE' | 'LIQUIDITY_CLUSTER' | 'ATR_FALLBACK';

export interface ConfluentLevel {
    price: number;
    confluence_count: number;
    sources: LevelSource[];
    strength: number;
    /** v6.10.17 (F23): the trade direction this level serves — LONG below
     *  close, SHORT above close, null on the close. */
    side?: 'LONG' | 'SHORT' | null;
}

export interface PriceRange {
    low: number;
    high: number;
}

/**
 * Direction family the L4 producer tags each `OpportunityProfile` with.
 *   - `TrendRiding` resolves to LONG when `Analysis.bias` is bullish,
 *     SHORT when bearish, NEUTRAL otherwise. Applies to TrendContinuation,
 *     Breakout, Pullback, Scalp, LiquiditySqueeze.
 *   - `CounterTrend` reverses the macro bias. Applies to MeanReversion,
 *     Reversal.
 *   - `Neutral` carries no actionable setup. Applies to NoClearOpportunity.
 */
export type DirectionFamily = 'TREND_RIDING' | 'COUNTER_TREND' | 'NEUTRAL';

/**
 * Trade viability classification for an `OpportunityProfile`. Tells the
 * operator whether the profile carries an actionable bracket (zones
 * pass per-side invariants on a resolvable direction) or whether the
 * profile is informational only. Set by the L4 producer and surfaced
 * in the UI as a coloured badge next to each qualifying profile's
 * preconditions bar.
 *
 * Wire format is SCREAMING_SNAKE_CASE. Legacy payloads without this
 * field default to `NoClear` (most conservative).
 */
export type TradeViability = 'Actionable' | 'Qualifying' | 'DirectionalNeutral' | 'GeometryInverted' | 'NoClear';

/**
 * The RAW wire vocabulary of `TradeViability` (serde
 * `SCREAMING_SNAKE_CASE`). The wire field is typed with this union; every
 * consumer normalizes to `TradeViability` (PascalCase display form) via
 * `lib/viability.ts::normalizeViability` at the boundary.
 */
export type TradeViabilityWire = 'ACTIONABLE' | 'QUALIFYING' | 'DIRECTIONAL_NEUTRAL' | 'GEOMETRY_INVERTED' | 'NO_CLEAR';

/**
 * One per-setup-type entry in `OpportunityMatrix.profiles`. Each
 * qualifying profile (with `preconditions_met > 0`) carries its own
 * actionable entry/target/invalidation/R:R on the resolved direction.
 * The frontend's `selectProfileSide` + `profileZones` helpers consume
 * these fields to render one bracket per profile.
 */
export interface OpportunityProfile {
    opportunity_type: string;
    score: number;
    preconditions_met: number;
    preconditions_total: number;
    notes: string;
    /** Direction family this profile implies. `null` on legacy payloads. */
    direction_family: DirectionFamily | null;
    /** LONG-side zones. Populated only when the profile resolves to LONG. */
    long_entry_zone: PriceRange | null;
    long_target_zone: PriceRange | null;
    long_invalidation_level: number | null;
    /** SHORT-side zones. Populated only when the profile resolves to SHORT. */
    short_entry_zone: PriceRange | null;
    short_target_zone: PriceRange | null;
    short_invalidation_level: number | null;
    /** Per-side expected R:R derived from the per-profile zones. */
    long_expected_rr_internal: number | null;
    short_expected_rr_internal: number | null;
    /** Trade viability classification. `null` on legacy payloads. */
    trade_viability: TradeViabilityWire | null;
    /** Server-side geometry-consistency for the LONG side. */
    long_geometry_consistent?: boolean;
    /** Server-side geometry-consistency for the SHORT side. */
    short_geometry_consistent?: boolean;
    /**
     * v6.14: precondition-scaled operator-facing score —
     * `round(score × min(1, preconditions_met/preconditions_total))`,
     * emitted by the backend as the single source of truth for the
     * displayed setup score. Raw `score` stays intact for data-science.
     * `null`/absent on legacy payloads — fall back to the local
     * `displayScore` rule.
     */
    display_score?: number | null;
}

export interface OpportunityMatrix {
    symbol: string;
    /** Matrix-level direction family (always TrendRiding or Neutral on the
     *  wire; CounterTrend is expressed per-profile). */
    direction_family?: DirectionFamily | null;
    primary_opportunity: string;
    opportunity_score: number;
    setup_quality: string;
    profiles: OpportunityProfile[];
    forecast_confidence: number;
    contributing_signals: string[];
    invalidation_note: string;
    entry_zone: PriceRange;
    target_zone: PriceRange;
    invalidation_level: number;
    long_entry_zone: PriceRange;
    long_target_zone: PriceRange;
    long_invalidation_level: number;
    short_entry_zone: PriceRange;
    short_target_zone: PriceRange;
    short_invalidation_level: number;
    /** Per-side LONG R:R — v6.10.19 (P5): the NET value (gross minus
     *  estimated fees/slippage/funding). Active side resolved by `analysis.bias`. */
    long_expected_rr_internal: number;
    /** Per-side SHORT R:R — v6.10.19 (P5): the NET value. */
    short_expected_rr_internal: number;
    /** v6.10.19 (P5): the GROSS geometric R:R (pre-cost) per side. */
    long_gross_rr_internal?: number | null;
    short_gross_rr_internal?: number | null;
    time_horizon: string;
    confluent_entry_levels: ConfluentLevel[];
    confluent_target_levels: ConfluentLevel[];
    confluent_invalidation_levels: ConfluentLevel[];
    /** Server-side geometry-consistency for the LONG side at matrix level. */
    long_geometry_consistent?: boolean;
    /** Server-side geometry-consistency for the SHORT side at matrix level. */
    short_geometry_consistent?: boolean;
    /** v6.10.21 (NBR): direction-agnostic range reference bracket. Present
     *  only when the primary is NoClearOpportunity and the regime reads as
     *  a range — informational only, never actionable. Legacy payloads
     *  omit the field. */
    neutral_reference_bracket?: NeutralBracket | null;
}

/** v6.10.21 (NBR): range-fade reference frame emitted by the backend
 *  under NoClear + Range. Mirrors `core_domain::opportunity::NeutralBracket`. */
export interface NeutralBracket {
    entry_zone: PriceRange;
    target_zone: PriceRange;
    invalidation_level: number;
    expected_rr_internal: number;
    geometry_consistent: boolean;
    rationale: string;
}

// ── Snapshot Export (v6.10.4+) ──────────────────────────────────────
//
// Mirrors the Rust `SnapshotExportRuntime` (in
// `core-domain/src/snapshot_export.rs`) and the JSON wire shape
// returned by `GET /api/snapshot-export/status`. The 9 tabs
// listed in `ALL_SNAPSHOT_TABS` mirror the Rust `ALL_TABS` array.
export const ALL_SNAPSHOT_TABS = [
    'metrics',
    'mtf',
    'alignment',
    'opportunity',
    'risk',
    'analysis',
    'advisory',
    'decision',
    'recommendation',
] as const;

export type SnapshotExportTabId = typeof ALL_SNAPSHOT_TABS[number];

export interface SnapshotExportStatus {
    enabled: boolean;
    output_path: string;
    interval_secs: number;
    max_snapshots_retained: number;
    tabs: string[];
    /** ISO-8601 UTC timestamp of the most recent successful tick. */
    last_snapshot_at: string | null;
    total_snapshots_written: number;
    last_error: string | null;
    last_instance_count: number;
}

/** Patch shape accepted by `PUT /api/snapshot-export/config`. */
export interface SnapshotExportConfigPatch {
    enabled?: boolean;
    output_path?: string;
    interval_secs?: number;
    max_snapshots_retained?: number;
    tabs?: string[];
}

// ─── BTE run payloads (shared by the Backtesting shell + the store;
//     moved component-local → types in v11.2 Phase 3) ─────────────────

export interface BteResult {
    backtest_id: number;
    mode?: string;
    params: { symbol: string; timeframe_secs: number; from_secs: number; to_secs: number; portfolio_capital_usd: number };
    summary: {
        total_trades: number; win_count: number; loss_count: number; win_rate: number;
        gross_profit: number; gross_loss: number; profit_factor: number | null;
        expectancy: number; max_drawdown_pct: number;
    };
    stats: any;
    trades: { timestamp: number; direction: string; entry_price: number; exit_price: number; size: number; pnl: number; exit_reason: string }[];
    equity_curve: [number, number][];
}

/** `GET /api/backtest/:id/portfolio` */
export interface BtePortfolioPayload {
    run_id: number;
    portfolio: any[];
}

/** `GET /api/backtest/:id/signals` */
export interface BteSignalsPayload {
    run_id: number;
    count: number;
    signals: any[];
}
