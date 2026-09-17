pub mod duration_profile;
pub mod models;
pub use models::*;
pub mod strategy;
pub use strategy::*;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// v11.9: timeframe identity is the DURATION in seconds — there are no
// named slots. The supported pool and the derived labels live in
// `core-domain`; config-models re-exports them so every config consumer
// reads one source of truth.
pub use core_domain::{
    duration_label, duration_label_upper, is_supported_duration, slot_label_for,
    SUPPORTED_DURATIONS,
};

/// The DEFAULT ACTIVE ladder: the fastest 8 durations of the supported
/// pool (1s/3s/5s/15s/30s/1m/3m/5m). Operators override via
/// `[workspace].timeframes` (1..=14 pool members).
pub const DEFAULT_TIMEFRAMES: [u64; 8] = [1, 3, 5, 15, 30, 60, 180, 300];

fn default_timeframes() -> Vec<u64> {
    DEFAULT_TIMEFRAMES.to_vec()
}

/// Error type for all config-loader failures. Replaces the previous
/// pattern of `.expect("...")` calls, which mixed parser errors with IO
/// errors and made recovery impossible at boot time.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("TOML syntax error in `{path}`: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("TOML serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),

    #[error(
        "legacy config file `{path}` is no longer recognized.\n\
         The platform now reads `config.toml` only. Migrate your\n\
         settings to the new schema documented in\n\
         `docs/conceptual-foundations/01-07-data-model-hierarchy.md`."
    )]
    LegacyFile { path: PathBuf },

    #[error(
        "workspace table is missing from `config.toml`. The platform\n\
         requires a `[workspace]` section. See\n\
         `docs/conceptual-foundations/01-07-data-model-hierarchy.md`."
    )]
    WorkspaceMissing,

    #[error(
        "legacy timeframe key `{key}` in `config.toml` is no longer supported.\n\
         v11.9 replaced named slots with duration-keyed timeframes:\n\
         - replace `active_slots`/`active_timeframes` with\n\
           `[workspace].timeframes = [1, 3, 5, 15, 30, 60, 180, 300]` (seconds, fastest->slowest)\n\
         - replace per-instance `micro_term`/`fast_term`/`slow_term`/`macro_term`\n\
           with `[workspace.instances.timeframes.<secs>]` blocks\n\
         There is no legacy fallback — migrate the file and restart."
    )]
    LegacyTimeframeKey { key: String },

    #[error(
        "invalid numeric config (audit M8): {detail}.\n\
         Zero-valued periods/durations panic in the hot path (Decimal/u64\n\
         division, median-window indexing) — every period must be >= 1."
    )]
    InvalidNumeric { detail: String },
}

/// Alias for `Result<T, ConfigError>`.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// On-disk shape of `config.toml`. Serde-deserialized directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnDiskConfig {
    #[serde(default)]
    hyperliquid: HyperliquidConfig,
    #[serde(default)]
    bitget: BitgetConfig,
    #[serde(default)]
    clock_monitor: Option<ClockMonitorTomlConfig>,
    #[serde(default)]
    quality: Option<QualityConfig>,
    #[serde(default)]
    reconnect: ReconnectConfig,
    #[serde(default)]
    candle_buffer: CandleBufferConfig,
    /// Optional snapshot-export scheduler. When `None` the
    /// `SnapshotExportConfig::default()` (disabled) is used.
    #[serde(default)]
    snapshot_export: Option<SnapshotExportConfig>,
    /// HTTP server bind address + port (per-folder sessions). Defaults to
    /// loopback `127.0.0.1:3000` when the `[platform.server]` section is
    /// absent.
    #[serde(default)]
    server: Option<ServerConfig>,
    workspace: WorkspaceConfig,
}

impl OnDiskConfig {
    /// Decompose into `(PlatformConfig, WorkspaceConfig)`.
    fn split(self) -> (PlatformConfig, WorkspaceConfig) {
        (
            PlatformConfig {
                hyperliquid: self.hyperliquid,
                bitget: self.bitget,
                clock_monitor: self.clock_monitor,
                quality: self.quality,
                reconnect: self.reconnect,
                candle_buffer: self.candle_buffer,
                snapshot_export: self.snapshot_export.unwrap_or_default(),
                server: self.server.unwrap_or_default(),
            },
            self.workspace,
        )
    }
}

/// The HTTP server the daemon serves the dashboard + WS from. One process
/// per folder ⇒ each session needs its own port. Precedence:
/// `--port/--bind` CLI flag → `PLATFORM_PORT`/`PLATFORM_BIND` env →
/// `[platform.server]` in `config.toml` → defaults `127.0.0.1:3000`.
/// Loopback-only by default (K1 security boundary).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: String,
    pub port: u16,
    /// v11.3: when the resolved port is already in use, step +1 (up to
    /// `port_fallback_range` attempts) and bind the first free port instead
    /// of exiting. Lets two folder-per-session deployments coexist without
    /// hand-assigning ports.
    pub auto_fallback: bool,
    /// v11.3: how many consecutive ports to probe after the resolved one.
    pub port_fallback_range: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".to_string(),
            port: 3000,
            auto_fallback: true,
            port_fallback_range: 20,
        }
    }
}

/// Platform-level configuration. Read once at startup by `execution-daemon`.
/// Contains the things that are NOT per-workspace / per-instance: the
/// exchange endpoints the binary connects to and the NTP clock monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlatformConfig {
    #[serde(default)]
    pub hyperliquid: HyperliquidConfig,
    #[serde(default)]
    pub bitget: BitgetConfig,
    /// Optional clock-drift monitor. When `Some` and `is_active()`, main.rs
    /// spawns the NTP-based monitor alongside the other background tasks.
    #[serde(default)]
    pub clock_monitor: Option<ClockMonitorTomlConfig>,

    /// Optional data-quality configuration (median filter, outlier tolerance).
    /// When `None`, the median filter is disabled and all ticks are accepted.
    #[serde(default)]
    pub quality: Option<QualityConfig>,
    #[serde(default)]
    pub reconnect: ReconnectConfig,
    /// Single source of truth for candle buffer behavior. Replaces the
    /// previous per-instance `analysis_limit` field. See
    /// `docs/operations-and-compliance/08-08-candle-buffer-spec.md` (CB-01).
    #[serde(default)]
    pub candle_buffer: CandleBufferConfig,
    /// Periodic per-tab JSON dump configuration. See
    /// `SnapshotExportConfig` and `docs/operations-and-compliance/08-09-snapshot-export.md`.
    /// Default `SnapshotExportConfig::default()` (disabled) is used when
    /// the `[snapshot_export]` section is absent from `config.toml`.
    #[serde(default)]
    pub snapshot_export: SnapshotExportConfig,
    /// HTTP server bind address + port — see `ServerConfig`. The platform
    /// stays loopback-bound by default; a per-folder `port` lets several
    /// sessions run side by side on one machine.
    #[serde(default)]
    pub server: ServerConfig,
}

/// Status of a single trading-pair instance. Persisted in the workspace file
/// so the dashboard can render the row correctly after a restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum InstanceStatus {
    #[default]
    Running,
    Paused,
    Stopped,
}

/// One workspace = one portfolio + analytics + strategies + market-monitor
/// settings + an array of trading-pair instances.
///
/// "All engines running my program": the workspace is the unit of ownership
/// for the user's portfolio. Exactly one workspace per binary.
/// v10: the data-science export layer (`./ds/`). NDJSON mirrors of every
/// artifact the GUI renders — one producer, three sinks (DB, WS/GUI, files).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DataScienceConfig {
    pub enabled: bool,
    /// Root folder for the DS export tree.
    pub output_path: String,
    pub capture_market: bool,
    pub capture_trading: bool,
    pub capture_analytics: bool,
    pub flush_interval_secs: u64,
}

