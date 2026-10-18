//! The command surface (P6 W6.1).
//!
//! Every command reads the **same `AppState` the WebSocket handler serves**.
//! That is the whole parity argument: there is no second query path to keep in
//! step, so "the shell disagrees with the dashboard" is not a class of bug this
//! codebase can express — only a rendering difference, which `shell_ws_parity.rs`
//! checks byte-for-byte on the fields it shares.
//!
//! ## What a command may not do
//!
//! Say something actionable. The shell prints descriptions of what the candles
//! are doing and never what to do about it: no entry, no stop, no target, no
//! verdict, no risk/reward (CR-15, `no strategy vocabulary`). `V-75` greps this
//! file's output for exactly those words.

use std::sync::Arc;

use api_gateway::AppState;
use network_adapters::connection_quality_tracker::QualityWindow;
use serde_json::{json, Value};

use super::manifest::Status;
use super::parser::{Command, DataCommand, ModelCommand, TfSelector};

/// What a command produced: the machine value and the human rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The canonical document (`--json` prints this verbatim).
    pub value: Value,
    /// The psql-style rendering.
    pub human: String,
}

impl Outcome {
    fn new(value: Value, human: impl Into<String>) -> Self {
        Outcome {
            value,
            human: human.into(),
        }
    }

    /// A refusal. Not an error state of the process — a refusal to answer.
    fn refused(message: impl Into<String>) -> Self {
        let message = message.into();
        Outcome {
            value: json!({ "error": message }),
            human: format!("  {message}"),
        }
    }
}

/// Run one command against the live state.
pub async fn execute(state: &Arc<AppState>, command: &Command) -> Outcome {
    match command {
        Command::Help => Outcome::new(
            json!({ "commands": super::parser::TOP_LEVEL }),
            super::print_help(),
        ),
        Command::Version => Outcome::new(
            json!({
                "corpus": env!("CARGO_PKG_VERSION"),
                "build": option_env!("GIT_BUILD_ID").unwrap_or("dev"),
                "mode": "observe",
            }),
            super::version_line(),
        ),
        Command::Exit => Outcome::new(json!({ "exit": true }), ""),
        Command::ConfigShow => Outcome::new(
            json!({ "status": "declared", "phase": "R1", "note":
                "one owner per key lands with the five config files (CR-23)" }),
            "  `config show` lands with R1 — the config files are still the monolith.",
        ),
        Command::ConfigSet { engine, path, .. } => Outcome::new(
            json!({ "error": format!("config set {engine} {path} — declared, lands with R1") }),
            "  `config set` lands with R1. Nothing was changed.",
        ),
        Command::Data(d) => data(state, d).await,
        Command::Model(m) => model(state, m).await,
    }
}

// ── Data Engine ────────────────────────────────────────────────────────────

