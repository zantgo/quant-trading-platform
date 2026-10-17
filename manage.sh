#!/usr/bin/env bash

# ==============================================================================
# Trading Platform - Workspace Management Script
# ==============================================================================
#
# Workspace architecture: see docs/conceptual-foundations/01-06-crate-layout-
# and-cycles.md for the canonical crate inventory, dependency graph, and
# cycle-breaking design rationale (single source of truth).
# Configuration: `config.toml` (canonical; read by config-models::load_config()).
# Binary: `cargo run --bin execution-daemon -- --web`
# ==============================================================================

set -euo pipefail

# Configuration
LOG_FILE="engine.log"
FRONTEND_DIR="ui"
PID_FILE=".engine.pid"

# v10.1: per-folder sessions — the HTTP port resolves as
# PLATFORM_PORT env → `[server] port` in config.toml → 3000. Each folder
# runs its own daemon on its own port; `stop`/`status` target the folder's
# own port as a fallback when the PID file is missing.
resolve_port() {
    local cfg_port
    cfg_port=$(sed -n '/^\[server\]/,/^\[/p' config.toml 2>/dev/null | grep -E '^\s*port\s*=' | head -1 | sed -n 's/.*port\s*=\s*\([0-9]\+\).*/\1/p' || true)
    echo "${PLATFORM_PORT:-${cfg_port:-3000}}"
}
PORT="$(resolve_port)"
export PLATFORM_PORT="${PLATFORM_PORT:-$PORT}"

show_help() {
    echo "Trading Platform - CLI Management Tool"
    echo "Usage: ./manage.sh [command]"
    echo ""
    echo "Commands:"
    echo "  build              Compile frontend assets and verify cargo workspace compiles"
    echo "  build-release      Build the release daemon binary (what run uses)"
    echo "  run                Run the engine in the foreground with live logs (web mode)"
    echo "  run-silent         Run the engine in the background, redirecting logs to $LOG_FILE"
    echo "  shell              Boot the observe shell (--mode cli, no web server) — the canonical CLI"
    echo "  run-cli            Alias for `shell`"
    echo "  stop               Stop any background engine instance currently running"
    echo "  status             Check if the engine is running (and print process info)"
    echo "  test               Run all test suites (core → golden → indicators → engine → ui → doc)"
    echo "  test-core          Pure indicator math + serialization (core-domain, market-analyzer, config-models)"
    echo "  test-engine        Engine crates: hermetic run, then live-venue stage if LIVE_VENUE=1"
    echo "  test-engine-offline Hermetic engine tests only (no network, deterministic)"
    echo "  test-engine-live   Only the tests that reach live exchange APIs"
    echo "  test-engine-full   Engine suite including load/stress + live venue"
    echo "  test-ui            Svelte 5 visual state & component tests"
    echo "  test-indicators    Per-indicator pipeline e2e with console reporting"
    echo "  test-property      Generative property tests across indicators"
    echo "  test-doc           Documentation corpus consistency checks (Phases 8/9/10 gate)"
    echo "  e2e-backtest       v8.2 backtest matrix harness (headless CLI cases, exchange-aware)"
    echo "  lint               Run cargo fmt --check + clippy (correctness lints) + svelte-check"
    echo "  lint-fix           Run cargo fmt + cargo clippy --fix (mechanical fixes only)"
    echo "  clean              Delete build targets, dependencies, and temporary locks"
    echo "  destroy            Stop the engine, run clean, and permanently delete telemetry.db"
    echo "  help               Show this helper documentation"
    echo ""
}

build() {
    echo "📦 Building Svelte 5 Frontend..."
    (cd "$FRONTEND_DIR" && bun install --frozen-lockfile && bun run build)

    echo "🦀 Verifying Rust Workspace Compilation..."
    cargo check
    echo "✅ Build completed successfully."
}

# v11.12.25 (memory): run the RELEASE binary, not the debug build.
#
# The debug profile is not a small difference: `run_single` compiles to a ~2 MB
# state machine (see the Box::pin note in registry/pipelines.rs), so a 10-instance
# / 10-duration workspace carries ~200 MB of pure debug-build penalty across 100
# pipelines — on a 2.75 GiB host that is the difference between running and being
# OOM-killed. `manage.sh test*` still uses debug (fast iteration); only the
# run path changes.
RELEASE_BIN="target/release/execution-daemon"

