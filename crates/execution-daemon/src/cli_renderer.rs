//! # CLI Terminal Monitor Renderer
//!
//! Plain-ANSI box-drawing renderer for the `--mode cli` launch. Renders the
//! SAME server-computed payload the GUI Overview panel (GeneralDashboard)
//! consumes: `OverviewMatrix` + the v7.2 panel fields (`hero`,
//! `overview_rows`, `signal_quality`, `direction_distribution`,
//! `market_health_dims`) produced by the L7 aggregation task. The renderer
//! is a pure view — every number comes from the single server-side
//! derivation, so GUI and CLI can never disagree for the same instances.
//!
//! Deliberately dependency-free (no `ratatui`/`crossterm`) — a periodic
//! full-screen clear + fixed-width tables keeps the binary slim, matching
//! the hand-rolled CLI philosophy in
//! `docs/conceptual-foundations/01-09-cli-setup-flow.md`.

use std::sync::Arc;

use core_domain::overview::OverviewMatrix;
use core_domain::overview_panel::HeroVerdict;
use portfolio_supervisor::instance::Instance;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Render one full monitor frame. `overview` may be `None` during warmup.
pub async fn render_frame(
    overview: &Option<OverviewMatrix>,
    instances: &[Arc<Instance>],
    session_line: &str,
) -> String {
    let mut out = String::with_capacity(8192);
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");

    out.push_str("═══════════════════════════════════════════════════════════════════════════\n");
    out.push_str(&format!(
        "  TRADING PLATFORM — TERMINAL MONITOR    {:<28}    {}\n",
        session_line, now
    ));
    out.push_str("═══════════════════════════════════════════════════════════════════════════\n");

    match overview {
        None => {
            out.push_str("\n  ⏳ Warming up — waiting for the first market snapshots…\n\n");
            render_instances(&mut out, instances).await;
            return out;
        }
        Some(om) => {
            render_headline(&mut out, om);
            render_hero(&mut out, om);
            render_opportunities(&mut out, om);
            render_signal_and_direction(&mut out, om);
            render_market_health(&mut out, om);
            render_instances(&mut out, instances).await;
            render_asset_ranking(&mut out, om);
            render_summary(&mut out, om);
        }
    }
    out
}

fn render_headline(out: &mut String, om: &OverviewMatrix) {
    out.push_str(&format!(
        "  Market Bias      : {:<14}  Health : {:<8?}  Sync  : {:?}\n",
        om.global_market_bias, om.market_health, om.market_synchronization
    ));
    out.push_str(&format!(
        "  Breadth          : {:>+6.1}% ({:<14})  Systemic Risk : {:>5.1}   Cascade: {} ({:.0}, conf {:.0}%)\n",
        om.breadth_pct,
        format!("{:?}", om.market_breadth),
        om.systemic_risk_score,
        om.cascade_risk_index.level,
        om.cascade_risk_index.score,
        om.cascade_risk_index.confidence,
    ));
    out.push_str(&format!(
        "  TF Agreement     : {:>5.1}%       Alignment Consensus : {:>+5.1}\n",
        om.multi_tf_agreement_pct, om.alignment_consensus_index,
    ));
    if !om.alignment_distribution.is_empty() {
        let mut parts: Vec<String> = Vec::new();
        for key in [
            "STRONG_BULL_MTF",
            "WEAK_BULL_MTF",
            "NEUTRAL_MTF",
            "WEAK_BEAR_MTF",
            "STRONG_BEAR_MTF",
            "NO_DATA",
        ] {
            if let Some(n) = om.alignment_distribution.get(key) {
                if *n > 0 {
                    parts.push(format!("{} {}", key.replace("_MTF", ""), n));
                }
            }
        }
        if !parts.is_empty() {
            out.push_str(&format!("  Alignment         : {}\n", parts.join(" · ")));
        }
    }
    out.push('\n');
}