async fn data(state: &Arc<AppState>, cmd: &DataCommand) -> Outcome {
    match cmd {
        DataCommand::Status => Outcome::new(
            json!({
                "corpus": env!("CARGO_PKG_VERSION"),
                "build": option_env!("GIT_BUILD_ID").unwrap_or("dev"),
                "mode": "observe",
                "session_id": *state.session_id.read().await,
                "instances": state.workspace.len().await,
                "ws_clients": api_gateway::ws::connected_client_count(),
            }),
            format!(
                "  corpus {} · build {} · mode observe\n  session {} · {} instance(s) · {} ws client(s)",
                env!("CARGO_PKG_VERSION"),
                option_env!("GIT_BUILD_ID").unwrap_or("dev"),
                state
                    .session_id
                    .read()
                    .await
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".into()),
                state.workspace.len().await,
                api_gateway::ws::connected_client_count(),
            ),
        ),
        DataCommand::Instances => {
            let rows = running_instances(state).await;
            let mut out = String::new();
            out.push_str(&format!(
                "  {:<14} {:<12} {:<8} {:<8} {}\n",
                "PAIR", "STATUS", "TFS", "SNAP", "DURATIONS"
            ));
            for r in &rows {
                out.push_str(&format!(
                    "  {:<14} {:<12} {:<8} {:<8} {}\n",
                    r["pair"], r["status"], r["durations"], r["snapshots"], r["ladder"]
                ));
            }
            if rows.is_empty() {
                out.push_str("  (no running instance)\n");
            }
            Outcome::new(json!({ "instances": rows }), out)
        }
        DataCommand::Overview => {
            let instances = running_instances(state).await;
            let per_instance: Vec<Value> = instances
                .iter()
                .map(|i| {
                    json!({
                        "pair": i["pair"],
                        "durations": i["durations"],
                        "snapshots": i["snapshots"],
                    })
                })
                .collect();
            Outcome::new(
                json!({ "instances": per_instance, "count": instances.len() }),
                format!("  {} running instance(s)\n", instances.len()),
            )
        }
        DataCommand::Exchange => {
            let report = state.exchange_status.report().await;
            let mut out = String::new();
            out.push_str(&format!(
                "  {:<12} {:<14} {:<8} {:<10} {}\n",
                "EXCHANGE", "STATE", "PAIRS", "RECONNECTS", "LAST HEARTBEAT"
            ));
            for s in &report.exchanges {
                out.push_str(&format!(
                    "  {:<12} {:<14} {:<8} {:<10} {}\n",
                    s.name, format!("{:?}", s.state), s.active_pairs, s.total_reconnects, s.last_heartbeat_ms
                ));
            }
            if report.exchanges.is_empty() {
                out.push_str("  (no venue registered)\n");
            }
            Outcome::new(
                serde_json::to_value(&report).unwrap_or(Value::Null),
                out,
            )
        }
        DataCommand::Connectivity => {
            let now_ms = now_ms();
            let reports = state
                .connection_quality
                .all_reports(QualityWindow::OneHour, now_ms)
                .await;
            let mut out = String::new();
            out.push_str(&format!(
                "  {:<14} {:>6}  {:>8} {:>11} {:>9} {:>6}\n",
                "PAIR", "TF", "UPTIME%", "RECONN ms", "DROPS", "SCORE"
            ));
            for (pair, secs, r) in &reports {
                out.push_str(&format!(
                    "  {:<14} {:>6}  {:>8.2} {:>11.0} {:>9} {:>6.1}\n",
                    pair, secs, r.uptime_pct, r.avg_reconnect_ms, r.disconnect_count, r.score
                ));
            }
            if reports.is_empty() {
                out.push_str("  (no quality samples yet)\n");
            }
            Outcome::new(
                json!({
                    "window": "one_hour",
                    "scopes": reports
                        .iter()
                        .map(|(p, s, r)| json!({
                            "pair": p,
                            "timeframe_secs": s,
                            "uptime_pct": r.uptime_pct,
                            "disconnect_count": r.disconnect_count,
                            "avg_reconnect_ms": r.avg_reconnect_ms,
                            "score": r.score,
                        }))
                        .collect::<Vec<_>>(),
                }),
                out,
            )
        }
        DataCommand::MarketData => {
            let instances = running_instances(state).await;
            let mut out = String::new();
            out.push_str(&format!(
                "  {:<14} {:<8} {:<12} {}\n",
                "PAIR", "TFS", "MID", "LAST BAR"
            ));
            for r in &instances {
                out.push_str(&format!(
                    "  {:<14} {:<8} {:<12} {}\n",
                    r["pair"], r["durations"], r["mid"], r["last_bar"]
                ));
            }
            Outcome::new(json!({ "pairs": instances }), out)
        }
        DataCommand::Ntp => match &state.clock_monitor {
            None => Outcome::refused("the clock monitor is not running"),
            Some(monitor) => {
                let offset = monitor.current_offset_us();
                let jitter = monitor.rms_jitter_us();
                let breaches = monitor.breach_count();
                Outcome::new(
                    json!({
                        "offset_us": offset,
                        "rms_jitter_us": jitter,
                        "samples": monitor.sample_count(),
                        "breach_count": breaches,
                        "consecutive_breaches": monitor.consecutive_breaches(),
                    }),
                    format!(
                        "  offset {} µs · jitter {} µs · {} samples · {} breach(es)",
                        offset.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                        jitter.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
                        monitor.sample_count(),
                        breaches
                    ),
                )
            }
        },
        DataCommand::DataQuality => {
            let instances = running_instances(state).await;
            let states: Vec<Value> = instances
                .iter()
                .flat_map(|i| {
                    i["pipelines"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                })
                .collect();
            Outcome::new(
                json!({ "pipelines": states }),
                format!("  {} pipeline(s) reporting\n", states.len()),
            )
        }
        DataCommand::Distribution => Outcome::new(
            json!({
                "ws_clients": api_gateway::ws::connected_client_count(),
                "note": "L4 egress is per-channel; the client count is the live figure the UI reads",
            }),
            format!("  {} ws client(s) subscribed", api_gateway::ws::connected_client_count()),
        ),
        DataCommand::ConnectionSettings => {
            let platform = state.platform.read().await;
            let value = serde_json::to_value(&*platform).unwrap_or(Value::Null);
            Outcome::new(
                value.clone(),
                format!(
                    "  [workspace.api_failover] and the clock monitor are declared (CR-23); \
                     showing the platform config as parsed.\n  {}",
                    serde_json::to_string(&value).unwrap_or_default()
                ),
            )
        }
        DataCommand::Candles { pair, tf, limit } => candles(state, pair, *tf, *limit).await,
    }
}

async fn candles(state: &Arc<AppState>, pair: &str, tf: TfSelector, limit: usize) -> Outcome {
    let Some(active) = state.get_active_pair(pair).await else {
        return Outcome::refused(format!("no running instance for `{pair}`"));
    };
    let wanted: Vec<u64> = match tf {
        TfSelector::All => active.active_secs.clone(),
        TfSelector::One(s) => vec![s],
    };
    let mut rows = Vec::new();
    let mut human = String::new();
    for secs in &wanted {
        let Some(pipeline) = active.pipeline_for_secs(*secs) else {
            human.push_str(&format!(
                "  {secs:>6}s  — not an ACTIVE duration for {pair}\n"
            ));
            continue;
        };
        let history = pipeline.history.read().await;
        let tail: Vec<Value> = history
            .iter()
            .rev()
            .take(limit)
            .map(|c| {
                json!({
                    "start_time_ms": c.start_time_ms,
                    "open": c.open.to_string(),
                    "high": c.high.to_string(),
                    "low": c.low.to_string(),
                    "close": c.close.to_string(),
                    "volume": c.volume.to_string(),
                    "reconstructed": c.reconstructed.is_some(),
                })
            })
            .collect();
        let mut ordered = tail;
        ordered.reverse();
        human.push_str(&format!(
            "  {} {}\n    {:>12} {:>12} {:>12} {:>12} {:>14}\n",
            pipeline.slot_label,
            if ordered.is_empty() {
                "(no candles)".to_string()
            } else {
                format!("{} rows", ordered.len())
            },
            "open",
            "high",
            "low",
            "close",
            "volume"
        ));
        for c in &ordered {
            human.push_str(&format!(
                "    {:>12} {:>12} {:>12} {:>12} {:>14}\n",
                c["open"], c["high"], c["low"], c["close"], c["volume"]
            ));
        }
        rows.push(json!({
            "timeframe_secs": secs,
            "timeframe_label": pipeline.slot_label,
            "candles": ordered,
        }));
    }
    Outcome::new(json!({ "pair": pair, "durations": rows }), human)
}

/// The instance rows both `data instances` and the model commands share.
async fn running_instances(state: &Arc<AppState>) -> Vec<Value> {
    let mut out = Vec::new();
    for instance in state.workspace.list().await {
        let pair = format!("{}-{}", instance.pair.0, instance.pair.1);
        let active = instance.active_pair.clone();
        if active.pipelines.is_empty() {
            continue;
        }
        let mut pipelines: Vec<Value> = Vec::new();
        for p in &active.pipelines {
            pipelines.push(json!({
                "timeframe_secs": p.timeframe_secs,
                "timeframe_label": p.slot_label,
                "pipeline_state": format!("{:?}", *p.pipeline_state.read().await),
            }));
        }
        let mut snapshots: Vec<(u64, Option<String>)> = Vec::new();
        for p in &active.pipelines {
            let mid = p
                .latest_snapshot
                .read()
                .await
                .as_ref()
                .map(|s| s.mid_price.to_string());
            snapshots.push((p.timeframe_secs, mid));
        }
        let with_snapshot = snapshots.iter().filter(|(_, m)| m.is_some()).count();
        let fastest_mid = snapshots
            .iter()
            .find(|(_, m)| m.is_some())
            .and_then(|(_, m)| m.clone());
        let last_bar = snapshots
            .iter()
            .filter_map(|(_, m)| m.clone())
            .next()
            .unwrap_or_else(|| "-".into());
        out.push(json!({
            "pair": pair,
            "status": "running",
            "durations": active.active_secs.len(),
            "snapshots": with_snapshot,
            "ladder": active.active_secs.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","),
            "mid": fastest_mid.unwrap_or_else(|| "-".into()),
            "last_bar": last_bar,
            "pipelines": pipelines,
        }));
    }
    out.sort_by(|a, b| a["pair"].as_str().cmp(&b["pair"].as_str()));
    out
}