impl Default for DataScienceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            output_path: "./ds".to_string(),
            capture_market: true,
            capture_trading: true,
            capture_analytics: true,
            flush_interval_secs: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Workspace identifier (slug, filesystem-safe). Currently always
    /// `"main"` — the binary supports one workspace per deployment.
    pub id: String,
    /// Display name shown in the dashboard header.
    pub name: String,
    /// Default settlement currency for new instances.
    pub default_currency: String,
    /// Default exchange for new instances.
    pub default_exchange: String,
    /// v9 (F-07): THE single capital dial — the shared equity ledger
    /// seeds from this value (paper). Live reads the exchange balance;
    /// backtests use the same field as their simulated account seed.
    #[serde(default = "default_portfolio_capital")]
    pub portfolio_capital_usd: f64,

    // ─── Market-monitor defaults (per-instance inheritance) ────────
    #[serde(default)]
    pub candles: CandlesConfig,
    /// v11.9: the ACTIVE timeframe set — an explicit subset of the
    /// supported duration pool (1..=14 members, unique, ascending order
    /// enforced by validation; `active_timeframes_for()` re-orders
    /// canonically). Every instance runs exactly these durations.
    /// Editable live via `POST /api/config` (recharges running instances).
    #[serde(default = "default_timeframes")]
    pub timeframes: Vec<u64>,
    #[serde(default)]
    pub indicators: IndicatorsConfig,
    #[serde(default)]
    pub fast_timeframe: FastTimeframeConfig,
    #[serde(default)]
    pub slow_timeframe: SlowTimeframeConfig,
    #[serde(default)]
    pub macro_timeframe: SlowTimeframeConfig,
    #[serde(default)]
    pub fibonacci: FibonacciConfig,
    #[serde(default)]
    pub pivots: PivotsConfig,

    // ─── Portfolio / strategy / analytics ──────────────────────────
    #[serde(default)]
    pub safety: SafetyConfig,
    #[serde(default)]
    pub fees: FeesConfig,
    #[serde(default)]
    pub intervals: IntervalsConfig,
    #[serde(default)]
    pub liquidity: LiquidityConfig,
    #[serde(default)]
    pub heatmap: HeatmapConfig,
    #[serde(default)]
    pub api_failover: ApiFailoverConfig,
    #[serde(default)]
    pub activation: ActivationConfig,
    /// Opportunity-matrix knobs — currently just the ATR-fallback toggle
    /// for confluent levels (Phase C of the v6.10 fix). When `enabled`,
    /// the synthesis emits at least one entry / target level derived from
    /// `close ± k·ATR` if every structural source (Fibonacci / Volume
    /// Profile / Pivot Points / Liquidation Clusters) is empty, so the
    /// Opportunities panel never shows "No confluent levels" for a
    /// healthy market. When `disabled` (strict behaviour), the empty
    /// state is the honest signal of "no structural levels near price".
    #[serde(default)]
    pub opportunity_matrix: OpportunityMatrixConfig,
    /// Order-book depth analysis knobs (v9 F-04: the `[order_book]` TOML
    /// surface — previously the runtime hardcoded
    /// `OrderBookConfig::default()` in the pipeline constructor).
    #[serde(default)]
    pub order_book: OrderBookConfig,
    /// Schema version counter — incremented on every successful POST /api/config.
    #[serde(default)]
    pub config_version: u64,
    #[serde(default)]
    pub leverage: LeverageConfig,
    #[serde(default)]
    pub defaults: DefaultsConfig,

    /// Zero or more trading-pair instances.
    #[serde(default)]
    pub instances: Vec<InstanceEntry>,

    /// v7 Trade Automation Engine — minimal setup-executor configuration.
    #[serde(default)]
    pub minimal_tae: MinimalTaeConfig,

    /// v7.3 PAE significance-treatment configuration (α, Monte Carlo runs,
    /// min-trades for the edge verdict).
    #[serde(default)]
    pub analytics: AnalyticsConfig,

    /// v7.3 portfolio risk limits — concentration / exposure / correlation
    /// caps the PME Exposure layer enforces and the dashboard renders.
    #[serde(default)]
    pub risk_limits: RiskLimitsConfig,

    /// Execution-layer configuration (slippage ceiling, etc.).
    #[serde(default)]
    pub execution: ExecutionConfig,

    /// Backtesting Engine (BTE) — candle archive depth, warmup bars,
    /// per-exchange paging limits for the deep-history backtest.
    #[serde(default)]
    pub backtest: BacktestConfig,
    /// v9: the strategy registry — one JSON per model. The built-in
    /// `default` strategy reproduces v8.2 behavior exactly; instances
    /// always launch bound to `default` and can be rebound later
    /// (full recharge at the next candle boundary).
    #[serde(default)]
    pub strategies: Vec<StrategyConfig>,
    /// v10: the data-science export layer (`./ds/`).
    #[serde(default)]
    pub data_science: DataScienceConfig,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            id: "main".to_string(),
            name: "Default Workspace".to_string(),
            default_currency: "USDC".to_string(),
            default_exchange: "Hyperliquid".to_string(),
            portfolio_capital_usd: default_portfolio_capital(),
            candles: CandlesConfig::default(),
            timeframes: default_timeframes(),
            indicators: IndicatorsConfig::default(),
            fast_timeframe: FastTimeframeConfig::default(),
            slow_timeframe: SlowTimeframeConfig::default(),
            macro_timeframe: SlowTimeframeConfig::default(),
            fibonacci: FibonacciConfig::default(),
            pivots: PivotsConfig::default(),
            safety: SafetyConfig::default(),
            fees: FeesConfig::default(),
            intervals: IntervalsConfig::default(),
            liquidity: LiquidityConfig::default(),
            heatmap: HeatmapConfig::default(),
            api_failover: ApiFailoverConfig::default(),
            activation: ActivationConfig::default(),
            opportunity_matrix: OpportunityMatrixConfig::default(),
            order_book: OrderBookConfig::default(),
            config_version: 1,
            leverage: LeverageConfig::default(),
            defaults: DefaultsConfig::default(),
            instances: Vec::new(),
            minimal_tae: MinimalTaeConfig::default(),
            analytics: AnalyticsConfig::default(),
            risk_limits: RiskLimitsConfig::default(),
            execution: ExecutionConfig::default(),
            backtest: BacktestConfig::default(),
            strategies: vec![StrategyConfig::default()],
            data_science: DataScienceConfig::default(),
        }
    }
}

impl WorkspaceConfig {
    /// Convenience: the set of symbols that the workspace declares it should
    /// be running (extracted from the `instances[].symbol` list). Useful for
    /// the dashboard's "what pairs are configured" panel.
    pub fn declared_symbols(&self) -> Vec<String> {
        self.instances.iter().map(|i| i.symbol.clone()).collect()
    }

    /// v11.9: the FULL supported duration pool (14 members, fastest →
    /// slowest: 1s … 1d). The Launch Setup wizard, the CLI launch prompt,
    /// and the API ladder builders all derive from this pool, so every
    /// surface agrees. (Canonical source: `core_domain::SUPPORTED_DURATIONS`.)
    pub fn tf_ladder_defaults(&self) -> Vec<u64> {
        SUPPORTED_DURATIONS.to_vec()
    }

    /// v11.9: the ACTIVE ladder — the configured `[workspace].timeframes`
    /// subset, defensively healed (pool members only, deduplicated,
    /// re-ordered ascending fastest → slowest). Validation rejects typos
    /// at load; this accessor never returns an out-of-pool duration.
    /// This is what every instance actually runs.
    pub fn active_timeframes_for(&self) -> Vec<u64> {
        let mut out: Vec<u64> = Vec::with_capacity(self.timeframes.len());
        for &secs in &self.timeframes {
            if is_supported_duration(secs) && !out.contains(&secs) {
                out.push(secs);
            }
        }
        out.sort_unstable();
        out
    }

