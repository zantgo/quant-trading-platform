// COLD PATH — rolling quality metrics with 60 s persistence loop.
// This module tracks aggregate statistics over 1h/6h/24h windows.
// It is read by the frontend dashboard and persisted to SQLite.
// It does not gate the live data pipeline.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

const MAX_EVENT_RETENTION_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QualityWindow {
    OneHour,
    SixHour,
    TwentyFourHour,
}

impl QualityWindow {
    pub fn duration(&self) -> Duration {
        match self {
            Self::OneHour => Duration::from_secs(3600),
            Self::SixHour => Duration::from_secs(6 * 3600),
            Self::TwentyFourHour => Duration::from_secs(24 * 3600),
        }
    }

    fn database_name(&self) -> &'static str {
        match self {
            Self::OneHour => "ONE_HOUR",
            Self::SixHour => "SIX_HOUR",
            Self::TwentyFourHour => "TWENTY_FOUR_HOUR",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionQualityReport {
    pub window: QualityWindow,
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    pub uptime_pct: f64,
    pub disconnect_count: u32,
    pub avg_reconnect_ms: f64,
    pub total_data_loss_secs: u64,
    pub reconstructed_candles: u32,
    pub score: f64,
}

#[derive(Clone)]
pub struct ConnectionQualityTracker {
    state: Arc<RwLock<TrackerState>>,
}

struct TrackerState {
    events: VecDeque<QualityEvent>,
    last_heartbeat_ms: u64,
    last_connected_ms: Option<u64>,
    cumulative_connected_secs: u64,
    /// AUDIT-M17: wall-clock of the most recent interaction, used only by
    /// `ConnectionQualityRegistry::prune_idle_scopes` to drop scopes nobody
    /// has touched. Distinct from `last_heartbeat_ms`, which tracks stream
    /// liveness and is deliberately not written by replay/backfill paths.
    last_seen_ms: u64,
}

#[derive(Debug, Clone)]
enum QualityEvent {
    Connected {
        at_ms: u64,
    },
    Disconnected {
        at_ms: u64,
    },
    ReconnectCompleted {
        at_ms: u64,
        duration_ms: u64,
    },
    /// AUDIT-V9 B3: a candle was reconstructed (REST gap fill or
    /// sub-minute synthesis). Carries the timestamp so the rolling
    /// window report can count only events that fall inside the
    /// window — replacing the previous unbounded process-lifetime
    /// `reconstructed_candle_count` which permanently penalised the
    /// composite score for the rest of the session.
    Reconstructed {
        at_ms: u64,
    },
}

impl ConnectionQualityTracker {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(TrackerState {
                events: VecDeque::new(),
                last_heartbeat_ms: 0,
                last_connected_ms: None,
                cumulative_connected_secs: 0,
                last_seen_ms: 0,
            })),
        }
    }

    /// AUDIT-M17: most recent interaction, for idle-scope pruning.
    pub fn last_seen_ms(&self) -> u64 {
        // A non-blocking peek: `prune_idle_scopes` runs on the persistence
        // sweep and must never stall behind a per-scope write lock.
        match self.state.try_read() {
            Ok(g) => g.last_seen_ms,
            Err(_) => 0,
        }
    }

    pub async fn record_connect(&self, at_ms: u64) {
        let mut state = self.state.write().await;
        state.last_seen_ms = state.last_seen_ms.max(at_ms);
        if state.last_connected_ms.is_none() {
            state.last_connected_ms = Some(at_ms);
        }
        append_event(&mut state, QualityEvent::Connected { at_ms }, at_ms);
    }

    pub async fn record_disconnect(&self, at_ms: u64) {
        let mut state = self.state.write().await;
        state.last_seen_ms = state.last_seen_ms.max(at_ms);
        if let Some(connected_at_ms) = state.last_connected_ms.take() {
            if at_ms >= connected_at_ms {
                state.cumulative_connected_secs = state
                    .cumulative_connected_secs
                    .saturating_add((at_ms - connected_at_ms) / 1000);
            }
        }
        append_event(&mut state, QualityEvent::Disconnected { at_ms }, at_ms);
    }

    pub async fn record_reconnect(&self, at_ms: u64, duration_ms: u64) {
        let mut state = self.state.write().await;
        state.last_seen_ms = state.last_seen_ms.max(at_ms);
        if state.last_connected_ms.is_none() {
            state.last_connected_ms = Some(at_ms);
        }
        append_event(
            &mut state,
            QualityEvent::ReconnectCompleted { at_ms, duration_ms },
            at_ms,
        );
    }

    pub async fn record_heartbeat(&self, at_ms: u64) {
        let mut state = self.state.write().await;
        state.last_seen_ms = state.last_seen_ms.max(at_ms);
        state.last_heartbeat_ms = at_ms;
    }

    pub async fn record_reconstructed_candle(&self, at_ms: u64) {
        let mut state = self.state.write().await;
        append_event(&mut state, QualityEvent::Reconstructed { at_ms }, at_ms);
    }

    pub async fn report(&self, window: QualityWindow, now_ms: u64) -> ConnectionQualityReport {
        let mut state = self.state.write().await;
        prune_events(&mut state, now_ms);

        let window_start_ms = now_ms.saturating_sub(window.duration().as_millis() as u64);
        let window_end_ms = now_ms;
        let events: Vec<QualityEvent> = state.events.iter().cloned().collect();
        let mut ordered_events = events;
        ordered_events.sort_by_key(event_timestamp);

        let mut disconnected_at_ms = None;
        for event in ordered_events.iter() {
            if event_timestamp(event) >= window_start_ms {
                break;
            }
            match event {
                QualityEvent::Disconnected { .. } => {
                    disconnected_at_ms = Some(window_start_ms);
                }
                QualityEvent::Connected { .. } | QualityEvent::ReconnectCompleted { .. } => {
                    disconnected_at_ms = None;
                }
                // AUDIT-V9 B3: reconstructed events do not affect the
                // open-disconnect-window state.
                QualityEvent::Reconstructed { .. } => {}
            }
        }

        let mut data_loss_ms = 0_u64;
        let mut disconnect_count = 0_u32;
        let mut reconnect_sum_ms = 0_u64;
        let mut reconnect_count = 0_u32;
        let mut reconstructed_count = 0_u32;

        for event in ordered_events.iter() {
            let at_ms = event_timestamp(event);
            if at_ms < window_start_ms || at_ms > window_end_ms {
                continue;
            }

            match event {
                QualityEvent::Connected { .. } => {
                    close_disconnect(&mut data_loss_ms, &mut disconnected_at_ms, at_ms);
                }
                QualityEvent::Disconnected { .. } => {
                    disconnect_count = disconnect_count.saturating_add(1);
                    if disconnected_at_ms.is_none() {
                        disconnected_at_ms = Some(at_ms);
                    }
                }
                QualityEvent::ReconnectCompleted { duration_ms, .. } => {
                    reconnect_sum_ms = reconnect_sum_ms.saturating_add(*duration_ms);
                    reconnect_count = reconnect_count.saturating_add(1);
                    close_disconnect(&mut data_loss_ms, &mut disconnected_at_ms, at_ms);
                }
                QualityEvent::Reconstructed { .. } => {
                    // AUDIT-V9 B3: count reconstructed candles inside the
                    // window instead of using the previous unbounded
                    // process-lifetime counter. Prevents a single bad
                    // cold start from permanently depressing the score.
                    reconstructed_count = reconstructed_count.saturating_add(1);
                }
            }
        }

        if let Some(disconnected_at_ms) = disconnected_at_ms {
            data_loss_ms =
                data_loss_ms.saturating_add(window_end_ms.saturating_sub(disconnected_at_ms));
        }

        let window_duration_secs = window.duration().as_secs();
        let total_data_loss_secs = (data_loss_ms / 1000).min(window_duration_secs);
        let uptime_pct = ((window_duration_secs.saturating_sub(total_data_loss_secs) as f64)
            / window_duration_secs as f64
            * 100.0)
            .clamp(0.0, 100.0);
        let avg_reconnect_ms = if reconnect_count == 0 {
            0.0
        } else {
            reconnect_sum_ms as f64 / reconnect_count as f64
        };
        let disconnect_factor = 1.0 - (disconnect_count as f64 / 10.0).min(1.0);
        let reconnect_factor = 1.0 - (avg_reconnect_ms / 5000.0).min(1.0);
        let data_loss_penalty = 5.0 * (total_data_loss_secs as f64 / 600.0).min(1.0);
        let reconstructed_penalty = 5.0 * (reconstructed_count as f64 / 100.0).min(1.0);
        let score = (0.5 * uptime_pct + 30.0 * disconnect_factor + 20.0 * reconnect_factor
            - data_loss_penalty
            - reconstructed_penalty)
            .clamp(0.0, 100.0);

        ConnectionQualityReport {
            window,
            window_start_ms,
            window_end_ms,
            uptime_pct,
            disconnect_count,
            avg_reconnect_ms,
            total_data_loss_secs,
            reconstructed_candles: reconstructed_count,
            score,
        }
    }
}

