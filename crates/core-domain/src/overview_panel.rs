//! # Overview Panel — server-computed dashboard payload
//!
//! Single-source-of-truth builder for the Market Overview panel fields that the
//! GUI previously derived client-side in TypeScript. The builder is **pure**: it
//! consumes the per-instance snapshots the L7 aggregation task already reads plus
//! the canonical AssetRank scores, and produces the panel payload. The L7 task
//! merges it into `OverviewMatrix`; both renderers (Svelte `GeneralDashboard`,
//! the observe shell) then display the same server-computed values — GUI and CLI
//! can never disagree for the same instances.
//!
//! CR-15 removed the hero verdict, the per-profile viability pass, the
//! direction-resolution chain and the R:R resolution chain. Each of those was a
//! statement about *what to do* — an entry, a target, an invalidation, a
//! go/no-go verdict — and none of it is renderable in observe mode. What the
//! model can say about an instance now lives in `OverviewRow`, and nothing else.

use std::collections::HashMap;

use crate::models::MarketSnapshot;
use serde::{Deserialize, Serialize};

/// `Decimal` → `f64` for the panel's display fields. `to_string().parse()` is
/// what the rest of the pipeline used before `rust_decimal::to_f64` existed;
/// it is kept here because a price of zero and an absent price are both `0.0`
/// in this panel's contract.
fn f64_from_decimal(d: rust_decimal::Decimal) -> f64 {
    d.to_string().parse().unwrap_or(0.0)
}

// ─── Panel payload types ─────────────────────────────────────────────

/// One asset-ranking row — the model-only instance row.
///
/// CR-15 removed eight columns from this table: `signal` (the L6 gate verdict),
/// `rr`, `confidence`, `risk`, `setup_side`, and the `entry_low` / `entry_high` /
/// `target_low` / `target_high` / `invalidation` geometry. Every one of them was
/// strategy — a statement about what to do — and none is renderable in observe
/// mode. What remains is what the market-state model can say about an instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverviewRow {
    pub symbol: String,
    /// Micro-window mid price (0.0 before the first snapshot).
    pub price: f64,
    /// PascalCase `MarketBias` (`Bullish` / `Neutral` / ...) — model state.
    pub bias: String,
    /// Canonical L7 AssetRank score — model state.
    pub score: f64,
    /// `AlignmentMatrix.mtf_overall_score` — cross-duration model state.
    pub mtf_score: f64,
    /// `AlignmentMatrix.mtf_overall_label`.
    pub mtf_label: String,
    /// Micro-window snapshot timestamp (seconds since epoch).
    pub updated_ts: u64,
    /// Lifecycle-active (not stopped/cancelled).
    pub active: bool,
}

/// One market-health sub-dimension bar (quality value, high = good).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthBar {
    pub label: String,
    /// 0..100 quality value (risk inverted where the spec says so).
    pub value: f64,
    /// `false` when at least one instance exists but produced no data.
    pub available: bool,
    /// How many instances fed this bar.
    pub contributing_instances: u32,
}

/// The Market Health card's four sub-dimension bars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketHealthDims {
    pub bars: Vec<HealthBar>,
    /// Instances that contributed to at least one bar.
    pub active_instance_count: u32,
}

/// Everything the L7 task merges into `OverviewMatrix` (serde-defaulted).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OverviewPanel {
    pub rows: Vec<OverviewRow>,
    pub market_health_dims: Option<MarketHealthDims>,
}

/// Per-instance builder input — the snapshots the L7 task already holds.
pub struct PanelInstance {
    pub symbol: String,
    /// Present per-TF snapshots, fastest first (micro, fast, slow, macro).
    /// The reference snapshot is `snapshots[0]` (same "fastest present
    /// window" rule the L7 alignment handling uses).
    pub snapshots: Vec<MarketSnapshot>,
    pub is_active: bool,
    /// Canonical L7 AssetRank score for the symbol (0.0 when not ranked).
    pub rank_score: f64,
}

/// Build the model-only overview panel: one row per instance plus the market
/// health card.
pub fn build_overview_panel(
    instances: &[PanelInstance],
    ranks: &HashMap<String, f64>,
) -> OverviewPanel {
    // ── Rows (asset ranking table) ───────────────────────────────────
    let mut rows: Vec<OverviewRow> = Vec::new();
    for inst in instances {
        let reference = inst.snapshots.first();
        // CR-15: the model halves only.
        let analysis = reference.and_then(|s| s.analysis.as_ref());
        let alignment = reference.and_then(|s| s.alignment.as_ref());

        let price = reference
            .map(|s| f64_from_decimal(s.mid_price))
            .unwrap_or(0.0);
        let bias = analysis
            .map(|a| format!("{:?}", a.bias))
            .unwrap_or_else(|| "Neutral".to_string());
        // CR-15: the canonical L7 AssetRank. The old fallback chain read the L4
        // opportunity matrix, which no longer exists on the snapshot.
        let score = ranks.get(&inst.symbol).copied().unwrap_or(0.0);
        let mtf_score = alignment.map(|a| a.mtf_overall_score).unwrap_or(0.0);
        let mtf_label = alignment
            .map(|a| a.mtf_overall_label.clone())
            .unwrap_or_else(|| "NO_DATA".to_string());
        let updated_ts = reference.map(|s| s.timestamp).unwrap_or(0);

        rows.push(OverviewRow {
            symbol: inst.symbol.clone(),
            price,
            bias,
            score,
            mtf_score,
            mtf_label,
            updated_ts,
            active: inst.is_active,
        });
    }

    // ── Market health ───────────────────────────────────────────────
    // CR-15: the four market-health sub-dimension bars were computed from
    // `RiskMatrix` dimensions. Risk is strategy, so they go with it; the card
    // keeps its aggregate from `compute_overview`, and the sub-bars return in
    // P10 derived from the four axes rather than from a risk score.
    let market_health_dims: Option<MarketHealthDims> = None;

    OverviewPanel {
        rows,
        market_health_dims,
    }
}