    /// v9: resolve a strategy by name, walking the `base` chain (patch
    /// inheritance). Returns an error on unknown names or inheritance
    /// cycles. Missing `default` entry falls back to the built-in
    /// `StrategyConfig::default()`.
    pub fn resolve_strategy(&self, name: &str) -> std::result::Result<StrategyConfig, String> {
        let mut seen = std::collections::HashSet::new();
        let mut current = self
            .strategies
            .iter()
            .find(|s| s.name == name)
            .cloned()
            .ok_or_else(|| format!("strategy '{name}' not found"))?;
        loop {
            let Some(base_name) = current.base.clone() else {
                return Ok(current);
            };
            if !seen.insert(current.name.clone()) {
                return Err(format!("strategy inheritance cycle at '{}'", current.name));
            }
            if base_name == current.name {
                return Err(format!(
                    "strategy '{}' cannot inherit from itself",
                    current.name
                ));
            }
            let base =
                if base_name == "default" && !self.strategies.iter().any(|s| s.name == "default") {
                    StrategyConfig::default()
                } else {
                    self.strategies
                        .iter()
                        .find(|s| s.name == base_name)
                        .cloned()
                        .ok_or_else(|| {
                            format!(
                                "base strategy '{base_name}' (of '{}') not found",
                                current.name
                            )
                        })?
                };
            let child_json = serde_json::to_value(&current)
                .map_err(|e| format!("serialize '{}': {e}", current.name))?;
            let base_json =
                serde_json::to_value(&base).map_err(|e| format!("serialize '{base_name}': {e}"))?;
            let mut merged = StrategyConfig::resolve(Some(&base_json), &child_json)?;
            // Walk the base chain: the merged leaf inherits the base's own base (if any),
            // not its own original base — otherwise the next loop iteration would
            // re-merge the same base and incorrectly report a cycle.
            merged.base = base.base.clone();
            current = merged;
        }
        #[allow(unreachable_code)]
        Ok(current)
    }

    /// The effective `default` strategy (what every instance binds at
    /// launch and what unattributed consumers use).
    pub fn default_strategy(&self) -> std::result::Result<StrategyConfig, String> {
        self.resolve_strategy("default")
    }

    /// Ensure the built-in `default` strategy entry exists (a workspace
    /// loaded from a pre-v9 config.toml has an empty `strategies` vec —
    /// the built-in default must still be present and editable).
    pub fn ensure_default_strategy(&mut self) {
        if !self.strategies.iter().any(|s| s.name == "default") {
            self.strategies.push(StrategyConfig::default());
        }
    }
}

/// Serde helper: `BTreeMap<u64, V>` over TOML/JSON object keys. TOML table
/// keys are always strings, so each key is parsed to `u64` on load and
/// stringified on save. (v11.9 duration-keyed instance overrides.)
mod u64_key_map {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S, V>(map: &BTreeMap<u64, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: Serialize,
    {
        let string_map: BTreeMap<String, &V> =
            map.iter().map(|(k, v)| (k.to_string(), v)).collect();
        string_map.serialize(serializer)
    }

    pub fn deserialize<'de, D, V>(deserializer: D) -> Result<BTreeMap<u64, V>, D::Error>
    where
        D: Deserializer<'de>,
        V: Deserialize<'de>,
    {
        let string_map = BTreeMap::<String, V>::deserialize(deserializer)?;
        let mut out = BTreeMap::new();
        for (key, value) in string_map {
            let secs = key
                .parse::<u64>()
                .map_err(|_| serde::de::Error::custom(format!("invalid duration key `{key}`")))?;
            out.insert(secs, value);
        }
        Ok(out)
    }
}

/// One trading-pair instance inside a workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceEntry {
    /// Stable identifier used as the runtime key (e.g. `"btc"` or `"BTC-USDT"`).
    pub id: String,
    /// Exchange-native symbol (e.g. `"BTC-USDT"`, `"ETH-USDC"`).
    pub symbol: String,
    /// Quote currency for this instance. Usually matches the workspace's
    /// `default_currency`.
    #[serde(default)]
    pub quote: String,
    /// Initial capital allocation for this instance (USD).
    // v9 (F-07): the per-instance `initial_capital_usd` is ERASED —
    // capital is ONE portfolio-wide dial (`[workspace]
    // portfolio_capital_usd`); the shared ledger seeds from it.
    /// Runtime status: running / paused / stopped. Defaults to running; the
    /// dashboard flips the bit when the user pauses or stops the instance.
    #[serde(default)]
    pub status: InstanceStatus,
    /// v11.9: per-duration indicator overrides keyed by duration seconds.
    /// Each entry is the COMPLETE `TimeframeConfig` for that duration: at
    /// spawn/recharge it replaces the `duration_profile` row + workspace
    /// defaults for that duration. Absent durations use the profile.
    /// `candles.duration_seconds` must equal the map key; keys must be
    /// supported pool members.
    #[serde(
        with = "u64_key_map",
        default,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub timeframes: std::collections::BTreeMap<u64, TimeframeConfig>,
    #[serde(default)]
    pub automation: AutomationConfig,
    #[serde(default)]
    pub operational_mode: OperationalMode,
    /// v7 execution mode (Observe / Paper / Live). Default Paper.
    #[serde(default)]
    pub mode: ExecutionMode,
    /// v9: the bound strategy (by name). `None` = the workspace default
    /// strategy. Instances always launch on the default; binding a
    /// strategy later recharges the instance fully at the next candle
    /// boundary (params-at-entry freeze for open positions).
    #[serde(default)]
    pub strategy: Option<String>,
    /// v8.2 per-instance allocation override (percent of portfolio equity,
    /// 1..=100). `None` = the global `[workspace.minimal_tae].allocation_pct`.
    #[serde(default)]
    pub allocation_pct: Option<f64>,
    #[serde(default)]
    pub weight_overrides: Option<std::collections::HashMap<String, i32>>,
    /// Per-instance activation overrides (union with global [activation]).
    #[serde(default)]
    pub activation: Option<ActivationConfig>,
}

fn default_portfolio_capital() -> f64 {
    1_000.0
}

/// Execution mode for the unified execution engine. The mode only affects
/// the final broker dispatch: `Observe` never submits orders (advisory /
/// market-monitoring only), `Paper` simulates fills internally, `Live`
/// routes to an exchange. All accounting (fees, slippage, funding, PnL) is
/// identical in `Paper` and `Live`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ExecutionMode {
    /// Market/signal monitoring only — no orders are ever dispatched.
    Observe,
    #[default]
    Paper,
    Live,
}

/// v7 TAE — the minimal setup-executor configuration. Replaces the erased
/// policy engine: the executor consumes the MME's top setup directly and
/// manages the trade to completion. See docs/engines/trade-automation-engine/.
///
/// v8.2: sizing is portfolio-share allocation — `allocation_pct` (1–100 %)
/// replaces the erased stop-distance risk sizing. Position size =
/// `equity × allocation_pct/100 ÷ entry_mid`; the sum of all instance
/// allocations must be ≤ 100 %.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinimalTaeConfig {
    /// Master switch for the setup executor.
    #[serde(default)]
    pub enabled: bool,
    /// v8.2: percent of portfolio equity allocated to each position
    /// (10.0 = 10 %). Range 1..=100; per-instance override on
    /// `InstanceEntry.allocation_pct`; Σ of all allocations ≤ 100 %.
    #[serde(default = "default_allocation_pct")]
    pub allocation_pct: f64,
    /// Fee-adjusted minimum reward-to-risk ratio for accepting a setup.
    #[serde(default = "default_min_net_rr")]
    pub min_net_rr: f64,
    /// Optional per-position notional cap as a percentage of equity
    /// (v9 F-08: replaces the absolute USD cap — the strategy must be
    /// capital-size invariant). None = no cap.
    #[serde(default)]
    #[serde(alias = "max_position_size_usd")]
    pub max_position_size_pct_of_equity: Option<f64>,
    /// Global concurrent-position cap across all symbols (1..=100).
    #[serde(default = "default_max_open_positions")]
    pub max_open_positions: u32,
    /// Entry placement mode. v7 supports only "zone_midpoint".
    #[serde(default = "default_entry_mode")]
    pub entry_mode: String,
    /// Invalidation semantics for open positions. v7 default: strict
    /// opposite-direction flip only ("direction_flip").
    #[serde(default = "default_invalidate_on")]
    pub invalidate_on: String,
}

fn default_allocation_pct() -> f64 {
    10.0
}
fn default_min_net_rr() -> f64 {
    1.0
}
fn default_max_open_positions() -> u32 {
    10
}
fn default_entry_mode() -> String {
    "zone_midpoint".to_string()
}
fn default_invalidate_on() -> String {
    "direction_flip".to_string()
}