# v11.12.25 (memory): glibc reads these before the Rust runtime allocates, so
# they must be in the environment, not set from inside main(). Without them a
# boot spike ratchets RSS to its peak and never returns it: glibc keeps up to
# 8x-ncores per-thread arenas and only trims a whole arena at a time.
export MALLOC_ARENA_MAX="${MALLOC_ARENA_MAX:-2}"
export MALLOC_TRIM_THRESHOLD_="${MALLOC_TRIM_THRESHOLD_:-131072}"

build_release() {
    echo "🦀 Building release daemon (a debug build costs ~200 MB at 10 instances)..."
    cargo build --release --bin execution-daemon
}

run_foreground() {
    if [ ! -d "$FRONTEND_DIR/dist" ]; then
        echo "⚠️  Frontend build missing. Triggering compilation first..."
        build
    fi
    # Always ask cargo, never trust the binary on disk. This is incremental and
    # a fast no-op when nothing changed, whereas `[ -x $RELEASE_BIN ]` silently
    # ran STALE code after an edit — so a fix could land and the operator would
    # still see the old bug ("I rebuilt and it is still broken"). `start_daemon`
    # has always done this.
    echo "🦀 Building release daemon if stale (a debug build costs ~200 MB at 10 instances)..."
    cargo build --release --bin execution-daemon || exit 1
    echo "🚀 Starting Trading Platform in the foreground (release build)..."
    exec "$RELEASE_BIN" --web
}

rotate_log() {
    # M4 (production audit): engine.log previously grew unbounded (per-candle
    # console lines, clock polls, reconnect logs) — rotate at 50 MB, keep 3.
    if [ -f "$LOG_FILE" ] && [ "$(du -m "$LOG_FILE" | cut -f1)" -ge 50 ]; then
        echo "🔄 Rotating $LOG_FILE (>50 MB)..."
        rm -f "$LOG_FILE.3"
        mv "$LOG_FILE.2" "$LOG_FILE.3" 2>/dev/null || true
        mv "$LOG_FILE.1" "$LOG_FILE.2" 2>/dev/null || true
        mv "$LOG_FILE" "$LOG_FILE.1"
    fi
}

# M4 (production audit): record the DAEMON pid, not the `cargo run` wrapper.
# `cargo run` spawns the binary as a child; killing the wrapper orphaned the
# daemon on port 3000 (next start then panicked at bind). We build first and
# exec the binary directly so $! IS the daemon.
start_daemon() {
    local mode_args="$1"
    rotate_log
    echo "🚀 Building engine (release — a debug build costs ~200 MB at 10 instances)..."
    cargo build --release --bin execution-daemon 2>&1 | tail -2
    if [ ! -x "$RELEASE_BIN" ]; then
        echo "❌ Release build failed — engine not started."
        exit 1
    fi
    echo "📝 Logs will be written to: $LOG_FILE"
    nohup "$RELEASE_BIN" $mode_args > "$LOG_FILE" 2>&1 &
    echo $! > "$PID_FILE"
    echo "✅ Engine running under PID: $! (daemon, not cargo)"
}

run_silent() {
    if [ ! -d "$FRONTEND_DIR/dist" ]; then
        echo "⚠️  Frontend build missing. Triggering compilation first..."
        build
    fi

    if [ -x "$RELEASE_BIN" ]; then
        :
    else
        echo "🦀 Building release engine (debug costs ~200 MB at 10 instances)..."
        cargo build --release --bin execution-daemon 2>&1 | tail -2
    fi
    if [ -f "$PID_FILE" ]; then
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            echo "⚠️  Engine is already running in the background (PID: $PID)."
            exit 0
        fi
    fi

    start_daemon "--web"
}