// ── Model Engine ───────────────────────────────────────────────────────────

async fn model(state: &Arc<AppState>, cmd: &ModelCommand) -> Outcome {
    match cmd {
        ModelCommand::Instances => {
            let rows = running_instances(state).await;
            Outcome::new(
                json!({ "instances": rows }),
                format!("  {} model instance(s)\n", rows.len()),
            )
        }
        ModelCommand::Overview => match &*state.overview.read().await {
            None => Outcome::new(
                json!({ "cross_pair_state": null, "note": "not yet aggregated" }),
                "  the cross-pair reduction has not been published yet",
            ),
            Some(overview) => Outcome::new(
                serde_json::to_value(overview).unwrap_or(Value::Null),
                serde_json::to_string_pretty(overview).unwrap_or_default(),
            ),
        },
        ModelCommand::State { pair, tf, .. } => {
            let value = model_state_value(state, pair, *tf).await;
            Outcome::new(value.clone(), render_state(&value))
        }
        ModelCommand::Alignment { pair } => match latest_snapshot(state, pair, None).await {
            None => Outcome::refused(format!("no snapshot yet for `{pair}`")),
            Some(s) => match &s.alignment {
                None => Outcome::new(
                    json!({ "pair": pair, "alignment": null }),
                    "  this duration has published no alignment",
                ),
                Some(a) => Outcome::new(
                    serde_json::to_value(a).unwrap_or(Value::Null),
                    serde_json::to_string_pretty(a).unwrap_or_default(),
                ),
            },
        },
        ModelCommand::Consensus { pair } => match latest_snapshot(state, pair, None).await {
            None => Outcome::refused(format!("no snapshot yet for `{pair}`")),
            Some(s) => {
                let mut out = String::new();
                out.push_str(&format!(
                    "  {} @ {}\n",
                    s.symbol,
                    s.timeframe_label.clone().unwrap_or_default()
                ));
                out.push_str(&format!(
                    "  indicators: {}   lifecycle entries: {}\n",
                    s.indicators.len(),
                    s.indicator_lifecycle.len()
                ));
                Outcome::new(
                    json!({ "symbol": s.symbol, "indicators": s.indicators.len() }),
                    out,
                )
            }
        },
        ModelCommand::Metrics { pair, tf, .. } => metrics(state, pair, *tf).await,
        ModelCommand::Indicator {
            key,
            pair,
            tf,
            limit,
        } => indicator(state, pair, *tf, key, *limit).await,
        ModelCommand::Signals { pair, tf, kind } => {
            signals(state, pair, *tf, kind.as_deref()).await
        }
        ModelCommand::Divergences { pair, tf } => divergences(state, pair, *tf).await,
        ModelCommand::Levels { pair, tf } => levels(state, pair, *tf).await,
        // The pair is validated by the parser; the data does not exist yet.
        ModelCommand::Leg { .. } => declared("model leg", "P8"),
        ModelCommand::Events { .. } => declared("model events", "P9"),
        ModelCommand::Diagnostics { .. } => declared("model diagnostics", "P10"),
    }
}