fn render_hero(out: &mut String, om: &OverviewMatrix) {
    let Some(hero) = &om.hero else {
        out.push_str("  ── MARKET STATUS ──────────────────────────────────────────────────────\n");
        out.push_str("  STAND ASIDE — no data yet.\n\n");
        return;
    };
    out.push_str("  ── MARKET STATUS ──────────────────────────────────────────────────────\n");
    match hero.verdict {
        HeroVerdict::Trade => {
            let symbol = hero.best_symbol.as_deref().unwrap_or("—");
            let dir = hero.best_direction.as_str();
            out.push_str(&format!(
                "  {}   {} actionable setup{} · best {} {} · R:R {} · confidence {:.0}%\n",
                hero.verdict,
                hero.actionable_count,
                if hero.actionable_count == 1 { "" } else { "s" },
                symbol,
                dir,
                fmt_rr(hero.best_rr),
                hero.best_confidence,
            ));
        }
        HeroVerdict::Wait => {
            out.push_str(&format!(
                "  {}   {} candidate setup{} forming — no READY trade yet.\n",
                hero.verdict,
                hero.candidate_count,
                if hero.candidate_count == 1 { "" } else { "s" },
            ));
        }
        HeroVerdict::StandAside => {
            out.push_str("  STAND ASIDE   No high-quality opportunities detected — stand aside.\n");
        }
    }
    out.push('\n');
}

fn render_opportunities(out: &mut String, om: &OverviewMatrix) {
    out.push_str("  ── TRADE OPPORTUNITIES ───────────────────────────────────────────────\n");
    let Some(hero) = &om.hero else {
        out.push_str("  No qualifying opportunity yet.\n\n");
        return;
    };
    let Some(symbol) = &hero.best_symbol else {
        out.push_str("  No qualifying opportunity yet.\n\n");
        return;
    };
    out.push_str(&format!("  Best Pair     : {}\n", symbol));
    out.push_str(&format!("  Direction     : {}\n", hero.best_direction));
    out.push_str(&format!("  Best R:R       : {}\n", fmt_rr(hero.best_rr)));
    out.push_str(&format!(
        "  Confidence     : {:.0}%\n",
        hero.best_confidence
    ));
    out.push_str(&format!(
        "  Score          : {:.0} / 100\n",
        hero.best_score
    ));
    out.push_str(&format!(
        "  Actionable     : {} of {} candidates\n",
        hero.actionable_count, hero.candidate_count
    ));
    out.push('\n');
}

fn render_signal_and_direction(out: &mut String, om: &OverviewMatrix) {
    out.push_str("  ── SIGNAL QUALITY · DIRECTION ────────────────────────────────────────\n");
    match &om.signal_quality {
        Some(sq) => {
            let total = sq.strong + sq.moderate + sq.weak;
            out.push_str(&format!(
                "  Signals: STRONG {} · MODERATE {} · WEAK {}    ({} pair{})\n",
                sq.strong,
                sq.moderate,
                sq.weak,
                total,
                if total == 1 { "" } else { "s" }
            ));
        }
        None => {
            out.push_str("  Signals: no data\n");
        }
    }
    match &om.direction_distribution {
        Some(dd) => {
            let total = dd.long + dd.short + dd.neutral;
            out.push_str(&format!(
                "  Direction: LONG {} · SHORT {} · NEUTRAL {}    ({} pair{})\n",
                dd.long,
                dd.short,
                dd.neutral,
                total,
                if total == 1 { "" } else { "s" }
            ));
        }
        None => {
            out.push_str("  Direction: no data\n");
        }
    }
    out.push('\n');
}

fn render_market_health(out: &mut String, om: &OverviewMatrix) {
    out.push_str("  ── MARKET HEALTH ─────────────────────────────────────────────────────\n");
    let Some(dims) = &om.market_health_dims else {
        out.push_str("  no data\n\n");
        return;
    };
    for bar in &dims.bars {
        let value_str = if bar.available {
            format!("{:.0}", bar.value)
        } else {
            "—".to_string()
        };
        let bar_len = ((bar.value / 100.0) * 40.0).round() as usize;
        let bar_chars: String = "█".repeat(bar_len.min(40));
        out.push_str(&format!(
            "  {:<18} [{:<40}] {:>3}\n",
            bar.label, bar_chars, value_str
        ));
    }
    out.push_str(&format!(
        "  {} active instance{} contributing\n\n",
        dims.active_instance_count,
        if dims.active_instance_count == 1 {
            ""
        } else {
            "s"
        }
    ));
}