run_cli() {
    if [ ! -d "$FRONTEND_DIR/dist" ]; then
        echo "⚠️  Frontend build missing (not required for CLI mode — the web UI is skipped entirely)."
    fi

    echo "🚀 Starting Trading Platform in CLI mode (terminal monitor)..."
    echo "   🔧 Observe-only session — markets + signals, no orders dispatched"
    echo "   📡 Instances from the interactive launch prompt (pre-filled from config.toml)"
    echo "   🖥️  No web server — the observe shell renders the same AppState the WebSocket handler serves"
    echo "   💾 Add --save to the daemon args to enable snapshot-export JSON dumps"
    # Release, like the other two run paths: `cargo run` would hand back a DEBUG
    # binary, and the debug pipeline carries the same ~200 MB at 10 instances
    # (see the `Box::pin` note in registry/pipelines.rs).
    cargo build --release --bin execution-daemon || exit 1
    exec "$RELEASE_BIN" --mode cli
}

stop_instance() {
    # v11.3 smart port: drop the resolved-endpoint marker on stop.
    rm -f ".server.port"
    if [ -f "$PID_FILE" ]; then
        PID=$(cat "$PID_FILE")
        echo "🛑 Stopping background instance (PID: $PID)..."
        if kill "$PID" 2>/dev/null; then
            # M4: SIGTERM now triggers the daemon's graceful shutdown (K4);
            # give it up to 10 s to drain the telemetry queue, then SIGKILL.
            for _ in $(seq 1 10); do
                if ! kill -0 "$PID" 2>/dev/null; then break; fi
                sleep 1
            done
            if kill -0 "$PID" 2>/dev/null; then
                echo "⚠️  Graceful shutdown timed out — forcing kill."
                kill -9 "$PID" 2>/dev/null || true
            fi
            rm -f "$PID_FILE"
            echo "✅ Engine stopped."
        else
            echo "⚠️  Process $PID not found. Cleaning stale PID file."
            rm -f "$PID_FILE"
        fi
    else
        # Fallback to kill cargo/engine processes on this port if no pid file is present
        PORT_PID=$(lsof -t -i:"$PORT" || true)
        if [ -n "$PORT_PID" ]; then
            echo "🛑 Found engine running on port $PORT (PID: $PORT_PID). Stopping..."
            # `lsof -t` may return multiple PIDs separated by newlines; expand them so
            # `kill` receives each PID as a separate argument instead of a single
            # newline-containing string which `kill` would reject.
            kill $PORT_PID
            echo "✅ Engine stopped."
        else
            echo "ℹ️  No running instances detected."
        fi
    fi
}

check_status() {
    # v11.3 smart port: the daemon publishes its resolved bind:port to
    # `.server.port` on boot — trust it over the configured $PORT when present.
    if [ -f ".server.port" ]; then
        PORT_FILE=$(cat "$ROOT/.server.port")
        FILE_PORT="${PORT_FILE##*:}"
        FILE_PID=$(lsof -t -i:"$FILE_PORT" 2>/dev/null || true)
        if [ -n "$FILE_PID" ]; then
            echo "🟢 Engine status: RUNNING on $PORT_FILE (PID: $FILE_PID)"
            echo "📝 Log file size: $(du -sh "$LOG_FILE" | cut -f1)"
            return 0
        fi
    fi
    if [ -f "$PID_FILE" ]; then
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            echo "🟢 Engine status: RUNNING (PID: $PID)"
            echo "📝 Log file size: $(du -sh "$LOG_FILE" | cut -f1)"
            return 0
        fi
    fi

    PORT_PID=$(lsof -t -i:"$PORT" || true)
    if [ -n "$PORT_PID" ]; then
        echo "🟢 Engine status: RUNNING on port $PORT (PID: $PORT_PID)"
        return 0
    fi

    echo "🔴 Engine status: STOPPED"
    return 1
}

run_tests() {
    local failures=0
    echo "═══════════════════════════════════════════════════════════"
    echo "  STAGE 1/6: TEST-CORE — Pure math, indicators, serialization"
    echo "═══════════════════════════════════════════════════════════"
    test_core || { failures=$((failures + 1)); echo "❌ TEST-CORE failed"; }
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  STAGE 2/6: TEST-GOLDEN — Golden-vector conformance"
    echo "═══════════════════════════════════════════════════════════"
    test_golden || { failures=$((failures + 1)); echo "❌ TEST-GOLDEN failed"; }
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  STAGE 3/6: TEST-INDICATORS — Per-indicator e2e"
    echo "═══════════════════════════════════════════════════════════"
    test_indicators || { failures=$((failures + 1)); echo "❌ TEST-INDICATORS failed"; }
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  STAGE 4/6: TEST-ENGINE — DB + server + e2e"
    echo "═══════════════════════════════════════════════════════════"
    test_engine || { failures=$((failures + 1)); echo "❌ TEST-ENGINE failed"; }
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  STAGE 5/6: TEST-UI — Svelte 5 components, state, snapshots"
    echo "═══════════════════════════════════════════════════════════"
    test_ui || { failures=$((failures + 1)); echo "❌ TEST-UI failed"; }
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  STAGE 6/6: TEST-DOC — Documentation corpus consistency"
    echo "═══════════════════════════════════════════════════════════"
    test_doc || { failures=$((failures + 1)); echo "❌ TEST-DOC failed"; }
    echo ""
    if [ $failures -eq 0 ]; then
        echo "✅ All 6 test suites passed"
    else
        echo "❌ $failures test suite(s) failed"
        return 1
    fi
}