#[cfg(test)]
mod tests {
    //! Model-only panel tests. CR-15 removed the hero-verdict, viability,
    //! direction-resolution and R:R suites with the layers they exercised: none
    //! of those functions exists, so their tests could not survive. What is left
    //! is the panel's actual contract — a row per instance, built from the two
    //! model matrices, with no invented values when a matrix is absent.
    use super::*;
    use crate::alignment::AlignmentMatrix;
    use crate::analysis::{AnalysisMatrix, MarketBias};
    use rust_decimal::Decimal;

    fn snapshot(price: f64) -> MarketSnapshot {
        let mut s = MarketSnapshot {
            symbol: "BTC-USDC".into(),
            mid_price: Decimal::from_f64_retain(price).unwrap(),
            timestamp: 1_700_000_000,
            ..MarketSnapshot::default()
        };
        let mut a = AnalysisMatrix::empty("BTC-USDC");
        a.bias = MarketBias::Bullish;
        s.analysis = Some(a);
        let mut m = AlignmentMatrix::empty("BTC-USDC");
        m.mtf_overall_score = 72.0;
        m.mtf_overall_label = "BULLISH".into();
        s.alignment = Some(m);
        s
    }

    fn instance(symbol: &str, snaps: Vec<MarketSnapshot>, active: bool) -> PanelInstance {
        PanelInstance {
            symbol: symbol.into(),
            snapshots: snaps,
            is_active: active,
            rank_score: 0.0,
        }
    }

    #[test]
    fn one_row_per_instance() {
        let panel = build_overview_panel(
            &[
                instance("BTC-USDC", vec![snapshot(100.0)], true),
                instance("ETH-USDC", vec![snapshot(50.0)], true),
            ],
            &HashMap::new(),
        );
        assert_eq!(panel.rows.len(), 2);
        assert_eq!(panel.rows[0].symbol, "BTC-USDC");
        assert_eq!(panel.rows[1].symbol, "ETH-USDC");
    }

    #[test]
    fn row_reads_price_bias_and_cross_tf_agreement_from_the_model() {
        let panel = build_overview_panel(
            &[instance("BTC-USDC", vec![snapshot(100.0)], true)],
            &HashMap::from([("BTC-USDC".to_string(), 61.0)]),
        );
        let row = &panel.rows[0];
        assert_eq!(row.price, 100.0);
        assert_eq!(row.bias, "Bullish");
        assert_eq!(row.score, 61.0);
        assert_eq!(row.mtf_score, 72.0);
        assert_eq!(row.mtf_label, "BULLISH");
        assert_eq!(row.updated_ts, 1_700_000_000);
        assert!(row.active);
    }

    #[test]
    fn a_row_carries_no_strategy_field() {
        // The regression this file exists to prevent: a strategy column
        // creeping back onto the overview row. The struct is the wire contract,
        // so asserting the exact field set is the pin.
        let json = serde_json::to_value(OverviewRow {
            symbol: "BTC-USDC".into(),
            price: 1.0,
            bias: "Neutral".into(),
            score: 0.0,
            mtf_score: 0.0,
            mtf_label: "NO_DATA".into(),
            updated_ts: 0,
            active: true,
        })
        .unwrap();
        let mut keys: Vec<&String> = json.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "active",
                "bias",
                "mtf_label",
                "mtf_score",
                "price",
                "score",
                "symbol",
                "updated_ts",
            ]
        );
    }

    #[test]
    fn missing_matrices_render_unknown_never_invented() {
        let bare = MarketSnapshot {
            symbol: "SOL-USDC".into(),
            mid_price: Decimal::from_f64_retain(12.0).unwrap(),
            ..MarketSnapshot::default()
        };
        let panel =
            build_overview_panel(&[instance("SOL-USDC", vec![bare], false)], &HashMap::new());
        let row = &panel.rows[0];
        assert_eq!(row.bias, "Neutral", "absent analysis is not a bullish read");
        assert_eq!(
            row.mtf_label, "NO_DATA",
            "absent alignment must say NO_DATA"
        );
        assert_eq!(row.score, 0.0, "an unranked symbol scores 0.0");
        assert!(!row.active);
    }

    #[test]
    fn an_instance_with_no_snapshot_still_yields_a_row() {
        let panel = build_overview_panel(
            &[instance("NEW-USDC", vec![], true)],
            &HashMap::from([("NEW-USDC".to_string(), 40.0)]),
        );
        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].price, 0.0);
        assert_eq!(panel.rows[0].updated_ts, 0);
        assert_eq!(panel.rows[0].mtf_label, "NO_DATA");
    }
}