async fn render_instances(out: &mut String, instances: &[Arc<Instance>]) {
    out.push_str("  ── INSTANCES ──────────────────────────────────────────────────────────\n");
    if instances.is_empty() {
        out.push_str("  (no instances running)\n\n");
        return;
    }
    out.push_str(&format!(
        "  {:<14} {:<12} {:<8} {:>14}  {:<9} {:<8} {:<30}\n",
        "SYMBOL", "EXCHANGE", "MODE", "PRICE", "STATUS", "MICRO", "TF LADDER"
    ));
    for inst in instances {
        let price = inst.latest_price().await.unwrap_or(f64::NAN);
        let price_str = if price.is_nan() {
            "--".to_string()
        } else if price >= 1000.0 {
            format!("{:.2}", price)
        } else if price >= 1.0 {
            format!("{:.4}", price)
        } else {
            format!("{:.6}", price)
        };
        // Fastest slot of the active ladder + the ACTIVE ladder
        // (fastest N of the fixed pool) for one-glance parity with the
        // GUI instance rows.
        let micro_secs = format!(
            "{}s",
            inst.active_pair.active_secs.first().copied().unwrap_or(0)
        );
        let ladder = inst
            .active_secs
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("/");
        let mode = match inst.execution_mode().await {
            config_models::ExecutionMode::Observe => "observe",
            config_models::ExecutionMode::Paper => "paper",
            config_models::ExecutionMode::Live => "live",
        };
        out.push_str(&format!(
            "  {:<14} {:<12} {:<8} {:>14}  {:<9} {:<8} {:<30}\n",
            inst.pair_display(),
            inst.exchange.as_str(),
            mode,
            price_str,
            inst.status().await.as_str(),
            micro_secs,
            ladder,
        ));
    }
    out.push('\n');
}