test_core() {
    echo "🦀 TEST-CORE: Running core-domain + market-analyzer + config-models..."
    cargo test $CARGO_BUILD_ARGS --no-fail-fast -p core-domain -p market-analyzer -p config-models -- $CARGO_TEST_ARGS
}

# ── v11.13.1: the engine suite is split by network dependency ────────────────
#
# WHY. Two independent problems, both of which cost real debugging time on a
# small host:
#
# 1. `cargo test` ABORTS the whole run at the first failing test binary. Packages
#    run in the order listed, so one flaky test in `api-gateway` silently
#    prevented `network-adapters` and `execution-daemon` from EVER executing.
#    That is how a Bybit adapter defect sat unverified: the very crate holding
#    the fix never ran, because an unrelated Hyperliquid timeout stopped the
#    run twenty binaries earlier. `--no-fail-fast` makes a failure in one crate
#    unable to hide the results of another.
#
# 2. The suite is not hermetic. Several tests reach live exchange REST/WS, so
#    their outcome depends on a third party's latency. They are now behind the
#    `LIVE_VENUE=1` gate (matching the existing `BYBIT_LIVE` idiom), which makes
#    the default run deterministic on ANY machine, not just a small one.
#
# `test-engine` runs BOTH stages offline-first, then the network stage only if
# asked. `test-engine-offline` is the gate a change must pass; `test-engine-live`
# is for re-verifying wire assumptions against the venues.
ENGINE_PKGS="database-storage api-gateway portfolio-supervisor performance-analytics network-adapters execution-daemon"
# Engine packages whose tests are hermetic (no live venue calls).
ENGINE_OFFLINE_PKGS="database-storage portfolio-supervisor performance-analytics network-adapters execution-daemon"
# Engine packages carrying live-venue tests, gated behind LIVE_VENUE=1.
ENGINE_LIVE_PKGS="api-gateway network-adapters"

# libtest thread count. Defaults to min(nproc, 4): each test thread drives a full
# Axum + SQLite app state, and the default (nproc) saturates memory on a 2.8 GB
# host. Override with TEST_THREADS=1 to serialize completely.
: "${TEST_THREADS:=4}"
if [ "$(nproc 2>/dev/null || echo 4)" -lt "$TEST_THREADS" ]; then
    TEST_THREADS=$(nproc 2>/dev/null || echo 4)
fi
CARGO_TEST_ARGS="--test-threads=$TEST_THREADS"

# Compile jobs. Bound separately from TEST_THREADS because they dominate peak
# memory: linking one api-gateway test binary pulls ~600 MB of rust-lld, and
# 25 of them at full nproc drove a 2.8 GB host to ~50 MB free mid-compile.
# Defaults to the test bound; override with BUILD_JOBS=n.
: "${BUILD_JOBS:=$TEST_THREADS}"
if [ "$(nproc 2>/dev/null || echo 4)" -lt "$BUILD_JOBS" ]; then
    BUILD_JOBS=$(nproc 2>/dev/null || echo 4)
fi
CARGO_BUILD_ARGS="--jobs=$BUILD_JOBS"

test_engine_offline() {
    echo "🦀 TEST-ENGINE-OFFLINE: hermetic engine tests (no network)..."
    echo "    packages: $ENGINE_OFFLINE_PKGS"
    echo "    libtest threads: $TEST_THREADS (override with TEST_THREADS=n)"
    echo "    --no-fail-fast: a failure in one package cannot hide another package's results."
    cargo test $CARGO_BUILD_ARGS --no-fail-fast -p database-storage -p portfolio-supervisor \
        -p performance-analytics -p network-adapters -p execution-daemon \
        -- $CARGO_TEST_ARGS
}

