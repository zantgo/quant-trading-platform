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
    echo "  run                Run the engine in the foreground with live logs (web mode)"
    echo "  run-silent         Run the engine in the background, redirecting logs to $LOG_FILE"
    echo "  run-cli            Run the terminal monitor (--mode cli, observe-only, no web server)"
    echo "  stop               Stop any background engine instance currently running"
    echo "  status             Check if the engine is running (and print process info)"
    echo "  test               Run all test suites (core → golden → indicators → engine → ui → doc)"
    echo "  test-core          Pure indicator math + serialization (core-domain, market-analyzer, config-models)"
    echo "  test-engine        DB, server, failover (engine crates)"
    echo "  test-engine-full   Engine suite including load/stress test"
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

run_foreground() {
    if [ ! -d "$FRONTEND_DIR/dist" ]; then
        echo "⚠️  Frontend build missing. Triggering compilation first..."
        build
    fi
    echo "🚀 Starting Trading Platform in the foreground..."
    cargo run --bin execution-daemon -- --web
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
    echo "🚀 Building engine..."
    cargo build 2>&1 | tail -2
    if [ ! -x target/debug/execution-daemon ]; then
        echo "❌ Build failed — engine not started."
        exit 1
    fi
    echo "📝 Logs will be written to: $LOG_FILE"
    nohup target/debug/execution-daemon $mode_args > "$LOG_FILE" 2>&1 &
    echo $! > "$PID_FILE"
    echo "✅ Engine running under PID: $! (daemon, not cargo)"
}

run_silent() {
    if [ ! -d "$FRONTEND_DIR/dist" ]; then
        echo "⚠️  Frontend build missing. Triggering compilation first..."
        build
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
    echo "   🖥️  No web server — the L7 overview redraws in your terminal"
    echo "   💾 Add --save to the daemon args to enable snapshot-export JSON dumps"
    cargo run --bin execution-daemon -- --mode cli
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
    cargo test -p core-domain -p market-analyzer -p config-models
}

test_engine() {
    echo "🦀 TEST-ENGINE: Running database-storage + api-gateway + portfolio-supervisor + performance-analytics + network-adapters + execution-daemon..."
    cargo test -p database-storage -p api-gateway -p portfolio-supervisor -p performance-analytics -p network-adapters -p execution-daemon
}

test_indicators() {
    echo "🦀 TEST-INDICATORS: Per-indicator pipeline e2e with terminal console reporting..."
    echo "    Each indicator (37 candle-based) is exercised through calculator → normalizer → signal deriver → lifecycle builder"
    echo "    with synthesized OHLCV candles in 4 market patterns (uptrend, downtrend, range, volatile)."
    echo "    Failures surface duplicate (label, kind) signal pairs that would trigger each_key_duplicate in the UI."
    cargo test -p market-analyzer --test indicator_pipeline_e2e -- --nocapture --test-threads=1
}

test_engine_full() {
    echo "🦀 TEST-ENGINE-FULL: Running all engine tests including load/stress..."
    cargo test --workspace -- --include-ignored
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
    run-silent)
        run_silent
        ;;
    run-cli)
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