fn declared(command: &str, phase: &str) -> Outcome {
    Outcome::new(
        json!({ "status": "declared", "command": command, "phase": phase }),
        format!("  `{command}` is declared in the parity manifest and lands in {phase}."),
    )
}

/// The latest snapshot for `(pair, duration)`. `None` duration = the fastest.
async fn latest_snapshot(
    state: &Arc<AppState>,
    pair: &str,
    tf: Option<u64>,
) -> Option<core_domain::models::MarketSnapshot> {
    snapshot_at(state, pair, tf).await
}

/// `model state` — the snapshot the WebSocket handler would broadcast.
async fn model_state_value(state: &Arc<AppState>, pair: &str, tf: TfSelector) -> Value {
    let Some(active) = state.get_active_pair(pair).await else {
        return json!({ "error": format!("no running instance for `{pair}`") });
    };
    let wanted: Vec<u64> = match tf {
        TfSelector::All => active.active_secs.clone(),
        TfSelector::One(s) => vec![s],
    };
    let mut states = Vec::new();
    for secs in wanted {
        let Some(pipeline) = active.pipeline_for_secs(secs) else {
            continue;
        };
        match pipeline.latest_snapshot.read().await.as_ref() {
            None => states.push(json!({
                "timeframe_secs": secs,
                "timeframe_label": pipeline.slot_label,
                "state": "initializing",
            })),
            Some(snapshot) => states.push(json!({
                "timeframe_secs": secs,
                "timeframe_label": snapshot.timeframe_label,
                "available_at": snapshot.timestamp,
                "symbol": snapshot.symbol,
                "mid_price": snapshot.mid_price.to_string(),
                "pipeline_state": format!("{:?}", snapshot.pipeline_state),
                "indicators": snapshot.indicators.len(),
            })),
        }
    }
    json!({ "pair": pair, "timeframes": states })
}