impl Default for MinimalTaeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allocation_pct: default_allocation_pct(),
            min_net_rr: default_min_net_rr(),
            max_position_size_pct_of_equity: None,
            max_open_positions: default_max_open_positions(),
            entry_mode: default_entry_mode(),
            invalidate_on: default_invalidate_on(),
        }
    }
}

// ===========================================================================
// Loaders
// ===========================================================================

/// Canonical config path. Allows tests and the `manage.sh` wrapper to
/// override the location via `MARKET_MONITOR_CONFIG` for staging.
fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("MARKET_MONITOR_CONFIG") {
        return PathBuf::from(p);
    }
    PathBuf::from("config.toml")
}

/// Check that none of the legacy config files are present. If any are,
/// return `LegacyFile` so the caller can panic with a migration pointer.
fn assert_no_legacy_files() -> Result<()> {
    for legacy in ["instances.json", "workspaces.json", "pairs.json"] {
        if Path::new(legacy).exists() {
            return Err(ConfigError::LegacyFile {
                path: PathBuf::from(legacy),
            });
        }
    }
    Ok(())
}

/// Read + parse the config file, with v11.6 crash recovery: on a read or
/// parse failure, try `config.toml.bak` (last-good), then
/// `config.default.toml` (factory template — the previous corrupt copy is
/// quarantined as `config.toml.corrupt-<ts>`). Returns the raw text plus
/// a description of which source was used, so callers can log loudly.
fn read_config_raw() -> Result<(String, &'static str)> {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(raw) => match toml::from_str::<OnDiskConfig>(&raw) {
            Ok(_) => Ok((raw, "config.toml")),
            Err(parse_err) => {
                eprintln!(
                    "[config] ⚠️  config.toml failed to parse ({parse_err}) — attempting crash recovery"
                );
                recover_corrupt_config(&path, &raw)?;
                let (raw2, src2) = read_recovered()?;
                Ok((raw2, src2))
            }
        },
        Err(e) => {
            // Missing file → the template fallback path also applies (a
            // crash can leave the file absent after a failed rename cycle).
            eprintln!(
                "[config] ⚠️  could not read {} ({e}) — attempting crash recovery",
                path.display()
            );
            recover_corrupt_config(&path, "")?;
            let (raw2, src2) = read_recovered()?;
            Ok((raw2, src2))
        }
    }
}

/// Copy the corrupt file aside and restore from `.bak`, then the factory
/// template. Fatal (ConfigError) only if BOTH recovery sources are absent.
fn recover_corrupt_config(path: &Path, corrupt_raw: &str) -> Result<()> {
    if !corrupt_raw.is_empty() {
        let quarantine = path.with_extension(format!(
            "toml.corrupt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        ));
        if std::fs::write(&quarantine, corrupt_raw).is_ok() {
            eprintln!(
                "[config] corrupt copy quarantined as {}",
                quarantine.display()
            );
        }
    }
    let bak = path.with_extension("toml.bak");
    if bak.exists() {
        eprintln!("[config] 🔧 restoring last-good backup {}", bak.display());
        std::fs::copy(&bak, path).map_err(|e| ConfigError::Io {
            path: bak.clone(),
            source: e,
        })?;
        return Ok(());
    }
    // Template lookup: next to the config file first (per-folder
    // deployments), then the process CWD (the common repo-root case).
    let mut template = path
        .parent()
        .map(|d| d.join("config.default.toml"))
        .filter(|t| t.exists());
    if template.is_none() {
        let cwd = PathBuf::from("config.default.toml");
        if cwd.exists() {
            template = Some(cwd);
        }
    }
    if let Some(template) = template {
        eprintln!(
            "[config] 🔧 no backup found — restoring factory template {}",
            template.display()
        );
        std::fs::copy(&template, path).map_err(|e| ConfigError::Io {
            path: template.clone(),
            source: e,
        })?;
        return Ok(());
    }
    Err(ConfigError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "config.toml is corrupt and neither config.toml.bak nor config.default.toml exists",
        ),
    })
}

fn read_recovered() -> Result<(String, &'static str)> {
    let path = config_path();
    let raw = std::fs::read_to_string(&path).map_err(|e| ConfigError::Io {
        path: path.clone(),
        source: e,
    })?;
    // Final parse check — if even the recovered source fails to parse we
    // surface the error (boot fails honestly rather than silently lying).
    if let Err(e) = toml::from_str::<OnDiskConfig>(&raw) {
        return Err(ConfigError::Parse {
            path: path.clone(),
            source: e,
        });
    }
    Ok((raw, "recovered"))
}

/// Load the platform config from `config.toml` (crash-resilient, v11.6).
pub fn load_platform() -> Result<PlatformConfig> {
    assert_no_legacy_files()?;
    let (raw, _src) = read_config_raw()?;
    let on_disk: OnDiskConfig =
        toml::from_str(&raw).map_err(|e: toml::de::Error| ConfigError::Parse {
            path: config_path(),
            source: e,
        })?;
    let (platform, _workspace) = on_disk.split();
    validate_platform(&platform)?;
    Ok(platform)
}

/// Load the workspace config from `config.toml` (crash-resilient, v11.6).
pub fn load_workspace() -> Result<WorkspaceConfig> {
    assert_no_legacy_files()?;
    let (raw, _src) = read_config_raw()?;
    reject_legacy_timeframe_keys(&raw)?;
    let on_disk: OnDiskConfig =
        toml::from_str(&raw).map_err(|e: toml::de::Error| ConfigError::Parse {
            path: config_path(),
            source: e,
        })?;
    validate_workspace(&on_disk.workspace)?;
    let mut ws = on_disk.workspace;
    ws.ensure_default_strategy();
    Ok(ws)
}

/// Load both at once (the common case).
/// Load both at once (crash-resilient, v11.6).
pub fn load() -> Result<(PlatformConfig, WorkspaceConfig)> {
    assert_no_legacy_files()?;
    let (raw, _src) = read_config_raw()?;
    reject_legacy_timeframe_keys(&raw)?;
    let on_disk: OnDiskConfig =
        toml::from_str(&raw).map_err(|e: toml::de::Error| ConfigError::Parse {
            path: config_path(),
            source: e,
        })?;
    validate_workspace(&on_disk.workspace)?;
    Ok(on_disk.split())
}

/// Fail-fast boot validation (audit fix M6): surfaces config surfaces the
/// runtime cannot honor instead of silently ignoring them.
///
/// v11.9: legacy named-slot keys are rejected at load by
/// `reject_legacy_timeframe_keys` (there is no legacy fallback), and every
/// instance `timeframes.<secs>` override is validated for pool membership,
/// key/duration agreement, and indicator numeric guards.
/// v11.9 (D2): hard-reject legacy named-slot keys at load. Parses the raw
/// TOML value first so keys the new schema no longer declares cannot be
/// silently ignored. There is no fallback — the operator migrates to
/// `[workspace].timeframes = [..]` and `[workspace.instances.timeframes.<secs>]`.
fn reject_legacy_timeframe_keys(raw: &str) -> Result<()> {
    let value: toml::Value = match toml::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Ok(()), // the real parse error surfaces downstream
    };
    let Some(workspace) = value.get("workspace") else {
        return Ok(());
    };
    for key in ["active_slots", "active_timeframes"] {
        if workspace.get(key).is_some() {
            return Err(ConfigError::LegacyTimeframeKey {
                key: format!("[workspace].{key}"),
            });
        }
    }
    if let Some(instances) = workspace.get("instances").and_then(|v| v.as_array()) {
        for inst in instances {
            let symbol = inst
                .get("symbol")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            for key in [
                "micro_term",
                "fast_term",
                "slow_term",
                "macro_term",
                "custom_pipelines",
            ] {
                if inst.get(key).is_some() {
                    return Err(ConfigError::LegacyTimeframeKey {
                        key: format!("instance {symbol}: {key}"),
                    });
                }
            }
        }
    }
    Ok(())
}

