//! AUDIT-M22: `ConnectionScope` must actually bound the reporter's lifetime.
//!
//! The type's own doc comment promises "the reporter stops when it is
//! dropped", but there was no `Drop` impl: the `CancellationToken` was simply
//! forgotten and the spawned task sat on its 60 s `tick` forever.
//!
//! The reconnect loop holds its scope in `_liveness`, so every reconnect
//! produced one more permanently-live task, each pinning its `channels` vec and
//! the per-channel `reported` map for the life of the process. Nothing in the
//! test suite could see it — the task was idle, not misbehaving — and the
//! symptom only appears as a slow RSS climb across reconnects.

use std::time::Duration;

use network_adapters::adapters::diagnostics::{ChannelTracker, ConnectionScope};

fn tracker(label: &'static str) -> ChannelTracker {
    ChannelTracker::new(label)
}

#[tokio::test]
async fn dropping_the_scope_cancels_the_reporter_token() {
    let scope = ConnectionScope::spawn_channel_liveness(
        "TestVenue",
        "BTC-USDC".to_string(),
        vec![tracker("trades"), tracker("l2Book")],
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let token = scope.token();
    assert!(
        !token.is_cancelled(),
        "a live scope must not report itself cancelled"
    );

    drop(scope);

    assert!(
        token.is_cancelled(),
        "AUDIT-M22 regression: dropping the scope must cancel its token, or \
         the reporter task leaks forever"
    );
}

#[tokio::test]
async fn the_reporter_stops_promptly_rather_than_at_the_next_tick() {
    let scope = ConnectionScope::spawn_channel_liveness(
        "TestVenue",
        "ETH-USDC".to_string(),
        vec![tracker("trades")],
        Duration::from_secs(60),
        Duration::from_secs(60),
    );
    let token = scope.token();
    drop(scope);

    // The reporter selects on `cancelled()` and on a 60 s tick. If Drop only
    // stopped the token indirectly, the task could linger until the tick.
    // Cancellation must be observable immediately.
    tokio::select! {
        _ = token.cancelled() => {}
        _ = tokio::time::sleep(Duration::from_millis(250)) => {
            panic!("token cancellation was not observable within 250ms");
        }
    }
}

#[tokio::test]
async fn a_scope_that_is_still_alive_keeps_reporting() {
    // Guards against an over-eager fix that cancels on clone or on spawn.
    let scope = ConnectionScope::spawn_channel_liveness(
        "TestVenue",
        "SOL-USDC".to_string(),
        vec![tracker("trades")],
        Duration::from_secs(60),
        Duration::from_secs(60),
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !scope.token().is_cancelled(),
        "a retained scope must stay live"
    );
}