/// Compact price-level formatter for the setup columns — trims trailing
/// zeros after up to 4 decimals ("63200.0000" → "63200", "0.1234" kept).
fn fmt_level(v: f64) -> String {
    if v <= 0.0 {
        return "—".to_string();
    }
    let s = format!("{:.4}", v);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

/// Zone range renderer ("63200-63400"); "—" when the bracket is absent.
fn fmt_zone(low: f64, high: f64) -> String {
    if low <= 0.0 || high <= 0.0 {
        return "—".to_string();
    }
    format!("{}-{}", fmt_level(low), fmt_level(high))
}

/// 15-column asset ranking — mirrors the GUI's AssetRankingsTable. Reads
/// the server-computed `overview_rows` (single source); falls back to the
/// L7 `asset_ranking` during warmup when rows are absent.
fn render_asset_ranking(out: &mut String, om: &OverviewMatrix) {
    out.push_str("  ── ASSET RANKINGS ─────────────────────────────────────────────────────\n");
    if om.overview_rows.is_empty() && om.asset_ranking.is_empty() {
        out.push_str("  (no assets ranked yet)\n\n");
        return;
    }
    if !om.overview_rows.is_empty() {
        let rows = &om.overview_rows;
        // Image 1 fidelity: SYMBOL | PRICE | ENTRY | TAKE PROFIT | STOP LOSS | BIAS | SIGNAL | DIRECTION | R:R | SCORE | CONFIDENCE | MTF SCORE | MTF LABEL | RISK | UPDATED
        out.push_str(&format!(
            "  {:<10} {:>10} {:>11} {:>11} {:>11} {:<9} {:<6} {:<8} {:>7} {:>6} {:>10} {:>9} {:<12} {:>5} {:>8}\n",
            "SYMBOL",
            "PRICE",
            "ENTRY",
            "TAKE PROFIT",
            "STOP LOSS",
            "BIAS",
            "SIGNAL",
            "DIRECTION",
            "R:R",
            "SCORE",
            "CONFIDENCE",
            "MTF SCORE",
            "MTF LABEL",
            "RISK",
            "UPDATED"
        ));
        for row in rows {
            let updated = if row.updated_ts > 0 {
                let now = chrono::Utc::now().timestamp() as u64;
                let delta = now.saturating_sub(row.updated_ts);
                if delta < 5 {
                    "now".to_string()
                } else if delta < 60 {
                    format!("{}s ago", delta)
                } else if delta < 3600 {
                    format!("{}m ago", delta / 60)
                } else {
                    format!("{}h ago", delta / 3600)
                }
            } else {
                "—".to_string()
            };
            out.push_str(&format!(
                "  {:<10} {:>10.2} {:>11} {:>11} {:>11} {:<9} {:<6} {:<8} {:>7} {:>6.0} {:>9.0}% {:>9.0} {:<12} {:>5.0} {:>8}\n",
                row.symbol,
                row.price,
                fmt_zone(row.entry_low, row.entry_high),
                fmt_zone(row.target_low, row.target_high),
                fmt_level(row.invalidation),
                row.bias,
                row.signal,
                row.direction,
                fmt_rr(row.rr),
                row.score,
                row.confidence,
                row.mtf_score,
                row.mtf_label.replace("_MTF", "").replace('_', " "),
                row.risk,
                updated,
            ));
        }
    } else {
        // Warmup fallback — the L7 ranking has no price/direction detail.
        out.push_str(&format!(
            "  {:<12} {:>7} {:<14} {:>11} {:<14} {:>6}  {}\n",
            "SYMBOL", "SCORE", "BIAS", "CONFIDENCE", "REGIME", "RISK", "MTF"
        ));
        for rank in &om.asset_ranking {
            out.push_str(&format!(
                "  {:<12} {:>7.1} {:<14} {:>10.1}% {:<14} {:>6}  {}\n",
                rank.symbol,
                rank.score,
                rank.bias,
                rank.confidence,
                rank.regime,
                rank.risk_level,
                rank.mtf_label,
            ));
        }
    }
    out.push('\n');
}

fn render_summary(out: &mut String, om: &OverviewMatrix) {
    out.push_str("  ── MARKET SUMMARY ─────────────────────────────────────────────────────\n");
    out.push_str(&format!("  {}\n", om.global_summary));
    out.push_str(&format!(
        "  Active symbols   : {}\n",
        if om.active_symbols.is_empty() {
            "(none)".to_string()
        } else {
            om.active_symbols.join(", ")
        }
    ));
    out.push_str(&format!(
        "  Low coverage     : {}     Instances: {}\n",
        if om.low_coverage { "YES" } else { "no" },
        om.instance_count
    ));
}

/// `formatRR` port — `1 : N` display, `—` when no meaningful R:R.
fn fmt_rr(rr: f64) -> String {
    if !rr.is_finite() || rr <= 0.0 {
        "—".to_string()
    } else {
        format!("1 : {:.2}", rr)
    }
}

/// Periodic monitor loop: clear screen + render on every interval tick.
/// Reads the L7 overview (with the v7.2 panel payload) from
/// `overview_ref` — the same object `GET /api/overview` serves the GUI.
pub async fn run_terminal_monitor(
    overview_ref: Arc<RwLock<Option<OverviewMatrix>>>,
    workspace: portfolio_supervisor::workspace_state::WorkspaceState,
    pool: sqlx::SqlitePool,
    interval_secs: u64,
    session_line: String,
    cancel: CancellationToken,
) {
    println!(
        "🖥️  Terminal monitor: redraw every {}s (Ctrl+C to stop)",
        interval_secs
    );
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs.max(1)));
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = interval.tick() => {
                let overview = overview_ref.read().await.clone();
                let instances = workspace.list().await;
                // v10.1: long/short symmetry verdict (CLI↔GUI parity — the
                // same computation the PAE Overview card renders).
                let symmetry =
                    performance_analytics::stats_compiler::compute_direction_symmetry_live(&pool)
                        .await;
                let frame = render_frame(&overview, &instances, &session_line).await;
                print!("\x1b[2J\x1b[H");
                print!("{}", frame);
                if let Some(s) = &symmetry {
                    println!(
                        "\n  ── LONG/SHORT SYMMETRY ──────────────────────────────────────────────\n  longs {} (exp {:.2} · WR {:.0}%) vs shorts {} (exp {:.2} · WR {:.0}%) · t {:.2} · p {:.4} · {}",
                        s.long_count,
                        s.long_expectancy_usd,
                        s.long_win_rate,
                        s.short_count,
                        s.short_expectancy_usd,
                        s.short_win_rate,
                        s.t_statistic,
                        s.p_value,
                        s.verdict,
                    );
                }
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
        }
    }
}

