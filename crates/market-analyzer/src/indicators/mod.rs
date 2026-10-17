//! # Indicator Calculators
//!
//! ## Per-timeframe scope contract (v6.11 audit)
//!
//! **Every indicator in this module is per-timeframe pure state.** Each
//! calculator owns its own internal history buffers (`VecDeque`,
//! `Vec<f64>`, running sums, etc.) and is instantiated once per pipeline
//! slot (`Micro`/`Fast`/`Slow`/`Macro`) by `analyzer::run_single` and the
//! warm-up path. No indicator reads from a sibling timeframe's state, a
//! shared `Arc<RwLock<...>>`, `ActivePair`, or any cross-TF snapshot.
//!
//! The only cross-TF data flow in the crate is the **synthesis layer**
//! (`crates/market-analyzer/src/synthesis.rs`, invoked from
//! `analyzer/mod.rs::synthesize_cross_tf`), which consumes the finished
//! per-TF `MarketSnapshot`s to build the L2–L6 Alignment/Analysis/Risk/
//! Advisory matrices. That is an intentional product-level aggregation of
//! *already-computed* per-TF indicators — it never feeds back into the
//! per-bar indicator math.
//!
//! The one intentional shared exception is the derivatives trio
//! (`open_interest` / `funding_rate` / `mark_index_spread`): exchange-level
//! data is not timeframe-derived, so `latest_oi` / `latest_funding` /
//! `latest_mark_px` / `latest_index_px` are shared per-pair by design.
//!
//! Audit date: v6.11 — verified across all 52 indicators, both supported
//! exchanges (Hyperliquid + Bitget), and every timeframe (1s/3s/5s/15s and
//! 60s/180s/300s/900s). Regression pins: `crates/market-analyzer/tests/
//! sub_minute_indicator_cadence.rs`.

pub mod adx;
pub mod anchored_vwap;
pub mod aroon;
pub mod atr;
pub mod awesome_oscillator;
pub mod bbwp;
pub mod bollinger;
pub mod candlestick;
pub mod cci;
pub mod chandemo;
pub mod choppiness;
pub mod cmf;
pub mod divergence;
pub mod donchian;
pub mod ema;
pub mod fibonacci;
pub mod force_index;
pub mod hull_ma;
pub mod hv;
pub mod ichimoku;
pub mod keltner;
pub mod linreg_slope;
pub mod macd;
pub mod mfi;
pub mod normalized;
pub mod obv;
pub mod patterns;
pub mod pivot_points;
pub mod psar;
pub mod ratio;
pub mod registry;
pub mod rma;
pub mod rsi;
pub mod sma;
pub mod smart_money;
pub mod squeeze;
pub mod stddev_channel;
pub mod stochastic;
pub mod supertrend;
pub mod traits;
pub mod volume_profile;
pub mod williams_r;
pub mod zscore;

pub use adx::{Adx, AdxOutput, DiCrossoverDir, TrendRegime};
pub use anchored_vwap::{AnchoredVwap, AvwapOutput};
pub use aroon::{Aroon, AroonOutput};
pub use atr::{Atr, AtrOutput, VolatilityRegime};
pub use awesome_oscillator::{AoOutput, AwesomeOscillator};
pub use bbwp::Bbwp;
pub use bollinger::BollingerBands;
pub use candlestick::{
    Candlestick, CandlestickConfig, CandlestickPattern, CandlestickResult, CandlestickStatus,
};
pub use cci::Cci;
pub use chandemo::ChandeMO;
pub use choppiness::Choppiness;
pub use cmf::Cmf;
pub use divergence::{
    DivergenceCoords, DivergenceDetector, DivergenceResult, DivergenceStatus, DivergenceType,
    PeakTrough, SeriesDivergence, SeriesDivergenceResult,
};
pub use donchian::{Donchian, DonchianOutput};
pub use ema::Ema;
pub use fibonacci::{FibonacciRange, PivotPoint, PivotType, SwingLegType};
pub use force_index::ForceIndex;
pub use hull_ma::HullMA;
pub use hv::HistoricalVolatility;
pub use ichimoku::{Ichimoku, IchimokuOutput};
pub use keltner::{Keltner, KeltnerOutput};
pub use linreg_slope::LinRegSlope;
pub use macd::{CrossoverDir, Macd, MacdOutput, TrendState};
pub use mfi::Mfi;
pub use normalized::{
    DivergenceState, IndicatorInputs, IndicatorSignal, NormalizationContext, NormalizationEngine,
    NormalizedIndicatorValue, SignalDirection, SignalKind, SignalPoint, SignalStatus,
};
pub use obv::{Obv, ObvOutput};
pub use patterns::{detect_pattern, ChartPattern, PatternResult};
pub use pivot_points::{PivotLevels, PivotMethod, PivotPoints};
pub use psar::{ParabolicSar, PsarOutput};
pub use ratio::{annualization_factor, sharpe_ratio_annualized, SHARPE_WINDOW};
pub use registry::{IndicatorClass, IndicatorGroup, IndicatorMeta, RenderKind, INDICATORS};
pub use rma::WilderRma;
pub use rsi::Rsi;
pub use sma::Sma;
pub use smart_money::{MarketStructure, SmartMoney, SmcOutput};
pub use squeeze::{MomentumDirection, SqueezeMomentum, SqueezeOutput};
pub use stddev_channel::{SdChannelOutput, StdDevChannel};
pub use stochastic::{Stochastic, StochasticOutput};
pub use supertrend::{Supertrend, SupertrendOutput};
pub use traits::{BarInput, Indicator};
pub use volume_profile::{VolumeProfile, VolumeProfileOutput};
pub use williams_r::WilliamsR;
pub use zscore::ZScore;
