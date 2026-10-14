//! AUDIT-C2 regression: the per-symbol WS supervisor must NEVER terminate.
//!
//! Before this fix, `registry/pipelines.rs` counted *any* connection cycle as
//! a failure unless the session had survived 300 s, and on the 5th consecutive
//! strike it called `set_disabled` and `break` — exiting the supervisor task.
//! Nothing re-spawns that task, so any ~30 s network event (laptop
//! suspend/resume, a VPN drop, a Wi-Fi flap, a venue 429 burst) permanently
//! killed every configured instance: charts frozen on the last bar, every
//! indicator still reporting "Live", `/api/system/pipelines` reporting healthy,
//! one log line, and a daemon restart as the only recovery.
//!
//! Two properties are locked here:
//!   1. A cycle that survives `connect_grace` is a SUCCESS — it clears both the
//!      strike counter and the backoff. A socket that connects and drops every
//!      10 s must therefore accrue NO strikes at all.
//!   2. The breaker never terminates the loop. It returns `Cooldown` and the
//!      supervisor retries. There is no action that means "give up".

use std::time::Duration;

use portfolio_supervisor::registry::pipelines::{BreakerAction, WsBreaker};

/// The shipped `[resilience] connect_grace_ms` default.
fn default_grace() -> Duration {
    Duration::from_millis(2000)
}

#[test]
fn rapid_failures_trip_the_breaker_but_never_terminate_it() {
    let mut breaker = WsBreaker::new(default_grace());

    // Four rapid failures (each returns before the connect grace elapses).
    for i in 1..=4 {
        let action = breaker.on_cycle_end(Duration::from_millis(50));
        assert!(
            matches!(action, BreakerAction::Retry(_)),
            "cycle {i} must be a plain retry, got {action:?}"
        );
    }

    // The fifth trips the breaker — and the action is a Cooldown, NOT a
    // termination. This is the assertion the old `break` would fail: there is
    // no variant of `BreakerAction` that means "give up".
    let action = breaker.on_cycle_end(Duration::from_millis(50));
    match action {
        BreakerAction::Cooldown(d) => {
            assert_eq!(
                d,
                Duration::from_secs(300),
                "cooldown must be the documented 300 s"
            );
        }
        other => panic!("5th rapid failure must trip the breaker, got {other:?}"),
    }

    // ...and the loop is still usable afterwards. The counter reset, so the
    // supervisor resumes normal backoff retries rather than being stuck.
    assert_eq!(
        breaker.consecutive_failures(),
        0,
        "counter resets after a cooldown so retries resume"
    );
    assert!(matches!(
        breaker.on_cycle_end(Duration::from_millis(50)),
        BreakerAction::Retry(_)
    ));
}

#[test]
fn a_connection_that_survives_connect_grace_is_a_success() {
    let mut breaker = WsBreaker::new(default_grace());

    // Four rapid failures to build up near-trip state.
    for _ in 0..4 {
        breaker.on_cycle_end(Duration::from_millis(50));
    }
    assert_eq!(breaker.consecutive_failures(), 4);

    // Now a cycle that genuinely connected and ran for 30 s — comfortably past
    // the grace window. This MUST clear the counter.
    let action = breaker.on_cycle_end(Duration::from_secs(30));
    assert!(matches!(action, BreakerAction::Retry(_)));
    assert_eq!(
        breaker.consecutive_failures(),
        1,
        "a session surviving connect_grace is a success and clears strikes"
    );
}

#[test]
fn a_flapping_socket_never_accrues_strikes() {
    // The exact scenario that killed the supervisor before: the venue accepts
    // the connection, it runs for 10 s, then drops — forever. The old rule
    // ("reset only after 300 s") counted each of those as a failure, so after
    // ~5 flaps the supervisor died.
    let mut breaker = WsBreaker::new(default_grace());

    for flap in 0..200 {
        let action = breaker.on_cycle_end(Duration::from_secs(10));
        match action {
            BreakerAction::Retry(_) => {}
            BreakerAction::Cooldown(_) => panic!(
                "flapping socket tripped the breaker on flap {flap} — \
                 a connection that lived 10 s is a success, not a failure"
            ),
        }
    }
}

#[test]
fn genuinely_dead_venue_cools_down_and_recovers() {
    // The inverse: every cycle fails instantly. The breaker should trip
    // repeatedly (proving the supervisor keeps running) but interleave with
    // retries rather than escalating to a terminal state.
    let mut breaker = WsBreaker::new(default_grace());

    let mut cooldowns = 0;
    let mut retries = 0;
    for _ in 0..25 {
        match breaker.on_cycle_end(Duration::from_millis(1)) {
            BreakerAction::Cooldown(_) => cooldowns += 1,
            BreakerAction::Retry(_) => retries += 1,
        }
    }

    assert!(cooldowns > 0, "a dead venue must trip the breaker");
    assert!(retries > 0, "a dead venue must still see plain retries");
    // 25 cycles / 5 strikes = 5 breaker trips (the last 5 cycles leave one
    // partial group, hence >= 4).
    assert!(
        cooldowns >= 4,
        "the breaker must keep re-arming instead of terminating, got {cooldowns} cooldowns"
    );
}