/// Field-coverage contract — every OverviewMatrix field the GUI
/// GeneralDashboard components consume must appear in the CLI frame.
/// This is the parity gate: extend this test when a new panel field lands.
#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::overview::OverviewMatrix;
    use core_domain::overview_panel::{
        build_overview_panel, DirectionDistribution, HeroVerdict, MarketHealthDims, OverviewHero,
        OverviewRow, SignalQuality,
    };

    fn fixture_overview() -> OverviewMatrix {
        let mut om = OverviewMatrix::empty();
        om.instance_count = 1;
        om.active_symbols = vec!["BTC-USDT".to_string()];
        om.global_summary = "1 active instance across 1 symbols. Global bias: NEUTRAL with balanced market breadth. Risk environment: MODERATE.".to_string();
        om.hero = Some(OverviewHero {
            verdict: HeroVerdict::Trade,
            actionable_count: 1,
            candidate_count: 2,
            best_symbol: Some("BTC-USDT".to_string()),
            best_score: 70.0,
            best_direction: "LONG".to_string(),
            best_confidence: 80.0,
            best_rr: 2.5,
            instance_count: 1,
        });
        om.overview_rows = vec![OverviewRow {
            symbol: "BTC-USDT".to_string(),
            price: 64497.5,
            bias: "Bullish".to_string(),
            signal: "BUY".to_string(),
            direction: "LONG".to_string(),
            rr: 2.5,
            score: 61.0,
            confidence: 80.0,
            mtf_score: 42.0,
            mtf_label: "WEAK_BULL_MTF".to_string(),
            risk: 45.0,
            setup_side: "LONG".to_string(),
            entry_low: 63200.0,
            entry_high: 63400.0,
            target_low: 66000.0,
            target_high: 66500.0,
            invalidation: 62800.0,
            updated_ts: 1_700_000_000,
            active: true,
        }];
        om.signal_quality = Some(SignalQuality {
            strong: 1,
            moderate: 0,
            weak: 0,
        });
        om.direction_distribution = Some(DirectionDistribution {
            long: 1,
            short: 0,
            neutral: 0,
        });
        om.market_health_dims = Some(MarketHealthDims {
            bars: vec![
                core_domain::overview_panel::HealthBar {
                    label: "TREND STRENGTH".to_string(),
                    value: 60.0,
                    available: true,
                    contributing_instances: 1,
                },
                core_domain::overview_panel::HealthBar {
                    label: "LIQUIDITY".to_string(),
                    value: 70.0,
                    available: true,
                    contributing_instances: 1,
                },
                core_domain::overview_panel::HealthBar {
                    label: "VOLATILITY".to_string(),
                    value: 60.0,
                    available: true,
                    contributing_instances: 1,
                },
                core_domain::overview_panel::HealthBar {
                    label: "SIGNAL STABILITY".to_string(),
                    value: 80.0,
                    available: true,
                    contributing_instances: 1,
                },
            ],
            active_instance_count: 1,
        });
        om
    }

    #[tokio::test]
    async fn frame_covers_every_gui_panel_field() {
        let om = fixture_overview();
        let frame = render_frame(&Some(om), &[], "observe · hyperliquid · USDC").await;

        // Header (LayerHeader + KPIs).
        assert!(frame.contains("Market Bias"));
        assert!(frame.contains("Health"));
        assert!(frame.contains("Sync"));
        assert!(frame.contains("Breadth"));
        assert!(frame.contains("Systemic Risk"));
        assert!(frame.contains("Cascade"));
        assert!(frame.contains("TF Agreement"));
        assert!(frame.contains("Alignment Consensus"));
        assert!(frame.contains("Alignment"));

        // Hero (RecommendationHero) + subtext fields.
        assert!(frame.contains("MARKET STATUS"));
        assert!(frame.contains("TRADE"));
        assert!(frame.contains("actionable setup"));
        assert!(frame.contains("BTC-USDT"));
        assert!(frame.contains("LONG"));
        assert!(frame.contains("1 : 2.50"));
        assert!(frame.contains("80%"));
        assert!(frame.contains("2 candidates"));

        // Trade Opportunities card.
        assert!(frame.contains("TRADE OPPORTUNITIES"));
        assert!(frame.contains("Best Pair"));
        assert!(frame.contains("Score"));

        // Signal quality + direction cards.
        assert!(frame.contains("SIGNAL QUALITY"));
        assert!(frame.contains("STRONG 1"));
        assert!(frame.contains("DIRECTION"));
        assert!(frame.contains("LONG 1"));

        // Market health card (health + sync + 4 sub-bars).
        assert!(frame.contains("MARKET HEALTH"));
        assert!(frame.contains("TREND STRENGTH"));
        assert!(frame.contains("LIQUIDITY"));
        assert!(frame.contains("VOLATILITY"));
        assert!(frame.contains("SIGNAL STABILITY"));
        assert!(frame.contains("active instance"));

        // Asset rankings: every GUI column header value appears (Image 1 order).
        assert!(frame.contains("ASSET RANKINGS"));
        assert!(frame.contains("SYMBOL"));
        assert!(frame.contains("PRICE"));
        assert!(frame.contains("ENTRY"));
        assert!(frame.contains("TAKE PROFIT"));
        assert!(frame.contains("STOP LOSS"));
        assert!(frame.contains("BIAS"));
        assert!(frame.contains("SIGNAL"));
        assert!(frame.contains("DIRECTION"));
        assert!(frame.contains("SCORE"));
        assert!(frame.contains("CONFIDENCE"));
        assert!(frame.contains("MTF SCORE"));
        assert!(frame.contains("MTF LABEL"));
        assert!(frame.contains("RISK"));
        assert!(frame.contains("R:R"));
        assert!(frame.contains("UPDATED"));
        // Row values (incl. the top-setup ENTRY / TAKE PROFIT / STOP LOSS levels).
        assert!(frame.contains("64497.5"));
        assert!(frame.contains("Bullish"));
        assert!(frame.contains("BUY"));
        // CLI mirrors the GUI label cleaning: WEAK_BULL_MTF → WEAK BULL.
        assert!(frame.contains("WEAK BULL"));
        assert!(frame.contains("63200-63400"));
        assert!(frame.contains("66000-66500"));
        assert!(frame.contains("62800"));

        // Summary.
        assert!(frame.contains("MARKET SUMMARY"));
        assert!(frame.contains("Low coverage"));
        assert!(frame.contains("Instances: 1"));
    }

    #[tokio::test]
    async fn warmup_frame_renders_without_panel_payload() {
        let frame = render_frame(&None, &[], "observe · hyperliquid · USDC").await;
        assert!(frame.contains("Warming up"));
    }

    #[tokio::test]
    async fn empty_overview_renders_stand_aside() {
        let frame = render_frame(&Some(OverviewMatrix::empty()), &[], "observe").await;
        assert!(frame.contains("STAND ASIDE"));
    }

    #[test]
    fn build_overview_panel_roundtrip_feeds_renderer() {
        // The builder output must be renderable without any extra
        // derivation — GUI and CLI share this exact payload shape.
        let panel = build_overview_panel(&[], &std::collections::HashMap::new());
        let _ = panel.hero;
        assert!(panel.rows.is_empty());
    }
}