/// The psql-style rendering of `model state`.
fn render_state(value: &Value) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "  {:<8} {:<10} {:<10} {:<16} {:<10} {}\n",
        "TF", "AVAILABLE", "MID", "PIPELINE", "IND", "FIRST SEEN"
    ));
    if let Some(rows) = value["timeframes"].as_array() {
        for r in rows {
            out.push_str(&format!(
                "  {:<8} {:<10} {:<10} {:<16} {:<10} {}\n",
                r["timeframe_label"],
                r.get("available_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-"),
                r.get("mid_price").and_then(|v| v.as_str()).unwrap_or("-"),
                r["pipeline_state"].as_str().unwrap_or("initializing"),
                r.get("indicators")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".into()),
                r["symbol"].as_str().unwrap_or("-"),
            ));
        }
    }
    if let Some(err) = value["error"].as_str() {
        out.push_str(&format!("  {err}\n"));
    }
    out
}

async fn metrics(state: &Arc<AppState>, pair: &str, tf: u64) -> Outcome {
    let Some(active) = state.get_active_pair(pair).await else {
        return Outcome::refused(format!("no running instance for `{pair}`"));
    };
    let Some(pipeline) = active.pipeline_for_secs(tf) else {
        return Outcome::refused(format!("`{tf}` is not an ACTIVE duration for {pair}"));
    };
    let Some(snapshot) = pipeline.latest_snapshot.blocking_read().clone() else {
        return Outcome::refused(format!("{pair} @ {tf}s has published no snapshot yet"));
    };
    let mut keys: Vec<&String> = snapshot.indicators.keys().collect();
    keys.sort();
    let mut out = String::new();
    out.push_str(&format!("  {pair} @ {tf}s — {} indicators\n", keys.len()));
    out.push_str(&format!(
        "  {:<24} {:<12} {:<16} {}\n",
        "INDICATOR", "NORMALISED", "STATE", "SIGNALS"
    ));
    for k in keys {
        let v = &snapshot.indicators[k];
        let kinds: Vec<String> = v.signals.iter().map(|s| format!("{:?}", s.kind)).collect();
        out.push_str(&format!(
            "  {:<24} {:<12.4} {:<16} {}\n",
            k,
            v.normalized,
            v.state_label,
            kinds.join(",")
        ));
    }
    Outcome::new(
        serde_json::to_value(&snapshot.indicators).unwrap_or(Value::Null),
        out,
    )
}

