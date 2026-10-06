use std::time::Duration;

use network_adapters::clock_monitor::{
    verdict_from_sample, BreachAction, ClockMonitor, ClockMonitorConfig, ClockSample, DriftVerdict,
};

#[test]
fn config_defaults_are_correct() {
    let cfg = ClockMonitorConfig::default();
    assert_eq!(cfg.poll_interval, Duration::from_secs(30));
    // 10 ms matches config.toml / config-models default_clock_monitor_threshold_micros
    // (prior 50µs bare default was never used in production and made unit tests 200× stricter).
    assert_eq!(cfg.threshold, Duration::from_micros(10_000));
    assert_eq!(cfg.breach_action, BreachAction::Warn);
    // v11.12.19: reliability bound — samples slower than 1 s are "unreliable".
    assert_eq!(cfg.max_rtt, Duration::from_secs(1));
}

#[test]
fn rms_jitter_computation() {
    let monitor = ClockMonitor::new(ClockMonitorConfig {
        jitter_window_size: 20,
        ..ClockMonitorConfig::default()
    });

    for offset in [10, 20, 30, 40, 50] {
        monitor.record_sample(ClockSample {
            offset_us: offset,
            rtt_us: 1_000,
            server: "test.ntp".to_string(),
            measured_at_ms: 0,
        });
    }

    let rms = monitor
        .rms_jitter_us()
        .expect("should compute RMS from 5 samples");
    let expected = (200.0_f64).sqrt();
    assert!(
        (rms - expected).abs() < 1e-9,
        "RMS = {}, expected = {} (mean=30, variance=200)",
        rms,
        expected
    );
}

#[test]
fn breach_counter_increments_on_breach() {
    let monitor = ClockMonitor::new(ClockMonitorConfig::default());
    assert_eq!(monitor.breach_count(), 0);

    monitor.handle_verdict(&DriftVerdict::BreachThreshold {
        offset_us: 75,
        rtt_us: 1_000,
        server: "test.ntp".to_string(),
        threshold_us: 50,
    });
    assert_eq!(monitor.breach_count(), 1);

    monitor.handle_verdict(&DriftVerdict::BreachThreshold {
        offset_us: -60,
        rtt_us: 1_200,
        server: "test.ntp".to_string(),
        threshold_us: 50,
    });
    assert_eq!(monitor.breach_count(), 2);

    monitor.handle_verdict(&DriftVerdict::WithinThreshold {
        offset_us: 10,
        rtt_us: 500,
        server: "test.ntp".to_string(),
    });
    assert_eq!(
        monitor.breach_count(),
        2,
        "breach count must not increment on WithinThreshold"
    );
}