#[test]
fn backoff_grows_then_saturates_and_never_hot_spins() {
    let mut breaker = WsBreaker::new(Duration::from_secs(3600)); // never a success

    let mut prev = Duration::ZERO;
    for i in 0..4 {
        let action = breaker.on_cycle_end(Duration::from_millis(1));
        let BreakerAction::Retry(d) = action else {
            panic!("expected retry on cycle {i}")
        };
        // Never a hot-spin: every delay is at least ~1 s even at the floor.
        assert!(
            d >= Duration::from_millis(800),
            "backoff must never hot-spin, got {d:?}"
        );
        if i > 0 {
            assert!(d >= prev, "backoff must be monotonically non-decreasing");
        }
        prev = d;
    }
}

#[test]
fn zero_connect_grace_does_not_divide_or_panic() {
    // Defensive: a misconfigured `[reilience] connect_grace_ms = 0` must not
    // make every cycle a silent success (which would defeat the breaker) nor
    // panic. We treat 0 as "any non-zero uptime counts".
    let mut breaker = WsBreaker::new(Duration::ZERO);

    // A cycle with literally no uptime is still a failure.
    breaker.on_cycle_end(Duration::ZERO);
    assert!(breaker.consecutive_failures() >= 1);

    // Any real uptime is a success.
    breaker.on_cycle_end(Duration::from_millis(1));
    assert_eq!(breaker.consecutive_failures(), 1);
}

/// AUDIT-M15: `[reconnect] initial_backoff_ms` / `max_backoff_ms` were
/// accepted, validated, documented and served by the platform-config API — and
/// then ignored, because the supervisor hardcoded 1 s → 30 s and
/// `ReconnectPolicy` had no production caller at all.
///
/// An operator raising `initial_backoff_ms` to 10000 to avoid tripping a
/// venue's connection rate limit got 1 s, with no warning.
#[test]
fn the_configured_backoff_ladder_is_honoured() {
    use network_adapters::adapters::resilience::ReconnectPolicy;
    use portfolio_supervisor::registry::pipelines::WsBreaker;

    // An operator who wants a slow, polite reconnect.
    let policy = ReconnectPolicy {
        initial_backoff: Duration::from_secs(10),
        max_backoff: Duration::from_secs(120),
        jitter_pct: 0.0, // deterministic for the assertion
        max_attempts: None,
    };
    let mut breaker = WsBreaker::with_policy(Duration::from_secs(3600), policy); // never a success

    let delays: Vec<Duration> = (0..4)
        .map(|_| match breaker.on_cycle_end(Duration::from_millis(1)) {
            BreakerAction::Retry(d) => d,
            BreakerAction::Cooldown(_) => panic!("unexpected trip within 4 cycles"),
        })
        .collect();

    assert_eq!(
        delays[0],
        Duration::from_secs(10),
        "the FIRST retry must use the configured initial_backoff_ms (was hardcoded 1s)"
    );
    assert_eq!(delays[1], Duration::from_secs(20), "ladder must double");
    assert_eq!(delays[2], Duration::from_secs(40));
    assert_eq!(delays[3], Duration::from_secs(80));
    assert!(
        delays.iter().all(|d| *d >= Duration::from_secs(10)),
        "no retry may be shorter than the configured initial backoff"
    );
}

#[test]
fn the_backoff_ladder_saturates_at_the_configured_maximum() {
    use network_adapters::adapters::resilience::ReconnectPolicy;
    use portfolio_supervisor::registry::pipelines::WsBreaker;

    let policy = ReconnectPolicy {
        initial_backoff: Duration::from_secs(5),
        max_backoff: Duration::from_secs(20),
        jitter_pct: 0.0,
        max_attempts: None,
    };
    let mut breaker = WsBreaker::with_policy(Duration::from_secs(3600), policy);

    let mut saw_cap = false;
    for _ in 0..12 {
        match breaker.on_cycle_end(Duration::from_millis(1)) {
            BreakerAction::Retry(d) => {
                assert!(
                    d <= Duration::from_secs(20),
                    "must respect max_backoff: {d:?}"
                );
                if d == Duration::from_secs(20) {
                    saw_cap = true;
                }
            }
            BreakerAction::Cooldown(_) => break,
        }
    }
    assert!(saw_cap, "the ladder must reach the configured ceiling");
}

/// A pathological config must not be able to hot-spin the supervisor.
#[test]
fn a_zero_or_tiny_backoff_config_cannot_hot_spin() {
    use network_adapters::adapters::resilience::ReconnectPolicy;
    use portfolio_supervisor::registry::pipelines::WsBreaker;

    for (initial, maxb) in [
        (Duration::ZERO, Duration::ZERO),
        (Duration::from_millis(1), Duration::from_millis(1)),
        (Duration::ZERO, Duration::from_secs(30)),
    ] {
        let policy = ReconnectPolicy {
            initial_backoff: initial,
            max_backoff: maxb,
            jitter_pct: 1.0, // worst case: full jitter
            max_attempts: None,
        };
        let mut breaker = WsBreaker::with_policy(Duration::from_secs(3600), policy);
        for _ in 0..4 {
            let BreakerAction::Retry(d) = breaker.on_cycle_end(Duration::from_millis(1)) else {
                continue;
            };
            assert!(
                d >= Duration::from_millis(500),
                "backoff must never hot-spin, got {d:?} from {initial:?}/{maxb:?}"
            );
        }
    }
}
