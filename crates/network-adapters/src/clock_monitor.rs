// COLD PATH — background NTP polling and drift analysis.
// This module runs asynchronously on a long interval (default 30 s).
// Blocking or latency is acceptable; it does not gate the data pipeline.
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreachAction {
    Warn,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftVerdict {
    WithinThreshold {
        offset_us: i64,
        rtt_us: u64,
        server: String,
    },
    BreachThreshold {
        offset_us: i64,
        rtt_us: u64,
        server: String,
        threshold_us: i64,
    },
    /// v11.12.19: the sample's round-trip exceeded the reliability bound, so
    /// the offset cannot be trusted (uncertainty ≈ rtt/2) — drift is NOT
    /// evaluated and `breach_action=panic` never fires from it.
    Unreliable {
        offset_us: i64,
        rtt_us: u64,
        server: String,
        max_rtt_us: u64,
    },
    NetworkError {
        message: String,
        retry_after: Duration,
    },
}

#[derive(Debug, Clone)]
pub struct ClockSample {
    pub offset_us: i64,
    pub rtt_us: u64,
    pub server: String,
    pub measured_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ClockMonitorConfig {
    pub ntp_servers: Vec<String>,
    pub poll_interval: Duration,
    pub threshold: Duration,
    pub breach_action: BreachAction,
    pub warn_on_breach: bool,
    pub jitter_window_size: usize,
    pub query_timeout: Duration,
    /// v11.12.19: samples with a round-trip above this bound are reported as
    /// `Unreliable` instead of being judged — a slow network path must never
    /// masquerade as clock drift.
    pub max_rtt: Duration,
}

impl Default for ClockMonitorConfig {
    fn default() -> Self {
        Self {
            ntp_servers: vec!["pool.ntp.org".to_string(), "time.aws.com".to_string()],
            poll_interval: Duration::from_secs(30),
            // 10 ms matches config.toml / config-models default_clock_monitor_threshold_micros.
            // The prior 50µs bare default was never used in production (TOML overrides it)
            // but made unit tests 200× stricter than deployed.
            threshold: Duration::from_micros(10_000),
            breach_action: BreachAction::Warn,
            warn_on_breach: true,
            jitter_window_size: 20,
            query_timeout: Duration::from_secs(5),
            // 1 s: an NTP sample with a round-trip beyond this cannot resolve
            // a 10 ms budget (offset uncertainty ≈ rtt/2) — reported, not judged.
            max_rtt: Duration::from_secs(1),
        }
    }
}

pub struct ClockMonitor {
    config: ClockMonitorConfig,
    samples: Mutex<Vec<ClockSample>>,
    breach_count: AtomicU32,
}

impl ClockMonitor {
    pub fn new(config: ClockMonitorConfig) -> Self {
        Self {
            config,
            samples: Mutex::new(Vec::new()),
            breach_count: AtomicU32::new(0),
        }
    }

    pub fn config(&self) -> &ClockMonitorConfig {
        &self.config
    }

    /// Single NTP measurement. Returns a verdict; updates internal sample history on
    /// any successful query. On network failure for every configured server, returns
    /// `NetworkError`. Never panics on transport errors.
    pub async fn measure_once(&self) -> DriftVerdict {
        let threshold_us = clamp_threshold_to_i64(self.config.threshold);
        let max_rtt_us = self.config.max_rtt.as_micros().min(u64::MAX as u128) as u64;

        for server in &self.config.ntp_servers {
            let addr = format!("{}:123", server);
            match query_server(&addr, self.config.query_timeout).await {
                Ok(ntp_result) => {
                    let sample = ClockSample {
                        offset_us: ntp_result.offset,
                        rtt_us: ntp_result.roundtrip,
                        server: server.clone(),
                        measured_at_ms: now_ms(),
                    };
                    let verdict = verdict_from_sample(&sample, threshold_us, max_rtt_us);
                    self.record_sample(sample);
                    return verdict;
                }
                Err(err) => {
                    eprintln!(
                        "ClockMonitor: NTP query to {} failed: {:?} — trying next server",
                        server, err
                    );
                }
            }
        }

        DriftVerdict::NetworkError {
            message: "all configured NTP servers unreachable".to_string(),
            retry_after: self.config.poll_interval,
        }
    }

    /// Long-running loop: measure every `poll_interval`. Honours the supplied
    /// `CancellationToken` for graceful shutdown. Network errors are logged and the
    /// loop continues — the monitor never panics on transport errors.
    pub async fn run_until_cancelled(&self, cancel: CancellationToken) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    println!("ClockMonitor: cancelled, shutting down");
                    return;
                }
                _ = tokio::time::sleep(self.config.poll_interval) => {
                    let verdict = self.measure_once().await;
                    self.handle_verdict(&verdict);
                }
            }
        }
    }

    pub fn handle_verdict(&self, verdict: &DriftVerdict) {
        match verdict {
            DriftVerdict::WithinThreshold {
                offset_us,
                rtt_us,
                server,
            } => {
                println!(
                    "ClockMonitor: drift within threshold (offset={}µs, rtt={}µs, server={})",
                    offset_us, rtt_us, server
                );
            }
            DriftVerdict::BreachThreshold {
                offset_us,
                rtt_us,
                server,
                threshold_us,
            } => {
                self.breach_count.fetch_add(1, Ordering::Relaxed);
                let msg = format!(
                    "CLOCK DRIFT BREACH: |{}µs| > threshold {}µs (rtt={}µs, server={})",
                    offset_us, threshold_us, rtt_us, server
                );
                if self.config.warn_on_breach {
                    eprintln!("{}", msg);
                }
                if matches!(self.config.breach_action, BreachAction::Panic) {
                    // K2 (production audit): `panic!` inside the monitor task was
                    // discarded by `join_all`, killing only the monitor. The
                    // operator explicitly chose `breach_action=panic` as a
                    // hard-stop — terminate the process so trading stops. Flush
                    // stderr first so the breach message is not lost in the WAL
                    // tail; the daemon's graceful shutdown path (SIGTERM) already
                    // drains the telemetry queue, but a hard-stop is by definition
                    // immediate.
                    eprintln!("{}", msg);
                    eprintln!("ClockMonitor: breach_action=panic — terminating process");
                    // Best-effort flush before exit (not async, so no drain).
                    use std::io::Write as _;
                    let _ = std::io::stderr().flush();
                    let _ = std::io::stdout().flush();
                    std::process::exit(1);
                }
            }
            DriftVerdict::Unreliable {
                offset_us,
                rtt_us,
                server,
                max_rtt_us,
            } => {
                // v11.12.19: informational only — never a breach, never a
                // panic, never a breach_count increment.
                if self.config.warn_on_breach {
                    eprintln!(
                        "ClockMonitor: NTP sample UNRELIABLE — rtt={}µs > max {}µs (offset={}µs, server={}); drift not evaluated",
                        rtt_us, max_rtt_us, offset_us, server
                    );
                }
            }
            DriftVerdict::NetworkError {
                message,
                retry_after,
            } => {
                eprintln!(
                    "ClockMonitor: NTP network error: {} (retry in {}s)",
                    message,
                    retry_after.as_secs()
                );
            }
        }
    }

    /// Compute the rolling RMS jitter (standard deviation) from the last
    /// `jitter_window_size` samples, in microseconds.
    pub fn rms_jitter_us(&self) -> Option<f64> {
        let samples = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        if samples.len() < 2 {
            return None;
        }
        let n = samples.len() as f64;
        let mean: f64 = samples.iter().map(|s| s.offset_us as f64).sum::<f64>() / n;
        let variance: f64 = samples
            .iter()
            .map(|s| (s.offset_us as f64 - mean).powi(2))
            .sum::<f64>()
            / n;
        Some(variance.sqrt())
    }

    pub fn current_offset_us(&self) -> Option<i64> {
        let samples = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        samples.last().map(|s| s.offset_us)
    }

    pub fn sample_count(&self) -> usize {
        self.samples.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn breach_count(&self) -> u32 {
        self.breach_count.load(Ordering::Relaxed)
    }

    /// Record a sample into the rolling window. Public so tests can inject data
    /// and downstream consumers (e.g. a health endpoint) can record externally
    /// produced samples if they ever need to.
    pub fn record_sample(&self, sample: ClockSample) {
        let mut samples = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        samples.push(sample);
        let max_size = self.config.jitter_window_size.max(1);
        if samples.len() > max_size {
            let drop = samples.len() - max_size;
            samples.drain(0..drop);
        }
    }
}