test_engine_live() {
    echo "🌐 TEST-ENGINE-LIVE: tests that reach live exchange APIs..."
    echo "    packages: $ENGINE_LIVE_PKGS"
    echo "    enabling: BYBIT_LIVE=1 + --include-ignored (unlocks the #[ignore]d"
    echo "              venue-contract tests, e.g. network-adapters/bybit_live_contract)"
    # The live network tests are double-gated: a Rust `#[ignore]` attribute AND a
    # BYBIT_LIVE=1 env read inside the test body. BOTH are required — the env
    # var alone is inert because cargo skips ignored tests, and --include-ignored
    # alone runs them in a "skipping" no-op. An earlier version of this function
    # set LIVE_VENUE, which nothing in the tree reads, so the "live" stage
    # silently re-ran the hermetic suite.
    BYBIT_LIVE=1 cargo test $CARGO_BUILD_ARGS --no-fail-fast -p api-gateway -p network-adapters \
        -- --include-ignored $CARGO_TEST_ARGS
}

test_engine() {
    echo "🦀 TEST-ENGINE: full engine suite (offline first, then live venue)..."
    local rc=0
    test_engine_offline || rc=1
    echo
    if [ "${LIVE_VENUE:-0}" = "1" ]; then
        test_engine_live || rc=1
    else
        echo "ℹ️  Skipping the live-venue stage (set LIVE_VENUE=1 to include it)."
        echo "    This keeps the default run deterministic — no third-party latency."
    fi
    return $rc
}

test_indicators() {
    echo "🦀 TEST-INDICATORS: Per-indicator pipeline e2e with terminal console reporting..."
    echo "    Each indicator (37 candle-based) is exercised through calculator → normalizer → signal deriver → lifecycle builder"
    echo "    with synthesized OHLCV candles in 4 market patterns (uptrend, downtrend, range, volatile)."
    echo "    Failures surface duplicate (label, kind) signal pairs that would trigger each_key_duplicate in the UI."
    cargo test $CARGO_BUILD_ARGS --no-fail-fast -p market-analyzer --test indicator_pipeline_e2e -- --nocapture --test-threads=1
}

test_engine_full() {
    echo "🦀 TEST-ENGINE-FULL: All engine tests including load/stress + live venue..."
    LIVE_VENUE=1 test_engine
}

test_property() {
    echo "🦀 TEST-PROPERTY: Running generative property tests across all indicators..."
    cargo test -p market-analyzer --test property_ema_sma --test property_rsi --test property_macd --test property_adx --test property_bollinger_atr --test property_squeeze --test property_bbwp --test property_fibonacci --test property_divergence --test property_patterns
}

test_golden() {
    echo "🦀 TEST-GOLDEN: Running golden-vector conformance tests (AUDIT-AIU Phase 10)..."
    cargo test -p market-analyzer --test golden_vectors
}

test_ui() {
    echo "🧪 TEST-UI: Running Svelte 5 frontend Vitest tests..."
    if [ ! -d "$FRONTEND_DIR/node_modules" ]; then
        echo "📦 ui/node_modules missing — installing frontend dependencies..."
        (cd "$FRONTEND_DIR" && bun install --frozen-lockfile)
    fi
    (cd "$FRONTEND_DIR" && bun run test)
}

clean_workspace() {
    echo "🧹 Cleaning cargo workspace targets..."
    cargo clean
    echo "🧹 Removing frontend dependencies and builds..."
    rm -rf "$FRONTEND_DIR/node_modules"
    rm -rf "$FRONTEND_DIR/dist"
    rm -f "$PID_FILE"
    rm -f "$LOG_FILE"
    # Note: bun.lock is intentionally preserved so CI's
    # `bun install --frozen-lockfile` continues to work without a
    # network round-trip. Use `./manage.sh destroy` for a full reset.
    echo "✅ Workspace clean."
}