pub fn validate_workspace(ws: &WorkspaceConfig) -> Result<()> {
    // v7.3 (M8-style numeric guards): the significance treatment and the
    // risk limits are real numerics that flow into division/ranking logic —
    // reject nonsense at boot instead of silently mis-verdicting trades.
    if !(ws.analytics.alpha.is_finite() && ws.analytics.alpha > 0.0 && ws.analytics.alpha <= 1.0) {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.analytics].alpha = {} (must be in (0, 1])",
                ws.analytics.alpha
            ),
        });
    }
    if ws.analytics.monte_carlo_runs < 1000 {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.analytics].monte_carlo_runs = {} (must be >= 1000)",
                ws.analytics.monte_carlo_runs
            ),
        });
    }
    if ws.analytics.min_trades_for_verdict < 10 {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.analytics].min_trades_for_verdict = {} (must be >= 10)",
                ws.analytics.min_trades_for_verdict
            ),
        });
    }
    // v8.2 allocation model: percent-of-equity sizing with a hard capital
    // conservation bound — allocations are 1..=100 %, at most 100 instances,
    // and the sum of all instance allocations must not exceed 100 %.
    let alloc = ws.minimal_tae.allocation_pct;
    if !alloc.is_finite() || !(1.0..=100.0).contains(&alloc) {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.minimal_tae].allocation_pct = {alloc} (must be in 1..=100)"
            ),
        });
    }
    if !(1..=100).contains(&ws.minimal_tae.max_open_positions) {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.minimal_tae].max_open_positions = {} (must be in 1..=100)",
                ws.minimal_tae.max_open_positions
            ),
        });
    }
    if ws.instances.len() > 100 {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "workspace declares {} instances (maximum is 100)",
                ws.instances.len()
            ),
        });
    }
    let allocation_sum: f64 = ws
        .instances
        .iter()
        .map(|i| i.allocation_pct.unwrap_or(alloc))
        .sum();
    if allocation_sum > 100.0 + 1e-9 {
        return Err(ConfigError::InvalidNumeric {
            detail: format!("Σ instance allocations = {allocation_sum:.2}% (must be <= 100%)"),
        });
    }
    if ws.backtest.hyperliquid.max_candles_per_tf > 100_000 {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.backtest].hyperliquid.max_candles_per_tf = {} (must be <= 100000)",
                ws.backtest.hyperliquid.max_candles_per_tf
            ),
        });
    }
    // BTE (v8): archive depth 1..=365, warmup floor, and per-exchange
    // paging sanity. The depth is the "how far back can I look" contract —
    // reject out-of-range values instead of silently clamping.
    if !(1..=365).contains(&ws.backtest.archive_depth_days) {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.backtest].archive_depth_days = {} (must be in 1..=365)",
                ws.backtest.archive_depth_days
            ),
        });
    }
    if ws.backtest.warmup_bars < 30 {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.backtest].warmup_bars = {} (must be >= 30)",
                ws.backtest.warmup_bars
            ),
        });
    }
    if ws.backtest.max_equity_points < 10 {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.backtest].max_equity_points = {} (must be >= 10)",
                ws.backtest.max_equity_points
            ),
        });
    }
    for (exchange, limits) in [
        ("hyperliquid", &ws.backtest.hyperliquid),
        ("bitget", &ws.backtest.bitget),
    ] {
        if limits.page_cap == 0 {
            return Err(ConfigError::InvalidNumeric {
                detail: format!(
                    "[workspace.backtest].{exchange}.page_cap = {} (must be > 0)",
                    limits.page_cap
                ),
            });
        }
        if limits.max_pages_per_run == 0 {
            return Err(ConfigError::InvalidNumeric {
                detail: format!(
                    "[workspace.backtest].{exchange}.max_pages_per_run = {} (must be >= 1)",
                    limits.max_pages_per_run
                ),
            });
        }
    }
    for (name, v) in [
        (
            "max_single_pair_exposure_pct",
            ws.risk_limits.max_single_pair_exposure_pct,
        ),
        (
            "max_portfolio_exposure_pct",
            ws.risk_limits.max_portfolio_exposure_pct,
        ),
    ] {
        if !v.is_finite() || !(0.0 < v) || !(v <= 100.0) {
            return Err(ConfigError::InvalidNumeric {
                detail: format!("[workspace.risk_limits].{name} = {v} (must be in (0, 100])"),
            });
        }
    }
    if !(0.0 < ws.risk_limits.max_correlation) || ws.risk_limits.max_correlation > 1.0 {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[workspace.risk_limits].max_correlation = {} (must be in (0, 1])",
                ws.risk_limits.max_correlation
            ),
        });
    }
    // v11.9: the ACTIVE timeframe set — 1..=14 unique members of the
    // supported duration pool. Validation rejects typos and duplicates
    // loudly instead of silently healing.
    {
        let set = &ws.timeframes;
        let mut seen = std::collections::HashSet::new();
        for &secs in set {
            if !is_supported_duration(secs) {
                return Err(ConfigError::InvalidNumeric {
                    detail: format!(
                        "[workspace].timeframes: {}s is not a supported duration (valid: {})",
                        secs,
                        SUPPORTED_DURATIONS
                            .iter()
                            .map(|d| duration_label(*d))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
            if !seen.insert(secs) {
                return Err(ConfigError::InvalidNumeric {
                    detail: format!("[workspace].timeframes: duplicate duration {}s", secs),
                });
            }
        }
        if set.is_empty() || set.len() > SUPPORTED_DURATIONS.len() {
            return Err(ConfigError::InvalidNumeric {
                detail: format!(
                    "[workspace].timeframes: {} entries (must be 1..={})",
                    set.len(),
                    SUPPORTED_DURATIONS.len()
                ),
            });
        }
    }
    for inst in &ws.instances {
        for (&secs, tf) in &inst.timeframes {
            if !is_supported_duration(secs) {
                return Err(ConfigError::InvalidNumeric {
                    detail: format!(
                        "instance {}: timeframes.{}s is not a supported duration",
                        inst.symbol, secs
                    ),
                });
            }
            if tf.candles.duration_seconds != secs {
                return Err(ConfigError::InvalidNumeric {
                    detail: format!(
                        "instance {}: timeframes.{}s declares candles.duration_seconds = {} (must equal the key)",
                        inst.symbol, secs, tf.candles.duration_seconds
                    ),
                });
            }
            validate_indicators_config(
                &tf.indicators,
                &format!("workspace.instances.{}.timeframes.{}", inst.symbol, secs),
            )?;
        }
    }
    // v11.9 (D4): ladder-role timeframes must be duration labels of the
    // supported pool (only checked when role separation is enabled).
    for strat in &ws.strategies {
        let roles = &strat.ladder_roles;
        if roles.enabled {
            for (field, value) in [
                ("decision_tf", &roles.decision_tf),
                ("entry_tf", &roles.entry_tf),
                ("stop_tf", &roles.stop_tf),
                ("target_tf", &roles.target_tf),
            ] {
                let ok = SUPPORTED_DURATIONS
                    .iter()
                    .any(|d| duration_label(*d).eq_ignore_ascii_case(value));
                if !ok {
                    return Err(ConfigError::InvalidNumeric {
                        detail: format!(
                            "strategy {}: ladder_roles.{field} = `{value}` (must be a duration label like `1s`/`1m`/`1h`/`1d`)",
                            strat.name
                        ),
                    });
                }
            }
        }
    }
    // Workspace-level indicator defaults — same guards as per-instance.
    validate_indicators_config(&ws.indicators, "workspace.indicators.")?;
    // Liquidity / heatmap / candle-buffer numeric guards
    {
        let liq = &ws.liquidity;
        if !(liq.maintenance_margin_rate.is_finite()
            && liq.maintenance_margin_rate > 0.0
            && liq.maintenance_margin_rate <= 1.0)
        {
            return Err(ConfigError::InvalidNumeric {
                detail: format!(
                    "[workspace.liquidity].maintenance_margin_rate = {} (must be in (0,1])",
                    liq.maintenance_margin_rate
                ),
            });
        }
        if !(liq.bucket_retention_days > 0) {
            return Err(ConfigError::InvalidNumeric {
                detail: format!(
                    "[workspace.liquidity].bucket_retention_days = {} (must be >0)",
                    liq.bucket_retention_days
                ),
            });
        }
        // 0 = "synchronize with TF cadence" (valid default, see default_cluster_refresh_secs).
        if liq.cluster_refresh_secs != 0 && liq.cluster_refresh_secs < 1 {
            return Err(ConfigError::InvalidNumeric {
                detail: format!(
                    "[workspace.liquidity].cluster_refresh_secs = {} (must be 0 or >=1)",
                    liq.cluster_refresh_secs
                ),
            });
        }
        if !(liq.min_cluster_notional_usd.is_finite() && liq.min_cluster_notional_usd >= 0.0) {
            return Err(ConfigError::InvalidNumeric {
                detail: format!(
                    "[workspace.liquidity].min_cluster_notional_usd = {} (must be >=0)",
                    liq.min_cluster_notional_usd
                ),
            });
        }
    }
    {
        let hm = &ws.heatmap;
        if !(hm.bucket_size_pct.is_finite()
            && hm.bucket_size_pct > 0.0
            && hm.bucket_size_pct <= 1.0)
        {
            return Err(ConfigError::InvalidNumeric {
                detail: format!(
                    "[workspace.heatmap].bucket_size_pct = {} (must be in (0,1])",
                    hm.bucket_size_pct
                ),
            });
        }
        if hm.retention_secs == 0 {
            return Err(ConfigError::InvalidNumeric {
                detail: format!("[workspace.heatmap].retention_secs = 0 (must be >0)"),
            });
        }
    }
    Ok(())
}

fn validate_indicators_config(ind: &IndicatorsConfig, prefix: &str) -> Result<()> {
    macro_rules! check_period {
        ($field:ident) => {
            if ind.$field == 0 {
                return Err(ConfigError::InvalidNumeric {
                    detail: format!("{}{} = 0 (must be >0)", prefix, stringify!($field)),
                });
            }
        };
    }
    check_period!(ema_fast);
    check_period!(ema_medium);
    check_period!(ema_slow);
    check_period!(ema_long);
    check_period!(rsi_period);
    check_period!(macd_fast);
    check_period!(macd_slow);
    check_period!(macd_signal);
    check_period!(adx_period);
    check_period!(atr_period);
    check_period!(squeeze_period);
    check_period!(stoch_k_period);
    check_period!(stoch_d_period);
    check_period!(stoch_s_period);
    check_period!(chandemo_period);
    check_period!(supertrend_period);
    check_period!(keltner_ema_period);
    check_period!(keltner_atr_period);
    check_period!(donchian_period);
    check_period!(obv_smoothing);
    check_period!(cmf_period);
    check_period!(mfi_period);
    check_period!(hv_period);
    check_period!(aroon_period);
    check_period!(chop_period);
    check_period!(linreg_period);
    check_period!(zscore_period);
    check_period!(bbwp_lookback);
    check_period!(bbwp_period);
    check_period!(squeeze_bb_period);
    check_period!(squeeze_kc_period);
    check_period!(volume_average_period);
    check_period!(ichimoku_tenkan);
    check_period!(ichimoku_kijun);
    check_period!(ichimoku_senkou_b);
    check_period!(ichimoku_displacement);
    check_period!(cci_period);
    check_period!(williams_r_period);
    check_period!(hull_ma_period);
    check_period!(force_index_smoothing);
    check_period!(stddev_channel_period);
    check_period!(smc_lookback);
    check_period!(volume_profile_bins);
    check_period!(volume_profile_window);
    if ind.supertrend_multiplier <= 0.0 || !ind.supertrend_multiplier.is_finite() {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "{}supertrend_multiplier = {} (must be >0)",
                prefix, ind.supertrend_multiplier
            ),
        });
    }
    if ind.keltner_multiplier <= 0.0 || !ind.keltner_multiplier.is_finite() {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "{}keltner_multiplier = {} (must be >0)",
                prefix, ind.keltner_multiplier
            ),
        });
    }
    if ind.volume_profile_value_area <= 0.0
        || ind.volume_profile_value_area > 1.0
        || !ind.volume_profile_value_area.is_finite()
    {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "{}volume_profile_value_area = {} (must be in (0,1])",
                prefix, ind.volume_profile_value_area
            ),
        });
    }
    Ok(())
}

