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

    match verdict_from_sample(&sample, 50) {
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
        verdict_from_sample(&sample, 50),
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

    match verdict_from_sample(&sample, 50) {
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
        verdict_from_sample(&sample, 50),
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