async fn indicator(state: &Arc<AppState>, pair: &str, tf: u64, key: &str, limit: usize) -> Outcome {
    let snapshot = snapshot_at(state, pair, Some(tf)).await;
    let Some(snapshot) = snapshot else {
        return match state.get_active_pair(pair).await {
            None => Outcome::refused(format!("no running instance for `{pair}`")),
            Some(active) if active.pipeline_for_secs(tf).is_none() => {
                Outcome::refused(format!("`{tf}` is not an ACTIVE duration for {pair}"))
            }
            _ => Outcome::refused(format!("{pair} @ {tf}s has published no snapshot yet")),
        };
    };
    let Some(value) = snapshot.indicators.get(key) else {
        let mut keys: Vec<&String> = snapshot.indicators.keys().collect();
        keys.sort();
        return Outcome::refused(format!(
            "`{key}` is not a live indicator here. Known: {}",
            keys.iter()
                .take(12)
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let history = match state
        .get_active_pair(pair)
        .await
        .as_ref()
        .and_then(|a| a.pipeline_for_secs(tf))
    {
        Some(p) => p.snapshot_history.read().await.len(),
        None => 0,
    };
    let mut out = format!(
        "  {pair} @ {tf}s — {key}\n  normalised {:.4} · raw {:.4} · state {}\n  retention: {history} snapshot(s) available\n",
        value.normalized, value.raw_value, value.state_label
    );
    out.push_str(&format!(
        "  (at most {limit} point(s) are retained — the 500-candle cap, 01-16 T1)\n"
    ));
    Outcome::new(serde_json::to_value(value).unwrap_or(Value::Null), out)
}

async fn signals(
    state: &Arc<AppState>,
    pair: &str,
    tf: Option<u64>,
    kind: Option<&str>,
) -> Outcome {
    let snapshot = match snapshot_at(state, pair, tf).await {
        Some(s) => s,
        None => return Outcome::refused(format!("no snapshot yet for `{pair}`")),
    };
    let mut rows: Vec<Value> = Vec::new();
    let mut keys: Vec<&String> = snapshot.indicators.keys().collect();
    keys.sort();
    for k in keys {
        let v = &snapshot.indicators[k];
        for s in &v.signals {
            let name = format!("{:?}", s.kind);
            if let Some(filter) = kind {
                if !name.eq_ignore_ascii_case(filter) {
                    continue;
                }
            }
            rows.push(json!({
                "indicator": k,
                "kind": name,
                "detail": serde_json::to_value(s).unwrap_or(Value::Null),
            }));
        }
    }
    let mut out = format!("  {pair} — {} signal(s)\n", rows.len());
    for r in &rows {
        out.push_str(&format!("  {:<24} {}\n", r["indicator"], r["kind"]));
    }
    Outcome::new(json!({ "signals": rows }), out)
}

async fn divergences(state: &Arc<AppState>, pair: &str, tf: Option<u64>) -> Outcome {
    let snapshot = match snapshot_at(state, pair, tf).await {
        Some(s) => s,
        None => return Outcome::refused(format!("no snapshot yet for `{pair}`")),
    };
    let mut rows = Vec::new();
    let mut keys: Vec<&String> = snapshot.indicators.keys().collect();
    keys.sort();
    for k in keys {
        let v = &snapshot.indicators[k];
        for s in &v.signals {
            if format!("{:?}", s.kind) == "Divergence" {
                rows.push(json!({ "indicator": k, "signal": s }));
            }
        }
    }
    Outcome::new(
        json!({ "divergences": rows.clone() }),
        format!("  {pair} — {} divergence(s)\n", rows.len()),
    )
}

async fn levels(state: &Arc<AppState>, pair: &str, tf: u64) -> Outcome {
    let snapshot = match snapshot_at(state, pair, Some(tf)).await {
        Some(s) => s,
        None => return Outcome::refused(format!("no snapshot yet for `{pair}` @ {tf}s")),
    };
    let levels: Vec<(String, Value)> = snapshot
        .indicators
        .iter()
        .filter(|(_, v)| {
            v.signals
                .iter()
                .any(|s| matches!(format!("{:?}", s.kind).as_str(), "LevelTest" | "Breakout"))
        })
        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(Value::Null)))
        .collect();
    let mut out = format!("  {pair} @ {tf}s — {} level(s)\n", levels.len());
    for (k, _) in &levels {
        out.push_str(&format!("  {k}\n"));
    }
    Outcome::new(json!({ "levels": levels }), out)
}

/// The latest snapshot for `(pair, duration)`; `None` duration = the fastest.
async fn snapshot_at(
    state: &Arc<AppState>,
    pair: &str,
    tf: Option<u64>,
) -> Option<core_domain::models::MarketSnapshot> {
    let active = state.get_active_pair(pair).await?;
    let latest = match tf {
        Some(secs) => active
            .pipeline_for_secs(secs)
            .map(|p| p.latest_snapshot.clone()),
        None => active.all().first().map(|p| p.latest_snapshot.clone()),
    };
    latest?.read().await.clone()
}

/// Wall-clock milliseconds. Only for *observation* stamps (quality windows) —
/// never for a model reading, which is bar-indexed (CG12).
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// The manifest status of a command, so the shell can refuse honestly.
pub fn status_of(command: &str) -> Option<Status> {
    super::manifest::MANIFEST
        .iter()
        .find(|row| row.command == command)
        .map(|row| row.status)
}