/// M8: platform-level numeric guards (`load_platform` path) — the median
/// filter window must be ≥ 1. K1: bind is loopback-only (single-operator
/// local deployment, no LAN exposure).
fn validate_platform(platform: &PlatformConfig) -> Result<()> {
    if let Some(q) = &platform.quality {
        if q.median_window_size == 0 {
            return Err(ConfigError::InvalidNumeric {
                detail: "[quality].median_window_size = 0 (must be >= 1)".into(),
            });
        }
    }
    if platform.server.port == 0 {
        return Err(ConfigError::InvalidNumeric {
            detail: "[platform.server].port = 0 (must be 1..=65535)".into(),
        });
    }
    let bind = platform.server.bind.trim();
    if bind.is_empty() {
        return Err(ConfigError::InvalidNumeric {
            detail: "[platform.server].bind must not be empty".into(),
        });
    }
    // K1 single-operator: only loopback. Bare-metal manage.sh never needs
    // LAN binding; operators reaching the daemon remotely use ssh -L tunnel
    // per docs/01-02 §5. Any other bind is a misconfiguration, not a feature.
    const ALLOWED_BINDS: &[&str] = &["127.0.0.1", "::1", "localhost"];
    if !ALLOWED_BINDS.contains(&bind) {
        return Err(ConfigError::InvalidNumeric {
            detail: format!(
                "[platform.server].bind = '{bind}' is not loopback — only {} allowed (single-operator local deployment; use ssh -L tunnel for remote access)",
                ALLOWED_BINDS.join(", ")
            ),
        });
    }
    if platform.candle_buffer.size == 0 {
        return Err(ConfigError::InvalidNumeric {
            detail: "[candle_buffer].size = 0 (must be >0)".into(),
        });
    }
    Ok(())
}

