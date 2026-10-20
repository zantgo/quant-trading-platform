use rust_decimal::prelude::ToPrimitive;
use std::sync::Arc;
use tokio::sync::RwLock;

use core_domain::alignment::{self, AlignmentMatrix};
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::market_context::MarketContext;
use core_domain::models::MarketSnapshot;
use std::collections::HashMap;

pub fn synthesize_cross_tf(
    symbol: &str,
    tf_snapshots: &[(u64, &MarketSnapshot)],
    alignment_params: &core_domain::alignment::AlignmentParams,
) -> AlignmentMatrix {
    // AUDIT-C2: labels are collected as owned `String`s first, then borrowed
    // for the alignment call. Previously the label was created *inside* the
    // filter_map closure and coerced to `&'static str` via `Box::leak` — one
    // leaked allocation per timeframe per candle close, unbounded over the
    // daemon lifetime (10+ MB/day on 1 s TFs). No allocation escapes here.
    type TfLabelInput<'a> = (
        String,
        u64,
        f64,
        &'a HashMap<String, NormalizedIndicatorValue>,
        &'a MarketContext,
    );
    type TimeframeInput<'a> = (
        &'a str,
        u64,
        f64,
        &'a HashMap<String, NormalizedIndicatorValue>,
        &'a MarketContext,
    );
    let tf_data_owned: Vec<TfLabelInput<'_>> = tf_snapshots
        .iter()
        .filter_map(|(secs, snap)| {
            let ctx = snap.context.as_ref()?;
            let price = snap.close.as_ref().and_then(|d| d.to_f64()).unwrap_or(0.0);
            Some((slot_label(snap), *secs, price, &snap.indicators, ctx))
        })
        .collect();
    let tf_data: Vec<TimeframeInput<'_>> = tf_data_owned
        .iter()
        .map(|(label, secs, price, map, ctx)| (label.as_str(), *secs, *price, *map, *ctx))
        .collect();
    let tf_states: Vec<(u64, Option<&core_domain::axes::TimeframeState>)> = tf_snapshots
        .iter()
        .filter(|(_, snap)| snap.context.is_some())
        .map(|(secs, snap)| (*secs, snap.timeframe_state.as_ref()))
        .collect();

    alignment::compute_alignment_with_states(symbol, &tf_data, &tf_states, alignment_params)
}

fn slot_label(snap: &MarketSnapshot) -> String {
    core_domain::duration_label(snap.timeframe_secs)
}

pub struct SynthesisContext {
    pub micro_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
    pub fast_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
    pub slow_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
    pub macro_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
}

impl SynthesisContext {
    pub async fn gather_snapshots(&self) -> Vec<(u64, MarketSnapshot)> {
        let mut out = Vec::with_capacity(4);
        if let Some(s) = self.micro_snapshot.read().await.clone() {
            out.push((s.timeframe_secs, s));
        }
        if let Some(s) = self.fast_snapshot.read().await.clone() {
            out.push((s.timeframe_secs, s));
        }
        if let Some(s) = self.slow_snapshot.read().await.clone() {
            out.push((s.timeframe_secs, s));
        }
        if let Some(s) = self.macro_snapshot.read().await.clone() {
            out.push((s.timeframe_secs, s));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::market_context::{ContextDimension, MarketContext};
    use core_domain::models::MarketSnapshot;
    use rust_decimal::Decimal;

    fn ctx(trend: f64, momentum: f64, overall: i32) -> MarketContext {
        MarketContext {
            trend: ContextDimension {
                score: trend,
                confidence: 0.7,
                label: "WEAK_BULL".into(),
            },
            momentum: ContextDimension {
                score: momentum,
                confidence: 0.6,
                label: "WEAK_BULL".into(),
            },
            volatility: ContextDimension {
                score: 0.0,
                confidence: 0.5,
                label: "NORMAL".into(),
            },
            volume: ContextDimension {
                score: 0.0,
                confidence: 0.5,
                label: "NORMAL".into(),
            },
            liquidity: ContextDimension::neutral(),
            regime: "TRENDING_BULL".into(),
            overall_score: overall,
            overall_label: if overall > 20 {
                "BULLISH".into()
            } else {
                "NEUTRAL".into()
            },
        }
    }

    fn snap(secs: u64, price: f64, context: MarketContext) -> MarketSnapshot {
        let mut m = MarketSnapshot {
            symbol: "BTC-USDC".into(),
            timeframe_secs: secs,
            timeframe_label: Some(format!("{secs}s")),
            mid_price: Decimal::from_f64_retain(price).unwrap(),
            close: Decimal::from_f64_retain(price),
            indicators: Default::default(),
            ..MarketSnapshot::default()
        };
        m.context = Some(context);
        m
    }

    fn synth(snaps: &[&MarketSnapshot]) -> AlignmentMatrix {
        let pairs: Vec<(u64, &MarketSnapshot)> =
            snaps.iter().map(|s| (s.timeframe_secs, *s)).collect();
        let alignment = core_domain::alignment::AlignmentParams::default();
        synthesize_cross_tf("BTC-USDC", &pairs, &alignment)
    }

    #[test]
    fn no_durations_yields_an_empty_alignment() {
        let r = synth(&[]);
        assert_eq!(r.timeframes_present, 0);
        assert_eq!(r.symbol, "BTC-USDC");
    }

    #[test]
    fn one_duration_is_present() {
        let s = snap(60, 100.0, ctx(0.6, 0.4, 40));
        let r = synth(&[(&s)]);
        assert_eq!(r.timeframes_present, 1);
    }

    #[test]
    fn the_score_aggregates_across_durations() {
        let fast = snap(60, 100.0, ctx(0.9, 0.5, 80));
        let slow = snap(3600, 100.0, ctx(0.9, 0.5, 80));
        let both = synth(&[(&fast), (&slow)]);
        let fast_only = synth(&[(&fast)]);

        assert_eq!(both.timeframes_present, 2);
        assert_eq!(fast_only.timeframes_present, 1);
        assert!(
            both.mtf_overall_score != fast_only.mtf_overall_score,
            "the cross-duration score must aggregate; both={}, fast={}",
            both.mtf_overall_score,
            fast_only.mtf_overall_score
        );
    }
}