/// Registry of per-`(pair_key, timeframe_secs)` quality scopes
/// (08-05-connection-quality.md). Each scope is an independent
/// `ConnectionQualityTracker`; the persistence loop collapses every scope
/// into per-window rows every 60 s.
#[derive(Clone, Default)]
pub struct ConnectionQualityRegistry {
    scopes: Arc<RwLock<std::collections::HashMap<(String, u64), ConnectionQualityTracker>>>,
}

impl ConnectionQualityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get-or-create the tracker scope for `(pair_key, timeframe_secs)`.
    pub async fn scope(&self, pair_key: &str, timeframe_secs: u64) -> ConnectionQualityTracker {
        let mut scopes = self.scopes.write().await;
        scopes
            .entry((pair_key.to_string(), timeframe_secs))
            .or_insert_with(ConnectionQualityTracker::new)
            .clone()
    }

    /// AUDIT-M17: drop every scope for a pair.
    ///
    /// The registry used to be a `HashMap` with NO remove/evict API anywhere, so
    /// deleting an instance -- or removing a duration from the ladder via
    /// `POST /api/config` and later re-adding it -- left a permanent entry
    /// holding its event deque. Over a long-lived process with pair churn that
    /// is steady-state memory growth in a daemon that has already been
    /// OOM-killed at 2.1 GB.
    ///
    /// Call this from the same place the retained-snapshot budget is refunded on
    /// delete/recharge, so the two accounting paths cannot drift apart.
    pub async fn evict_pair(&self, pair_key: &str) -> usize {
        let mut scopes = self.scopes.write().await;
        let before = scopes.len();
        scopes.retain(|(k, _), _| k != pair_key);
        before - scopes.len()
    }

    /// AUDIT-M17: drop scopes that have not been touched since `now_ms - idle`.
    ///
    /// A cheap sweep for anything `evict_pair` missed (e.g. a pair renamed by a
    /// C1 key correction). Called from the periodic persistence loop, which
    /// already walks every scope.
    pub async fn prune_idle_scopes(&self, now_ms: u64) -> usize {
        let cutoff = now_ms.saturating_sub(MAX_SCOPE_IDLE_MS);
        let mut scopes = self.scopes.write().await;
        let before = scopes.len();
        scopes.retain(|_, tracker| tracker.last_seen_ms() >= cutoff);
        before - scopes.len()
    }

    /// Number of live scopes (test/diagnostic visibility).
    pub async fn scope_count(&self) -> usize {
        self.scopes.read().await.len()
    }

    /// Report for one scope, or `None` when the scope has never been seen.
    pub async fn scoped_report(
        &self,
        pair_key: &str,
        timeframe_secs: u64,
        window: QualityWindow,
        now_ms: u64,
    ) -> Option<ConnectionQualityReport> {
        let tracker = {
            let scopes = self.scopes.read().await;
            scopes.get(&(pair_key.to_string(), timeframe_secs)).cloned()
        };
        match tracker {
            Some(t) => Some(t.report(window, now_ms).await),
            None => None,
        }
    }

    /// All per-scope reports, keyed by `(pair_key, timeframe_secs)`.
    pub async fn all_reports(
        &self,
        window: QualityWindow,
        now_ms: u64,
    ) -> Vec<(String, u64, ConnectionQualityReport)> {
        let snapshot: Vec<((String, u64), ConnectionQualityTracker)> = {
            let scopes = self.scopes.read().await;
            scopes.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };
        let mut out = Vec::with_capacity(snapshot.len());
        for ((pair_key, tf), tracker) in snapshot {
            out.push((pair_key, tf, tracker.report(window, now_ms).await));
        }
        out
    }

    /// Cross-scope aggregate (the process-wide view served when the API
    /// caller does not filter by instance/timeframe). The composite
    /// `score` is the **mean of per-scope scores** — operators expect
    /// the workspace-wide view to behave like an average of the
    /// individual pair × TF panels, not a re-application of the formula
    /// against summed counters.
    ///
    /// AUDIT-V9 B2: the previous implementation summed `disconnect_count`
    /// and `reconstructed_candles` across scopes then re-applied the
    /// formula. With N pairs × 4 TFs, a single reconnect cycle on one
    /// pair inflated the aggregate disconnect count by 4, so a 3-pair
    /// workspace hit the disconnect_factor = 0 floor and permanently
    /// lost all 30 score points.
    ///
    /// AUDIT-V9 B12: `total_data_loss_secs` is now summed (was max).
    /// Previously a workspace where 2 of 5 pairs each lost 60 s of
    /// data reported `total_data_loss_secs = 60`, which made the
    /// aggregate **optimistic** relative to summing — combined with B2
    /// this gave asymmetric and confusing behaviour. Sum is the
    /// conservative interpretation.
    pub async fn aggregate_report(
        &self,
        window: QualityWindow,
        now_ms: u64,
    ) -> ConnectionQualityReport {
        let reports = self.all_reports(window, now_ms).await;
        let window_start_ms = now_ms.saturating_sub(window.duration().as_millis() as u64);
        if reports.is_empty() {
            return ConnectionQualityReport {
                window,
                window_start_ms,
                window_end_ms: now_ms,
                uptime_pct: 100.0,
                disconnect_count: 0,
                avg_reconnect_ms: 0.0,
                total_data_loss_secs: 0,
                reconstructed_candles: 0,
                score: 100.0,
            };
        }
        let n = reports.len() as f64;
        let uptime_pct = reports.iter().map(|(_, _, r)| r.uptime_pct).sum::<f64>() / n;
        let disconnect_count: u32 = reports.iter().map(|(_, _, r)| r.disconnect_count).sum();
        let reconnects: Vec<f64> = reports
            .iter()
            .map(|(_, _, r)| r.avg_reconnect_ms)
            .filter(|v| *v > 0.0)
            .collect();
        let avg_reconnect_ms = if reconnects.is_empty() {
            0.0
        } else {
            reconnects.iter().sum::<f64>() / reconnects.len() as f64
        };
        let total_data_loss_secs: u64 =
            reports.iter().map(|(_, _, r)| r.total_data_loss_secs).sum();
        let reconstructed_candles: u32 = reports
            .iter()
            .map(|(_, _, r)| r.reconstructed_candles)
            .sum();
        // AUDIT-V9 B2: composite is the mean of per-scope composite
        // scores — a workspace-wide re-application of the formula is
        // misleading because its counters aggregate cross-scope.
        let score = reports.iter().map(|(_, _, r)| r.score).sum::<f64>() / n;
        ConnectionQualityReport {
            window,
            window_start_ms,
            window_end_ms: now_ms,
            uptime_pct,
            disconnect_count,
            avg_reconnect_ms,
            total_data_loss_secs,
            reconstructed_candles,
            score: score.clamp(0.0, 100.0),
        }
    }

    /// Persist every scope × window as one row in
    /// `connection_quality_samples` every 60 s (03-01-00 §3). Rows carry the
    /// `(pair_key, timeframe_secs)` scope columns per 08-05.
    pub async fn run_persistence_loop(
        self,
        db_pool: sqlx::SqlitePool,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    let now: DateTime<Utc> = Utc::now();
                    let now_ms = now.timestamp_millis().max(0) as u64;
                    // AUDIT-M17: sweep scopes nobody has touched in 24 h. The
                    // registry previously had NO eviction path at all, so every
                    // deleted instance and every removed duration left a
                    // permanent entry. This loop already walks every scope.
                    if self.prune_idle_scopes(now_ms).await > 0 {
                        eprintln!(
                            "🧹 Connection-quality registry: pruned idle scopes, {} remaining",
                            self.scope_count().await
                        );
                    }
                    for window in [
                        QualityWindow::OneHour,
                        QualityWindow::SixHour,
                        QualityWindow::TwentyFourHour,
                    ] {
                        for (pair_key, timeframe_secs, report) in
                            self.all_reports(window, now_ms).await
                        {
                            if let Err(error) = sqlx::query(
                                "INSERT INTO connection_quality_samples (timestamp_ms, window, uptime_pct, disconnect_count, avg_reconnect_ms, total_data_loss_secs, reconstructed_candles, score, pair_key, timeframe_secs) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                            )
                            .bind(now_ms as i64)
                            .bind(window.database_name())
                            .bind(report.uptime_pct)
                            .bind(report.disconnect_count as i64)
                            .bind(report.avg_reconnect_ms)
                            .bind(report.total_data_loss_secs as i64)
                            .bind(report.reconstructed_candles as i64)
                            .bind(report.score)
                            .bind(&pair_key)
                            .bind(timeframe_secs as i64)
                            .execute(&db_pool)
                            .await
                            {
                                eprintln!("Connection quality persistence failed: {error}");
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Default for ConnectionQualityTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn append_event(state: &mut TrackerState, event: QualityEvent, at_ms: u64) {
    state.events.push_back(event);
    prune_events(state, at_ms);
    // AUDIT-M17: hard cap on retained events.
    //
    // `prune_events` alone is not a bound. It walks from the FRONT assuming the
    // deque is in timestamp order, and that assumption does not hold: the
    // analyzer calls `record_reconstructed_candle(gap_candle.start_time_ms)`
    // with the HISTORICAL candle timestamp, so an out-of-order old event is
    // appended at the BACK where the front-walk never reaches it. A 500-candle
    // sub-minute backfill therefore appended 500 events that survived until the
    // next `report()` with a current `now_ms` — and if no report ever ran for
    // that scope, indefinitely.
    //
    // An absolute cap makes retention independent of ordering, which is the
    // only robust bound here. 512 is far above any real connection-event rate
    // (the longest window is 24 h) while keeping the O(n log n) sort in
    // `report()` cheap.
    while state.events.len() > MAX_EVENTS_PER_SCOPE {
        state.events.pop_front();
    }
}

/// AUDIT-M17: absolute per-scope event cap. See `append_event`.
const MAX_EVENTS_PER_SCOPE: usize = 512;

/// AUDIT-M17: how long a scope may go unseen before the registry drops it.
const MAX_SCOPE_IDLE_MS: u64 = 24 * 60 * 60 * 1000;

fn prune_events(state: &mut TrackerState, reference_ms: u64) {
    let cutoff_ms = reference_ms.saturating_sub(MAX_EVENT_RETENTION_MS);
    while state.events.len() > 1 {
        let should_remove = state
            .events
            .get(1)
            .map(|event| event_timestamp(event) < cutoff_ms)
            .unwrap_or(false);
        if should_remove {
            state.events.pop_front();
        } else {
            break;
        }
    }
    // AUDIT-M17: `len() > 1` above deliberately kept the last event, but a
    // lone event older than the cutoff was then never removed — so a scope that
    // disconnected once and never reconnected kept that event forever. Drop it.
    if state.events.len() == 1 {
        let stale = state
            .events
            .front()
            .map(|e| event_timestamp(e) < cutoff_ms)
            .unwrap_or(false);
        if stale {
            state.events.pop_front();
        }
    }
}

fn event_timestamp(event: &QualityEvent) -> u64 {
    match event {
        QualityEvent::Connected { at_ms }
        | QualityEvent::Disconnected { at_ms }
        | QualityEvent::ReconnectCompleted { at_ms, .. }
        | QualityEvent::Reconstructed { at_ms } => *at_ms,
    }
}

fn close_disconnect(
    data_loss_ms: &mut u64,
    disconnected_at_ms: &mut Option<u64>,
    connected_at_ms: u64,
) {
    if let Some(disconnected_at_ms_value) = disconnected_at_ms.take() {
        *data_loss_ms =
            data_loss_ms.saturating_add(connected_at_ms.saturating_sub(disconnected_at_ms_value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tracker_records_connect_disconnect_correctly() {
        let tracker = ConnectionQualityTracker::new();
        let base_ms = 10_000_000;
        tracker.record_connect(base_ms).await;
        tracker.record_disconnect(base_ms + 10_000).await;
        tracker.record_connect(base_ms + 20_000).await;

        let report = tracker
            .report(QualityWindow::OneHour, base_ms + 60_000)
            .await;

        assert_eq!(report.disconnect_count, 1);
        assert_eq!(report.total_data_loss_secs, 10);
    }

    #[tokio::test]
    async fn uptime_pct_computed_correctly_for_known_events() {
        let tracker = ConnectionQualityTracker::new();
        let now_ms = 10_000_000;
        let window_start_ms = now_ms - 3_600_000;
        tracker.record_connect(window_start_ms - 1_000).await;
        tracker.record_disconnect(window_start_ms + 600_000).await;
        tracker.record_connect(window_start_ms + 1_200_000).await;

        let report = tracker.report(QualityWindow::OneHour, now_ms).await;

        assert_eq!(report.total_data_loss_secs, 600);
        assert!((report.uptime_pct - (3000.0 / 3600.0 * 100.0)).abs() < 0.0001);
    }

    #[tokio::test]
    async fn disconnect_count_in_window() {
        let tracker = ConnectionQualityTracker::new();
        let now_ms = 10_000_000;
        tracker.record_connect(now_ms - 7_200_000).await;
        tracker.record_disconnect(now_ms - 7_000_000).await;
        tracker.record_connect(now_ms - 6_900_000).await;
        tracker.record_disconnect(now_ms - 100).await;
        tracker.record_connect(now_ms - 50).await;

        let report = tracker.report(QualityWindow::OneHour, now_ms).await;

        assert_eq!(report.disconnect_count, 1);
    }

    #[tokio::test]
    async fn avg_reconnect_ms_correct() {
        let tracker = ConnectionQualityTracker::new();
        let now_ms = 10_000_000;
        tracker.record_reconnect(now_ms - 2_000, 1_000).await;
        tracker.record_reconnect(now_ms - 1_000, 3_000).await;

        let report = tracker.report(QualityWindow::OneHour, now_ms).await;

        assert!((report.avg_reconnect_ms - 2_000.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn score_decreases_with_more_disconnects() {
        let one_disconnect = ConnectionQualityTracker::new();
        let many_disconnects = ConnectionQualityTracker::new();
        let base_ms = 10_000_000;
        one_disconnect.record_connect(base_ms).await;
        one_disconnect.record_disconnect(base_ms + 1_000).await;
        one_disconnect.record_reconnect(base_ms + 2_000, 100).await;

        many_disconnects.record_connect(base_ms).await;
        for index in 0..5 {
            let disconnected_at_ms = base_ms + 1_000 + index * 10_000;
            many_disconnects.record_disconnect(disconnected_at_ms).await;
            many_disconnects
                .record_reconnect(disconnected_at_ms + 1_000, 100)
                .await;
        }

        let one_report = one_disconnect
            .report(QualityWindow::OneHour, base_ms + 60_000)
            .await;
        let many_report = many_disconnects
            .report(QualityWindow::OneHour, base_ms + 60_000)
            .await;

        assert!(many_report.score < one_report.score);
    }

    #[tokio::test]
    async fn reconstructed_candles_counted() {
        let tracker = ConnectionQualityTracker::new();
        tracker.record_reconstructed_candle(10_000_000).await;
        tracker.record_reconstructed_candle(10_000_000).await;
        tracker.record_reconstructed_candle(10_000_000).await;

        let report = tracker.report(QualityWindow::OneHour, 10_000_000).await;

        assert_eq!(report.reconstructed_candles, 3);
    }

    #[tokio::test]
    async fn window_1h_filters_out_old_events() {
        let tracker = ConnectionQualityTracker::new();
        let now_ms = 10_000_000;
        tracker.record_connect(now_ms - 7_200_000).await;
        tracker.record_disconnect(now_ms - 7_000_000).await;
        tracker.record_reconnect(now_ms - 6_999_000, 9_000).await;
        tracker.record_disconnect(now_ms - 1_000).await;
        tracker.record_reconnect(now_ms - 500, 1_000).await;

        let report = tracker.report(QualityWindow::OneHour, now_ms).await;

        assert_eq!(report.disconnect_count, 1);
        assert!((report.avg_reconnect_ms - 1_000.0).abs() < f64::EPSILON);
    }

    /// AUDIT-V9 B3: reconstructed candles from a previous window MUST NOT
    /// count in the current window's report. The previous implementation
    /// accumulated `reconstructed_candle_count` for the lifetime of the
    /// process and subtracted up to 5 points from the score forever —
    /// which is why a single cold-start with a gap permanently
    /// depressed the score for the rest of the session.
    #[tokio::test]
    async fn reconstructed_candles_are_windowed() {
        let tracker = ConnectionQualityTracker::new();
        let now_ms = 10_000_000;
        let window_start_ms = now_ms - 3_600_000;
        // 5 reconstructions inside the 1h window.
        for off in [60_000, 120_000, 180_000, 240_000, 300_000] {
            tracker
                .record_reconstructed_candle(window_start_ms + off)
                .await;
        }
        // 50 reconstructions OUTSIDE the window (90 minutes ago) — these
        // must be pruned and NOT count toward the report.
        for off in 0..50 {
            tracker
                .record_reconstructed_candle(window_start_ms - 60_000 - (off * 1_000))
                .await;
        }

        let report = tracker.report(QualityWindow::OneHour, now_ms).await;

        assert_eq!(
            report.reconstructed_candles, 5,
            "old reconstructed events must not pollute the current window report"
        );
    }

    /// AUDIT-V9 B2: aggregate_score must be the mean of per-scope scores
    /// and must NOT collapse to the disconnect_factor=0 floor when many
    /// scopes each report 1 disconnect. The previous implementation
    /// summed disconnect counts across scopes, then re-applied the
    /// formula — so a 12-scope workspace (3 pairs × 4 TFs) lost all 30
    /// disconnect-factor points from a single reconnect cycle on one
    /// pair.
    #[tokio::test]
    async fn aggregate_score_averages_per_scope_scores() {
        let registry = ConnectionQualityRegistry::new();
        let now_ms = 10_000_000;
        // 12 scopes, each with exactly 1 disconnect inside the window.
        for pair in 0..3 {
            for tf_secs in [60_u64, 180, 300, 900] {
                let t = registry.scope(&format!("PAIR-{pair}"), tf_secs).await;
                t.record_connect(now_ms - 60_000).await;
                t.record_disconnect(now_ms - 30_000).await;
                t.record_reconnect(now_ms - 29_000, 100).await;
            }
        }

        let agg = registry
            .aggregate_report(QualityWindow::OneHour, now_ms)
            .await;
        // Per-scope score should be 99.x (one tiny disconnect).
        // Average across 12 scopes should still be in the 90s — NOT
        // 70.0 (which is what the old summed formula would give).
        assert!(
            agg.score > 90.0,
            "aggregate must average per-scope scores; got {} (expected > 90)",
            agg.score
        );
        // disconnect_count summed is reported as-is (12) so operators
        // can see total incident count even though the score is averaged.
        assert_eq!(agg.disconnect_count, 12);
    }

    /// AUDIT-V9 B12: total_data_loss_secs is summed, not maxed. Two
    /// scopes each losing 60 s of data must report 120 s in the
    /// aggregate, not 60.
    #[tokio::test]
    async fn aggregate_sums_data_loss_secs() {
        let registry = ConnectionQualityRegistry::new();
        let now_ms = 10_000_000;
        // Pair A: 60s of data loss.
        let a = registry.scope("PAIR-A", 60).await;
        a.record_connect(now_ms - 600_000).await;
        a.record_disconnect(now_ms - 240_000).await;
        a.record_reconnect(now_ms - 180_000, 1_000).await;
        // Pair B: 90s of data loss.
        let b = registry.scope("PAIR-B", 60).await;
        b.record_connect(now_ms - 600_000).await;
        b.record_disconnect(now_ms - 360_000).await;
        b.record_reconnect(now_ms - 270_000, 1_000).await;

        let agg = registry
            .aggregate_report(QualityWindow::OneHour, now_ms)
            .await;
        assert_eq!(
            agg.total_data_loss_secs, 150,
            "data loss must be summed across scopes (60 + 90)"
        );
    }
}