/// Serialize a `WorkspaceConfig` back to TOML and persist to `config.toml`.
///
/// The platform-level fields (exchanges, clock monitor) are not overwritten:
/// we read the current file, mutate the `[workspace]` table, and write the
/// file back. This preserves any platform-level edits the operator made
/// outside the workspace UI.
pub fn save_workspace(workspace: &WorkspaceConfig) -> Result<()> {
    validate_workspace(workspace)?;
    assert_no_legacy_files()?;
    let path = config_path();

    // Re-read the file so we preserve the [platform] section unchanged.
    let raw = std::fs::read_to_string(&path).map_err(|e| ConfigError::Io {
        path: path.clone(),
        source: e,
    })?;
    let on_disk: OnDiskConfig = toml::from_str(&raw).map_err(|e| ConfigError::Parse {
        path: path.clone(),
        source: e,
    })?;
    let new_raw = OnDiskConfig {
        hyperliquid: on_disk.hyperliquid,
        bitget: on_disk.bitget,
        clock_monitor: on_disk.clock_monitor,
        quality: on_disk.quality,
        reconnect: on_disk.reconnect,
        candle_buffer: on_disk.candle_buffer,
        snapshot_export: on_disk.snapshot_export,
        server: on_disk.server,
        workspace: workspace.clone(),
    };
    let serialized = toml::to_string_pretty(&new_raw)?;

    // v11.6 crash-safe write: (1) refresh the last-good backup of the
    // CURRENT (known-parseable) file, (2) write to a sibling temp file,
    // (3) atomically rename over the target. A kill/power-loss at any
    // point can no longer truncate `config.toml` — the worst case is a
    // leftover `.tmp` (harmless) and a `.bak` one revision behind.
    let bak = path.with_extension("toml.bak");
    if std::fs::copy(&path, &bak).is_err() {
        eprintln!(
            "[config] warning: could not refresh {} (continuing)",
            bak.display()
        );
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &serialized).map_err(|e| ConfigError::Io {
        path: tmp.clone(),
        source: e,
    })?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(ConfigError::Io {
            path: path.clone(),
            source: e,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
[workspace]
id = "main"
name = "Test"
default_currency = "USDC"
default_exchange = "Hyperliquid"

[[workspace.instances]]
id = "btc"
symbol = "BTC-USDT"
quote = "USDT"
"#;
        let cfg: OnDiskConfig = toml::from_str(toml).expect("parse");
        let (platform, workspace) = cfg.split();
        assert_eq!(workspace.id, "main");
        assert_eq!(workspace.instances.len(), 1);
        assert_eq!(workspace.instances[0].symbol, "BTC-USDT");
        assert_eq!(workspace.candles.duration_seconds, 60); // default
        assert!(platform.clock_monitor.is_none());
        // Server defaults: loopback + 3000 when [platform.server] is absent.
        assert_eq!(platform.server.bind, "127.0.0.1");
        assert_eq!(platform.server.port, 3000);
    }

    #[test]
    fn platform_server_section_overrides_defaults() {
        let toml = r#"
[server]
bind = "127.0.0.1"
port = 8080

[workspace]
id = "main"
name = "Test"
default_currency = "USDC"
default_exchange = "Hyperliquid"
"#;
        let cfg: OnDiskConfig = toml::from_str(toml).expect("parse");
        let (platform, _workspace) = cfg.split();
        assert_eq!(platform.server.bind, "127.0.0.1");
        assert_eq!(platform.server.port, 8080);
        assert!(validate_platform(&platform).is_ok());
    }

    #[test]
    fn platform_server_non_loopback_rejected() {
        let mut platform = PlatformConfig::default();
        platform.server.bind = "0.0.0.0".to_string();
        assert!(validate_platform(&platform).is_err());
        platform.server.bind = "192.168.1.10".to_string();
        assert!(validate_platform(&platform).is_err());
    }

    #[test]
    fn platform_server_port_zero_rejected() {
        let mut platform = PlatformConfig::default();
        platform.server.port = 0;
        assert!(validate_platform(&platform).is_err());
    }

    #[test]
    fn parse_partial_instance_indicator_override() {
        let toml = r#"
[workspace]
id = "main"
name = "Test"
default_currency = "USDC"
default_exchange = "Hyperliquid"

[[workspace.instances]]
id = "btc"
symbol = "BTC-USDT"
quote = "USDT"

[workspace.instances.timeframes.60]
candles = { duration_seconds = 60 }
indicators = { rsi_period = 21 }

[workspace.instances.timeframes.180]
candles = { duration_seconds = 180 }
indicators = { rsi_period = 14 }
"#;
        let cfg: OnDiskConfig = toml::from_str(toml).expect("partial indicators must parse");
        let (_platform, workspace) = cfg.split();
        let tf60 = &workspace.instances[0].timeframes[&60].indicators;
        assert_eq!(tf60.rsi_period, 21);
        assert_eq!(tf60.ema_fast, 10);
        assert_eq!(tf60.ema_long, 200);
        assert_eq!(tf60.macd_slow, 26);
        assert_eq!(tf60.squeeze_period, 20);
    }

    #[test]
    fn indicators_default_is_not_zero() {
        let cfg = IndicatorsConfig::default();
        assert_eq!(cfg.ema_fast, 10);
        assert_eq!(cfg.ema_medium, 50);
        assert_eq!(cfg.ema_slow, 100);
        assert_eq!(cfg.ema_long, 200);
        assert_eq!(cfg.rsi_period, 14);
        assert_eq!(cfg.macd_fast, 12);
        assert_eq!(cfg.macd_slow, 26);
        assert_eq!(cfg.macd_signal, 9);
        assert_eq!(cfg.adx_period, 14);
        assert_eq!(cfg.atr_period, 14);
        assert_eq!(cfg.squeeze_period, 20);
    }

    #[test]
    fn parse_empty_workspace_is_error() {
        let toml = "";
        let r: std::result::Result<OnDiskConfig, _> = toml::from_str(toml);
        assert!(r.is_err(), "missing [workspace] must fail parse");
    }

    #[test]
    fn tf_ladder_defaults_match_supported_pool() {
        // v11.9 parity gate: `tf_ladder_defaults` returns the full
        // 14-duration supported pool regardless of the (now ignored)
        // workspace slow/macro keys.
        let mut ws = WorkspaceConfig::default();
        ws.slow_timeframe.duration_seconds = 300;
        ws.macro_timeframe.duration_seconds = 900;
        assert_eq!(ws.tf_ladder_defaults(), SUPPORTED_DURATIONS.to_vec());
        assert_eq!(
            SUPPORTED_DURATIONS,
            [1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400, 43200, 86400]
        );
    }

    #[test]
    fn corrupt_config_recovers_from_bak_then_template() {
        // v11.6: the loader's crash-recovery chain — corrupt config.toml →
        // .bak restore → factory template restore, corrupt copy quarantined.
        let dir = std::env::temp_dir().join(format!(
            "qtp_cfg_recovery_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("config.toml");
        let bak = dir.join("config.toml.bak");
        let template = dir.join("config.default.toml");
        std::env::set_var("MARKET_MONITOR_CONFIG", &cfg);

        // (1) corrupt file + valid .bak → recovered from the .bak.
        std::fs::write(&cfg, "not [valid toml").unwrap();
        let template_src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config.default.toml"
        ))
        .unwrap();
        std::fs::write(&bak, &template_src).unwrap();
        let ws = load_workspace().unwrap();
        assert_eq!(ws.timeframes, DEFAULT_TIMEFRAMES.to_vec());
        assert!(cfg.exists());

        // (2) corrupt file + no .bak + template present → factory restore.
        std::fs::remove_file(&bak).unwrap();
        std::fs::write(&cfg, "\u{0}garbage").unwrap();
        std::fs::write(&template, &template_src).unwrap();
        let ws = load_workspace().unwrap();
        assert_eq!(ws.timeframes, DEFAULT_TIMEFRAMES.to_vec());

        // cleanup
        std::env::remove_var("MARKET_MONITOR_CONFIG");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file("config.toml");
        let _ = std::fs::remove_file("config.toml.corrupt-0");
    }

    #[test]
    fn active_timeframes_default_is_fastest_eight() {
        // v11.9: the default active set is the fastest 8 durations of the
        // supported pool; the accessor heals order/dupes/out-of-pool input.
        let mut ws = WorkspaceConfig::default();
        assert_eq!(ws.timeframes, DEFAULT_TIMEFRAMES.to_vec());
        assert_eq!(
            ws.active_timeframes_for(),
            vec![1, 3, 5, 15, 30, 60, 180, 300]
        );
        ws.timeframes = vec![3600];
        assert_eq!(ws.active_timeframes_for(), vec![3600]);
        ws.timeframes = vec![86400, 1, 300];
        // Ascending canonical order regardless of the written order.
        assert_eq!(ws.active_timeframes_for(), vec![1, 300, 86400]);
        // Out-of-pool entries are dropped defensively (validation rejects).
        ws.timeframes = vec![60, 240];
        assert_eq!(ws.active_timeframes_for(), vec![60]);
        ws.timeframes = vec![900, 900, 30];
        assert_eq!(ws.active_timeframes_for(), vec![30, 900]);
    }

    #[test]
    fn timeframes_validation_rejects_bad_sets() {
        let mut ws = WorkspaceConfig::default();
        ws.timeframes = Vec::new();
        assert!(validate_workspace(&ws).is_err());
        ws.timeframes = SUPPORTED_DURATIONS.to_vec();
        assert!(validate_workspace(&ws).is_ok());
        ws.timeframes = vec![60, 240];
        assert!(validate_workspace(&ws).is_err());
        ws.timeframes = vec![60, 60];
        assert!(validate_workspace(&ws).is_err());
        ws.timeframes = vec![60];
        assert!(validate_workspace(&ws).is_ok());
        ws.timeframes = vec![
            1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400, 43200, 86400,
        ];
        assert!(validate_workspace(&ws).is_ok());
    }

    #[test]
    fn execution_mode_serde_roundtrip_observe() {
        let toml = r#"
[workspace]
id = "main"
name = "Test"
default_currency = "USDC"
default_exchange = "Hyperliquid"

[[workspace.instances]]
id = "btc"
symbol = "BTC-USDT"
quote = "USDT"
mode = "observe"
"#;
        let cfg: OnDiskConfig = toml::from_str(toml).expect("observe mode must parse");
        let (_platform, workspace) = cfg.split();
        assert_eq!(workspace.instances[0].mode, ExecutionMode::Observe);

        let serialized = toml::to_string(&workspace).expect("roundtrip");
        assert!(serialized.contains("mode = \"observe\""));
    }

    #[test]
    fn default_workspace_has_zero_instances() {
        let ws = WorkspaceConfig::default();
        assert_eq!(ws.id, "main");
        assert_eq!(ws.instances.len(), 0);
        assert_eq!(ws.default_currency, "USDC");
    }

    #[test]
    fn assert_no_legacy_files_returns_ok_when_clean() {
        assert!(assert_no_legacy_files().is_ok());
    }

    #[test]
    fn declared_symbols_extracts_instance_symbols() {
        let mut ws = WorkspaceConfig::default();
        ws.instances.push(InstanceEntry {
            id: "btc".into(),
            symbol: "BTC-USDT".into(),
            quote: "USDT".into(),
            status: InstanceStatus::Running,
            timeframes: std::collections::BTreeMap::new(),
            strategy: None,
            automation: AutomationConfig::default(),
            operational_mode: OperationalMode::Advisory,
            mode: ExecutionMode::default(),
            allocation_pct: None,
            weight_overrides: None,
            activation: None,
        });
        ws.instances.push(InstanceEntry {
            id: "eth".into(),
            symbol: "ETH-USDT".into(),
            quote: "USDT".into(),
            status: InstanceStatus::Running,
            timeframes: std::collections::BTreeMap::new(),
            strategy: None,
            automation: AutomationConfig::default(),
            operational_mode: OperationalMode::Advisory,
            mode: ExecutionMode::default(),
            allocation_pct: None,
            weight_overrides: None,
            activation: None,
        });
        let syms = ws.declared_symbols();
        assert_eq!(syms, vec!["BTC-USDT", "ETH-USDT"]);
    }

    #[test]
    fn legacy_file_detection() {
        // Create a fake legacy file in /tmp and verify the assertion works
        // against an absolute path. We don't write to CWD here because that
        // would pollute the workspace.
        let tmp = std::env::temp_dir().join("market_monitor_legacy_test");
        std::fs::create_dir_all(&tmp).unwrap();
        let fake = tmp.join("instances.json");
        std::fs::write(&fake, "{}").unwrap();
        // We're not in `tmp`, so this assertion should succeed (no legacy
        // files in CWD).
        assert!(assert_no_legacy_files().is_ok());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn legacy_timeframe_keys_rejected_at_load() {
        // D2: named-slot keys are hard-rejected with a migration message.
        let cases = [
            r#"
[workspace]
id = "main"
active_slots = ["micro1", "fast1"]
"#,
            r#"
[workspace]
id = "main"
active_timeframes = 5
"#,
            r#"
[workspace]
id = "main"

[[workspace.instances]]
id = "btc"
symbol = "BTC-USDT"

[workspace.instances.micro_term]
candles = { duration_seconds = 60 }
"#,
            r#"
[workspace]
id = "main"

[[workspace.instances]]
id = "btc"
symbol = "BTC-USDT"
custom_pipelines = { 5 = { candles = { duration_seconds = 120 } } }
"#,
        ];
        for raw in cases {
            let err = reject_legacy_timeframe_keys(raw).expect_err("legacy key must be rejected");
            assert!(matches!(err, ConfigError::LegacyTimeframeKey { .. }));
        }
        assert!(
            reject_legacy_timeframe_keys("[workspace]\nid = \"main\"\ntimeframes = [1, 60]\n")
                .is_ok()
        );
    }

    #[test]
    fn zero_valued_periods_rejected_at_load() {
        // M8 (production audit): zero periods panic in the hot path
        // (Decimal/u64 division, median-window indexing) — reject at boot.
        // v11.9: per-duration overrides take effect directly, so a zero
        // indicator period inside `timeframes.<secs>` is rejected.
        let mut bad_rsi = InstanceEntry {
            id: "btc".into(),
            symbol: "BTC-USDT".into(),
            quote: "USDT".into(),
            status: InstanceStatus::Running,
            strategy: None,
            automation: AutomationConfig::default(),
            operational_mode: OperationalMode::Advisory,
            mode: ExecutionMode::default(),
            allocation_pct: None,
            weight_overrides: None,
            activation: None,
            timeframes: std::collections::BTreeMap::new(),
        };
        bad_rsi.timeframes.insert(
            60,
            TimeframeConfig {
                indicators: IndicatorsConfig {
                    rsi_period: 0,
                    ..IndicatorsConfig::default()
                },
                ..TimeframeConfig::new(60, IndicatorsConfig::default())
            },
        );
        let ws = WorkspaceConfig {
            instances: vec![bad_rsi],
            ..WorkspaceConfig::default()
        };
        assert!(matches!(
            validate_workspace(&ws),
            Err(ConfigError::InvalidNumeric { .. })
        ));

        // Key/duration mismatch is rejected too.
        let mut mismatched = InstanceEntry {
            id: "btc".into(),
            symbol: "BTC-USDT".into(),
            quote: "USDT".into(),
            status: InstanceStatus::Running,
            strategy: None,
            automation: AutomationConfig::default(),
            operational_mode: OperationalMode::Advisory,
            mode: ExecutionMode::default(),
            allocation_pct: None,
            weight_overrides: None,
            activation: None,
            timeframes: std::collections::BTreeMap::new(),
        };
        mismatched
            .timeframes
            .insert(60, TimeframeConfig::new(180, IndicatorsConfig::default()));
        let ws = WorkspaceConfig {
            instances: vec![mismatched],
            ..WorkspaceConfig::default()
        };
        assert!(matches!(
            validate_workspace(&ws),
            Err(ConfigError::InvalidNumeric { .. })
        ));

        let mut ws_bad_rsi = WorkspaceConfig::default();
        ws_bad_rsi.indicators.rsi_period = 0;
        assert!(matches!(
            validate_workspace(&ws_bad_rsi),
            Err(ConfigError::InvalidNumeric { .. })
        ));

        // Platform side: median window 0 rejected.
        let platform = PlatformConfig {
            quality: Some(QualityConfig {
                median_window_size: 0,
                ..QualityConfig::default()
            }),
            ..PlatformConfig::default()
        };
        assert!(matches!(
            validate_platform(&platform),
            Err(ConfigError::InvalidNumeric { .. })
        ));
    }

    #[test]
    fn backtest_config_defaults_and_bounds() {
        // Defaults ship valid.
        let mut ws = WorkspaceConfig::default();
        assert_eq!(ws.backtest.archive_depth_days, 180);
        assert_eq!(ws.backtest.hyperliquid.page_cap, 1000);
        assert_eq!(ws.backtest.bitget.page_cap, 200);
        assert!(validate_workspace(&ws).is_ok());

        // Depth bounds: 0 and 366 must fail, 1 and 365 must pass.
        for bad in [0u32, 366] {
            ws.backtest.archive_depth_days = bad;
            assert!(
                matches!(
                    validate_workspace(&ws),
                    Err(ConfigError::InvalidNumeric { .. })
                ),
                "depth {bad} must be rejected"
            );
        }
        for ok in [1u32, 365] {
            ws.backtest.archive_depth_days = ok;
            assert!(validate_workspace(&ws).is_ok(), "depth {ok} must pass");
        }

        // Warmup floor + page-cap sanity.
        ws.backtest.warmup_bars = 29;
        assert!(matches!(
            validate_workspace(&ws),
            Err(ConfigError::InvalidNumeric { .. })
        ));
        ws.backtest.warmup_bars = 30;
        ws.backtest.hyperliquid.page_cap = 0;
        assert!(matches!(
            validate_workspace(&ws),
            Err(ConfigError::InvalidNumeric { .. })
        ));
        ws.backtest.hyperliquid.page_cap = 1000;
        ws.backtest.bitget.max_pages_per_run = 0;
        assert!(matches!(
            validate_workspace(&ws),
            Err(ConfigError::InvalidNumeric { .. })
        ));
    }
}