#[test]
fn within_threshold_verdict() {
    let sample = ClockSample {
        offset_us: 40,
        rtt_us: 1_000,
        server: "test.ntp".to_string(),
        measured_at_ms: 0,
    };

    match verdict_from_sample(&sample, 50, 1_000_000) {
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

    let sample = ClockSample {
        offset_us: -50,
        rtt_us: 1_000,
        server: "test.ntp".to_string(),
        measured_at_ms: 0,
    };
    assert!(matches!(
        verdict_from_sample(&sample, 50, 1_000_000),
        DriftVerdict::WithinThreshold { .. }
    ));
}

#[test]
fn breach_threshold_verdict() {
    let sample = ClockSample {
        offset_us: 75,
        rtt_us: 1_000,
        server: "test.ntp".to_string(),
        measured_at_ms: 0,
    };

    match verdict_from_sample(&sample, 50, 1_000_000) {
        DriftVerdict::BreachThreshold {
            offset_us,
            rtt_us,
            server,
            threshold_us,
        } => {
            assert_eq!(offset_us, 75);
            assert_eq!(rtt_us, 1_000);
            assert_eq!(server, "test.ntp");
            assert_eq!(threshold_us, 50);
        }
        other => panic!("expected BreachThreshold, got {:?}", other),
    }

    let sample = ClockSample {
        offset_us: -75,
        rtt_us: 800,
        server: "test.ntp".to_string(),
        measured_at_ms: 0,
    };
    assert!(matches!(
        verdict_from_sample(&sample, 50, 1_000_000),
        DriftVerdict::BreachThreshold { .. }
    ));
}

#[test]
fn three_polls_detect_breach() {
    let cfg = ClockMonitorConfig::default();
    assert_eq!(
        cfg.poll_interval,
        Duration::from_secs(30),
        "default poll_interval must be 30s — 3 polls = 90s"
    );

    let monitor = ClockMonitor::new(cfg);
    assert_eq!(monitor.breach_count(), 0);

    for _ in 0..3 {
        monitor.handle_verdict(&DriftVerdict::BreachThreshold {
            offset_us: 75,
            rtt_us: 1_000,
            server: "test.ntp".to_string(),
            threshold_us: 50,
        });
    }

    assert_eq!(
        monitor.breach_count(),
        3,
        "3 consecutive breached polls must be detected within 90s (3 × 30s)"
    );
}

// ── v11.12.24: drift-log hardening ──────────────────────────────────
//
// A single `pool.ntp.org` answer with a 64 ms round trip carries ≈ ±32 ms of
// offset uncertainty against a 10 ms budget, and the monitor printed that as
// a hard breach on every 30 s poll. v11.12.24 asks several servers and trusts
// the fastest, and debounces the LINE (never the counter, never the panic).

#[test]
fn config_defaults_pin_the_v11224_hardening() {
    let cfg = ClockMonitorConfig::default();
    assert_eq!(cfg.servers_per_poll, 3, "ask several servers per poll");
    assert_eq!(
        cfg.breach_consecutive_threshold, 3,
        "one noisy sample must not print a drift line"
    );
    assert_eq!(
        cfg.breach_log_interval,
        Duration::from_secs(600),
        "a persistent offset must not print every 30 s poll"
    );
}

fn breach(offset_us: i64) -> DriftVerdict {
    DriftVerdict::BreachThreshold {
        offset_us,
        rtt_us: 2_000,
        server: "test.ntp".to_string(),
        threshold_us: 10_000,
    }
}

#[test]
fn breach_count_increments_on_every_breach_but_the_log_is_debounced() {
    let monitor = ClockMonitor::new(ClockMonitorConfig {
        breach_consecutive_threshold: 3,
        breach_log_interval: Duration::from_secs(600),
        ..ClockMonitorConfig::default()
    });

    monitor.handle_verdict(&breach(-1_295_326));
    assert_eq!(
        monitor.breach_count(),
        1,
        "the counter is observability and must see EVERY breach"
    );
    assert_eq!(monitor.consecutive_breaches(), 1);

    monitor.handle_verdict(&breach(-1_295_326));
    monitor.handle_verdict(&breach(-1_295_326));
    assert_eq!(monitor.breach_count(), 3);
    assert_eq!(
        monitor.consecutive_breaches(),
        3,
        "the third consecutive breach is where the line is allowed to print"
    );
}

#[test]
fn a_clean_sample_resets_the_breach_streak() {
    let monitor = ClockMonitor::new(ClockMonitorConfig {
        breach_consecutive_threshold: 3,
        ..ClockMonitorConfig::default()
    });
    monitor.handle_verdict(&breach(-50_000));
    monitor.handle_verdict(&breach(-50_000));
    assert_eq!(monitor.consecutive_breaches(), 2);
    monitor.handle_verdict(&DriftVerdict::WithinThreshold {
        offset_us: 100,
        rtt_us: 1_000,
        server: "test.ntp".to_string(),
    });
    assert_eq!(
        monitor.consecutive_breaches(),
        0,
        "one clean sample must break the streak, so the next breach starts over"
    );
    assert_eq!(monitor.breach_count(), 2, "the counter is NOT reset");
}

#[test]
fn an_unusable_sample_breaks_the_streak_rather_than_extending_it() {
    let monitor = ClockMonitor::new(ClockMonitorConfig {
        breach_consecutive_threshold: 3,
        ..ClockMonitorConfig::default()
    });
    monitor.handle_verdict(&breach(-50_000));
    monitor.handle_verdict(&DriftVerdict::Unreliable {
        offset_us: -50_000,
        rtt_us: 5_000_000,
        server: "test.ntp".to_string(),
        max_rtt_us: 1_000_000,
    });
    assert_eq!(
        monitor.consecutive_breaches(),
        0,
        "an unusable sample says nothing about drift — it must not build a streak"
    );
    monitor.handle_verdict(&DriftVerdict::NetworkError {
        message: "unreachable".into(),
        retry_after: Duration::from_secs(30),
    });
    assert_eq!(monitor.consecutive_breaches(), 0);
}

#[test]
fn threshold_of_one_restores_legacy_immediate_logging() {
    // An operator who wants the old every-breach line sets the threshold to
    // 1 (or leaves the log interval at zero) — the debounce must be tunable
    // away, not hard-wired.
    let monitor = ClockMonitor::new(ClockMonitorConfig {
        breach_consecutive_threshold: 1,
        breach_log_interval: Duration::from_secs(0),
        ..ClockMonitorConfig::default()
    });
    for expected in 1..=4 {
        monitor.handle_verdict(&breach(-50_000));
        assert_eq!(monitor.consecutive_breaches(), expected);
    }
    assert_eq!(monitor.breach_count(), 4);
}

/// The lowest-RTT sample is the one whose offset uncertainty (≈ rtt/2) is
/// smallest, so it must be the one the monitor judges.
#[test]
fn lowest_rtt_sample_is_the_one_recorded() {
    let slow = ClockSample {
        offset_us: -1_295_326,
        rtt_us: 64_514,
        server: "pool.ntp.org".to_string(),
        measured_at_ms: 0,
    };
    let fast = ClockSample {
        offset_us: 120,
        rtt_us: 3_200,
        server: "time.cloudflare.com".to_string(),
        measured_at_ms: 0,
    };
    assert!(fast.rtt_us < slow.rtt_us);

    let threshold_us = 10_000;
    let max_rtt_us = 1_000_000;
    // The legacy first-answer-wins loop would have judged the 64 ms sample and
    // reported a 1.3 s breach.
    assert!(matches!(
        verdict_from_sample(&slow, threshold_us, max_rtt_us),
        DriftVerdict::BreachThreshold { .. }
    ));
    // v11.12.24 judges the fast one instead: no breach.
    assert!(matches!(
        verdict_from_sample(&fast, threshold_us, max_rtt_us),
        DriftVerdict::WithinThreshold { .. }
    ));
}