destroy_all() {
    echo "🛑 Stopping any active background or running instances..."
    stop_instance

    echo "🧹 Executing standard workspace cleanup..."
    clean_workspace

    echo "💥 Permanently deleting SQLite database and journal files..."
    rm -f "telemetry.db"
    rm -f "telemetry.db-journal"
    rm -f "telemetry.db-shm"
    rm -f "telemetry.db-wal"

    # The platform uses config.toml (single source of truth).
    # `./manage.sh destroy` resets config.toml from the bundled
    # config.default.toml template.
    rm -f "config.toml"
    if [ -f "config.default.toml" ]; then
        echo "⚙️  Restoring config.toml from config.default.toml template..."
        cp "config.default.toml" "config.toml"
    else
        echo "❌ Error: config.default.toml is missing! Cannot restore default configuration."
        exit 1
    fi

    echo "✨ Absolutely everything has been purged and destroyed."
}

test_doc() {
    echo "📋 TEST-DOC: Running documentation corpus consistency checks..."
    python3 scripts/check_docs.py
}

e2e_backtest() {
    echo "🧪 TEST-E2E-BACKTEST: Running the v8.2 backtest matrix harness..."
    bash scripts/e2e-backtest-matrix.sh "$@"
}

lint() {
    local failures=0
    echo "═══════════════════════════════════════════════════════════"
    echo "  LINT 1/3: cargo fmt --check"
    echo "═══════════════════════════════════════════════════════════"
    cargo fmt --all -- --check || { failures=$((failures + 1)); echo "❌ cargo fmt check failed"; }
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  LINT 2/3: cargo clippy (correctness lints)"
    echo "═══════════════════════════════════════════════════════════"
    cargo clippy --workspace --all-targets --no-deps -- \
        -D clippy::await_holding_lock \
        -D static_mut_refs \
        -D clippy::items_after_test_module \
        || { failures=$((failures + 1)); echo "❌ cargo clippy failed"; }
    echo ""
    if [ -d "$FRONTEND_DIR" ]; then
        echo "═══════════════════════════════════════════════════════════"
        echo "  LINT 3/3: svelte-check (svelte + tsc)"
        echo "═══════════════════════════════════════════════════════════"
        (cd "$FRONTEND_DIR" && bun run check) \
            || { failures=$((failures + 1)); echo "❌ svelte-check failed"; }
    fi
    echo ""
    if [ $failures -eq 0 ]; then
        echo "✅ All lint checks passed"
    else
        echo "❌ $failures lint check(s) failed"
        return 1
    fi
}

lint_fix() {
    echo "🛠  LINT-FIX: running cargo fmt + cargo clippy --fix"
    cargo fmt --all
    cargo clippy --workspace --all-targets --no-deps --fix --allow-dirty -- \
        -D clippy::await_holding_lock \
        -D static_mut_refs \
        -D clippy::items_after_test_module \
        || { echo "❌ cargo clippy --fix failed"; return 1; }
    if [ -d "$FRONTEND_DIR" ]; then
        echo "🛠  LINT-FIX: bun run check --watch is interactive; run 'bun run check' manually in ui/."
    fi
}

# Main routing logic
if [ $# -eq 0 ]; then
    show_help
    exit 0
fi

case "$1" in
    build)
        build
        ;;
    run)
        run_foreground
        ;;
    build-release)
        build_release
        ;;
    run-silent)
        run_silent
        ;;
    shell|run-cli)
        run_cli
        ;;
    stop)
        stop_instance
        ;;
    status)
        check_status
        ;;
    test)
        run_tests
        ;;
    test-core)
        test_core
        ;;
    test-engine)
        test_engine
        ;;
    test-engine-offline)
        test_engine_offline
        ;;
    test-engine-live)
        test_engine_live
        ;;
    test-indicators)
        test_indicators
        ;;
    test-engine-full)
        test_engine_full
        ;;
    test-property)
        test_property
        ;;
    test-golden)
        test_golden
        ;;
    test-ui)
        test_ui
        ;;
    test-doc)
        test_doc
        ;;
    e2e-backtest)
        e2e_backtest "${@:2}"
        ;;
    lint)
        lint
        ;;
    lint-fix)
        lint_fix
        ;;
    clean)
        clean_workspace
        ;;
    destroy)
        destroy_all
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        echo "❌ Error: Unknown command '$1'"
        show_help
        exit 1
        ;;
esac