fn clamp_threshold_to_i64(d: Duration) -> i64 {
    let micros = d.as_micros();
    if micros > i64::MAX as u128 {
        i64::MAX
    } else {
        micros as i64
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn verdict_from_sample(
    sample: &ClockSample,
    threshold_us: i64,
    max_rtt_us: u64,
) -> DriftVerdict {
    // v11.12.19: a sample taken over a slow path cannot prove a drift — the
    // offset uncertainty is ≈ rtt/2. Report it as unreliable (no breach, no
    // breach_count, no panic) instead of blaming the clock.
    if sample.rtt_us > max_rtt_us {
        return DriftVerdict::Unreliable {
            offset_us: sample.offset_us,
            rtt_us: sample.rtt_us,
            server: sample.server.clone(),
            max_rtt_us,
        };
    }
    let abs_offset = sample.offset_us.unsigned_abs() as i64;
    if abs_offset > threshold_us {
        DriftVerdict::BreachThreshold {
            offset_us: sample.offset_us,
            rtt_us: sample.rtt_us,
            server: sample.server.clone(),
            threshold_us,
        }
    } else {
        DriftVerdict::WithinThreshold {
            offset_us: sample.offset_us,
            rtt_us: sample.rtt_us,
            server: sample.server.clone(),
        }
    }
}

/// Synchronous `simple_get_time` runs on a blocking thread so it cannot stall the
/// tokio runtime. The UDP socket's read timeout guarantees we return within
/// `query_timeout` even when the peer never replies (returns `Error::Network`).
async fn query_server(addr: &str, query_timeout: Duration) -> sntpc::Result<sntpc::NtpResult> {
    let addr_owned = addr.to_string();
    let join = tokio::task::spawn_blocking(move || -> sntpc::Result<sntpc::NtpResult> {
        use std::net::UdpSocket;
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| sntpc::Error::Network)?;
        socket
            .set_read_timeout(Some(query_timeout))
            .map_err(|_| sntpc::Error::Network)?;
        sntpc::simple_get_time(addr_owned.as_str(), &socket)
    })
    .await;

    match join {
        Ok(inner) => inner,
        Err(_) => Err(sntpc::Error::Network),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(offset: i64) -> ClockSample {
        ClockSample {
            offset_us: offset,
            rtt_us: 1_000,
            server: "test.ntp".to_string(),
            measured_at_ms: 0,
        }
    }

    #[test]
    fn default_config_has_sane_values() {
        let cfg = ClockMonitorConfig::default();
        assert!(
            !cfg.ntp_servers.is_empty(),
            "at least one NTP server must be configured"
        );
        for s in &cfg.ntp_servers {
            assert!(!s.is_empty(), "NTP server host must be non-empty");
        }
        assert!(
            cfg.poll_interval >= Duration::from_secs(1),
            "poll_interval must be at least 1s to avoid hammering NTP peers"
        );
        assert!(
            cfg.threshold > Duration::ZERO,
            "threshold must be strictly positive"
        );
        assert!(
            cfg.query_timeout >= Duration::from_millis(100),
            "query_timeout must allow at least one round-trip"
        );
        assert!(
            cfg.jitter_window_size >= 2,
            "jitter_window_size must allow at least 2 samples for RMS"
        );
        assert_eq!(
            cfg.max_rtt,
            Duration::from_secs(1),
            "max_rtt default must be 1s (unreliable-sample bound)"
        );
        assert_eq!(cfg.breach_action, BreachAction::Warn);
        assert!(cfg.warn_on_breach);
    }

    #[test]
    fn rms_jitter_with_insufficient_samples_returns_none() {
        let monitor = ClockMonitor::new(ClockMonitorConfig {
            jitter_window_size: 20,
            ..ClockMonitorConfig::default()
        });

        assert_eq!(monitor.rms_jitter_us(), None, "no samples -> None");

        monitor.record_sample(sample(42));
        assert_eq!(
            monitor.rms_jitter_us(),
            None,
            "one sample is not enough to compute RMS jitter"
        );
    }

    #[test]
    fn rms_jitter_with_constant_samples_is_zero() {
        let monitor = ClockMonitor::new(ClockMonitorConfig {
            jitter_window_size: 20,
            ..ClockMonitorConfig::default()
        });
        for _ in 0..5 {
            monitor.record_sample(sample(123));
        }

        let rms = monitor
            .rms_jitter_us()
            .expect("five samples should yield RMS");
        assert!(
            rms.abs() < 1e-9,
            "constant offsets should produce zero RMS jitter, got {}",
            rms
        );
    }

    #[test]
    fn rms_jitter_with_known_samples() {
        let monitor = ClockMonitor::new(ClockMonitorConfig {
            jitter_window_size: 20,
            ..ClockMonitorConfig::default()
        });
        for off in [10, 20, 30, 40, 50] {
            monitor.record_sample(sample(off));
        }

        let rms = monitor
            .rms_jitter_us()
            .expect("five samples should yield RMS");
        // mean = 30, variance = (400+100+0+100+400)/5 = 200, RMS = sqrt(200) ≈ 14.1421
        let expected = (200.0_f64).sqrt();
        assert!(
            (rms - expected).abs() < 1e-9,
            "expected RMS = {}, got {}",
            expected,
            rms
        );
        assert!(
            (rms - 14.1421).abs() < 0.01,
            "RMS should match the textbook value 14.14, got {}",
            rms
        );
    }

    #[test]
    fn verdict_from_sample_within_threshold() {
        let threshold_us = 50;
        let s = sample(40);
        match verdict_from_sample(&s, threshold_us, 1_000_000) {
            DriftVerdict::WithinThreshold {
                offset_us,
                rtt_us,
                server,
            } => {
                assert_eq!(offset_us, 40);
                assert_eq!(rtt_us, 1_000);
                assert_eq!(server, "test.ntp");
            }
            other => panic!("expected WithinThreshold, got {:?}", other),
        }

        // exactly at threshold counts as within (strictly greater would breach)
        let s = sample(-50);
        assert!(matches!(
            verdict_from_sample(&s, threshold_us, 1_000_000),
            DriftVerdict::WithinThreshold { .. }
        ));
    }

    #[test]
    fn verdict_from_sample_breach_threshold() {
        let threshold_us = 50;
        let s = sample(75);
        match verdict_from_sample(&s, threshold_us, 1_000_000) {
            DriftVerdict::BreachThreshold {
                offset_us,
                rtt_us,
                server,
                threshold_us: t,
            } => {
                assert_eq!(offset_us, 75);
                assert_eq!(rtt_us, 1_000);
                assert_eq!(server, "test.ntp");
                assert_eq!(t, 50);
            }
            other => panic!("expected BreachThreshold, got {:?}", other),
        }

        let s = sample(-75);
        assert!(matches!(
            verdict_from_sample(&s, threshold_us, 1_000_000),
            DriftVerdict::BreachThreshold { .. }
        ));
    }

    #[test]
    fn verdict_from_sample_unreliable_when_rtt_exceeds_max() {
        // v11.12.19: the offset uncertainty is ≈ rtt/2, so a sample measured
        // over a multi-second round-trip cannot prove a drift and must be
        // reported as Unreliable (never a breach → never breach_action=panic).
        let mut slow = sample(4_200_000);
        slow.rtt_us = 8_601_938;
        assert!(matches!(
            verdict_from_sample(&slow, 10_000, 1_000_000),
            DriftVerdict::Unreliable {
                max_rtt_us: 1_000_000,
                ..
            }
        ));

        // The same offset over a fast sample is still a real breach.
        let mut fast = sample(4_200_000);
        fast.rtt_us = 1_000;
        assert!(matches!(
            verdict_from_sample(&fast, 10_000, 1_000_000),
            DriftVerdict::BreachThreshold { .. }
        ));

        // Exactly at the bound is still judged (strictly greater is unreliable).
        let mut edge = sample(4_200_000);
        edge.rtt_us = 1_000_000;
        assert!(matches!(
            verdict_from_sample(&edge, 10_000, 1_000_000),
            DriftVerdict::BreachThreshold { .. }
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn measure_once_with_unreachable_server_returns_network_error() {
        let cfg = ClockMonitorConfig {
            ntp_servers: vec!["127.0.0.1".to_string()],
            poll_interval: Duration::from_secs(60),
            query_timeout: Duration::from_secs(1),
            ..ClockMonitorConfig::default()
        };
        let monitor = ClockMonitor::new(cfg);

        let verdict = tokio::time::timeout(Duration::from_secs(3), monitor.measure_once())
            .await
            .expect("measure_once did not return within 3s — possible hang");

        assert!(
            matches!(verdict, DriftVerdict::NetworkError { .. }),
            "expected NetworkError for unreachable server, got {:?}",
            verdict
        );
        assert_eq!(
            monitor.sample_count(),
            0,
            "no sample should be recorded on transport failure"
        );
    }
}
