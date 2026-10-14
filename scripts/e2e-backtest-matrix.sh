#!/usr/bin/env bash
# scripts/e2e-backtest-matrix.sh — v8.2 backtest matrix harness.
#
# Runs headless CLI backtests across timeframe ladders × archive depths
# with exchange-aware expectations (Bitget paginated; Hyperliquid's
# 5,000-candle ceiling), plus negative cases. Per case: exit code + JSON
# envelope + sqlite invariants (equity conservation, exit-reason
# vocabulary, burn-in respected, window bounds) via
# scripts/e2e_backtest_verify.py. One determinism double-run.
#
# Usage: ./manage.sh e2e-backtest [case-filter]
# Env:   E2E_BIN (default ./target/release/execution-daemon)
#        E2E_DB  (default ./telemetry.db)
#        E2E_TIMEOUT (per-case timeout, default 6h)
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BIN="${E2E_BIN:-$ROOT/target/release/execution-daemon}"
DB="${E2E_DB:-$ROOT/telemetry.db}"
TIMEOUT="${E2E_TIMEOUT:-21600}"
REPORT="$ROOT/e2e-backtest-report.md"
JSON_REPORT="$ROOT/e2e-backtest-report.json"
FILTER="${1:-}"

# Warmup bars (from config.toml) drive the burn-in expectation.
WARMUP_BARS="$(grep -E '^warmup_bars' config.toml | head -1 | grep -oE '[0-9]+')"
WARMUP_BARS="${WARMUP_BARS:-300}"

declare -a CASES=()
# case name | exchange | symbol | tf ladder | depth | expected (pass|fail|fail:reason-substring)
add_case() {
    if [ -n "$FILTER" ] && [[ "$1" != *"$FILTER"* ]]; then return; fi
    CASES+=("$1|$2|$3|$4|$5|$6")
}

# ── Bitget (per-granularity retention ceilings — measured 2026-08-21:
#    1m–30m ≈ 30d, 1H ≈ 45d, 4H ≈ 180d, 12H/1D ≈ 365d) ──
add_case "bitget-L1-d1"   bitget BTC 60,180,300,900     1   "fail"
add_case "bitget-L1-d4"   bitget BTC 60,180,300,900     4   "pass"
add_case "bitget-L1-d7"   bitget BTC 60,180,300,900     7   "pass"
add_case "bitget-L1-d30"  bitget BTC 60,180,300,900     30  "pass"
add_case "bitget-L1-d90"  bitget BTC 60,180,300,900     90  "fail"
add_case "bitget-L1-d180" bitget BTC 60,180,300,900     180 "fail"
add_case "bitget-L1-d365" bitget BTC 60,180,300,900     365 "fail"
add_case "bitget-L2-d7"   bitget BTC 60,300,900,1800    7   "pass"
add_case "bitget-L2-d30"  bitget BTC 60,300,900,1800    30  "pass"
add_case "bitget-L2-d90"  bitget BTC 60,300,900,1800    90  "fail"
add_case "bitget-L3-d60"  bitget BTC 60,900,3600,14400  60  "fail"
add_case "bitget-L4-d14"  bitget BTC 180,300,900,3600   14  "pass"
add_case "bitget-L4-d30"  bitget BTC 180,300,900,3600   30  "pass"
add_case "bitget-L4-d90"  bitget BTC 180,300,900,3600   90  "fail"
add_case "bitget-L5-d180" bitget BTC 180,900,1800,43200 180 "fail"
add_case "bitget-L6-d365" bitget BTC 300,900,3600,86400 365 "fail"
add_case "bitget-L7-d60"  bitget BTC 900,1800,3600,14400 60 "fail"

# ── Hyperliquid (5,000-candle ceiling per TF) ──
add_case "hyperliquid-L1-d3"  hl BTC 60,180,300,900     3  "fail"
add_case "hyperliquid-L1-d4"  hl BTC 60,180,300,900     4  "fail"
add_case "hyperliquid-L7-d51" hl BTC 900,1800,3600,14400 51 "pass"
add_case "hyperliquid-L7-d53" hl BTC 900,1800,3600,14400 53 "fail"

# ── Bybit (depth-UNCAPPED: measured ~2,425 days retention on every interval,
#    far past the 365-day archive ceiling, so every depth inside the ceiling
#    PASSES — the opposite of Bitget's per-granularity retention table) ──
add_case "bybit-L1-d1"   bybit BTC 60,180,300,900     1   "pass"
add_case "bybit-L1-d7"   bybit BTC 60,180,300,900     7   "pass"
add_case "bybit-L1-d30"  bybit BTC 60,180,300,900     30  "pass"
add_case "bybit-L1-d365" bybit BTC 60,180,300,900     365 "pass"
add_case "bybit-L7-d180" bybit BTC 900,1800,3600,14400 180 "pass"
# Venue-specific negatives: HFP-03 has no Bybit sub-minute interval at all.
add_case "bybit-negative-sub-minute"      bybit BTC 15,60,300,900   7 "fail"
add_case "bybit-negative-non-ascending"   bybit BTC 900,60,300,180  7 "fail"
add_case "bybit-negative-burn-in"         bybit BTC 60,180,300,900  1  "fail"
# TradFi line lives in the same `category=linear` product — Bybit's headline
# differentiator, and one the Bitget/Hyperliquid cases cannot cover.
add_case "bybit-tradfi-XAU-d7" bybit XAU 60,180,300,900 7 "pass"

# ── Negatives (exchange-agnostic payloads) ──
add_case "negative-sub-minute"  bitget BTC 15,60,300,900  7 "fail"
add_case "negative-non-ascending" bitget BTC 900,60,300,180 7 "fail"
add_case "negative-burn-in" bitget BTC 60,180,300,900 1 "fail"

# ── Multi-symbol (2 instances) ──
add_case "bitget-multi-L1-d7" bitget "BTC,ETH" 60,180,300,900 7 "pass"

total=${#CASES[@]}
pass=0
fail=0
declare -a FAILED_CASES=()
declare -a ROWS=()

echo "🧪 E2E backtest matrix — $total cases"
echo "   binary: $BIN"
echo "   db:     $DB"
echo "   warmup: $WARMUP_BARS bars"
echo ""

run_case() {
    IFS='|' read -r name ex syms tf depth expected <<<"$1"
    echo "── $name (exchange=$ex symbols=$syms tf=$tf depth=$depth expected=$expected)"
    local start
    start=$(date +%s)

    local out
    out=$(timeout "$TIMEOUT" "$BIN" --backtest --exchange "$ex" --symbols "$syms" \
        --tf "$tf" --depth "$depth" --capital 1000 --allocation 10 2>".e2e-$name.stderr")
    local code=$?
    local status
    status=$(printf '%s' "$out" | sed -n 's/.*"status":"\([a-z]*\)".*/\1/p' | head -1)
    local run_id
    run_id=$(printf '%s' "$out" | sed -n 's/.*"run_id":\([0-9]*\).*/\1/p' | head -1)
    local dur=$(( $(date +%s) - start ))

    local ok=0
    if [ "$expected" = "pass" ]; then
        if [ "$code" -eq 0 ] && [ "$status" = "ok" ] && [ -n "$run_id" ]; then
            # sqlite invariants
            local to_secs from_secs scored min_tf burn_in
            to_secs=$(date +%s)
            scored=$(python3 - "$depth" "$WARMUP_BARS" "$tf" <<'PYEOF'
import sys
depth = int(sys.argv[1]); warmup = int(sys.argv[2])
tf = max(int(x) for x in sys.argv[3].split(','))
burn = warmup * tf
scored = depth * 86400 - burn
print(max(scored, 0))
PYEOF
)
            from_secs=$(( to_secs - scored ))
            min_tf=$(python3 - "$tf" <<'PYEOF'
import sys
print(min(int(x) for x in sys.argv[1].split(',')))
PYEOF
)
            burn_in=$(( WARMUP_BARS * $(python3 - "$tf" <<'PYEOF'
import sys
print(max(int(x) for x in sys.argv[1].split(',')))
PYEOF
) ))
            if python3 scripts/e2e_backtest_verify.py check "$DB" "$run_id" 1000 "$from_secs" "$to_secs" "$min_tf" "$burn_in" >>".e2e-$name.stderr" 2>&1; then
                ok=1
            fi
        fi
    else
        if [ "$code" -ne 0 ]; then ok=1; fi
    fi

    if [ "$ok" -eq 1 ]; then
        pass=$((pass + 1))
        echo "   ✅ PASS (exit=$code status=$status run=$run_id ${dur}s)"
        ROWS+=("{\"case\":\"$name\",\"result\":\"pass\",\"exit\":$code,\"run_id\":${run_id:-null},\"seconds\":$dur}")
    else
        fail=$((fail + 1))
        FAILED_CASES+=("$name")
        echo "   ❌ FAIL (exit=$code status=$status run=$run_id ${dur}s) — see .e2e-$name.stderr"
        tail -5 ".e2e-$name.stderr" 2>/dev/null | sed 's/^/      /'
        ROWS+=("{\"case\":\"$name\",\"result\":\"fail\",\"exit\":$code,\"run_id\":${run_id:-null},\"seconds\":$dur}")
    fi
}

for c in "${CASES[@]}"; do
    run_case "$c"
    echo ""
done

# ── Determinism double-run (bitget L1 × 7d; second run reuses the archive) ──
if [ -z "$FILTER" ] || [[ "determinism" == *"$FILTER"* ]]; then
    echo "── determinism (bitget-L1-d7 run twice)"
    "$BIN" --backtest --exchange bitget --symbols BTC --tf 60,180,300,900 --depth 7 \
        --capital 1000 --allocation 10 >/dev/null 2>&1
    id_a=$(python3 - "$DB" <<'PYEOF'
import sqlite3, sys
con = sqlite3.connect(sys.argv[1])
print(con.execute("SELECT MAX(id) FROM backtest_runs").fetchone()[0])
PYEOF
)
    "$BIN" --backtest --exchange bitget --symbols BTC --tf 60,180,300,900 --depth 7 \
        --capital 1000 --allocation 10 >/dev/null 2>&1
    id_b=$(python3 - "$DB" <<'PYEOF'
import sqlite3, sys
con = sqlite3.connect(sys.argv[1])
print(con.execute("SELECT MAX(id) FROM backtest_runs").fetchone()[0])
PYEOF
)
    if [ -n "$id_a" ] && [ -n "$id_b" ] && [ "$id_a" != "$id_b" ] \
        && python3 scripts/e2e_backtest_verify.py compare "$DB" "$id_a" "$id_b"; then
        pass=$((pass + 1))
        ROWS+=("{\"case\":\"determinism\",\"result\":\"pass\",\"exit\":0,\"run_id\":$id_b,\"seconds\":0}")
    else
        fail=$((fail + 1))
        FAILED_CASES+=("determinism")
        ROWS+=("{\"case\":\"determinism\",\"result\":\"fail\",\"exit\":0,\"run_id\":null,\"seconds\":0}")
    fi
fi

# ── Report ──
{
    echo "# E2E Backtest Matrix Report (v8.2)"
    echo ""
    echo "Generated: $(date -u '+%Y-%m-%d %H:%M UTC')"
    echo ""
    echo "| Result | Cases |"
    echo "|---|---|"
    echo "| PASS | $pass |"
    echo "| FAIL | $fail |"
    echo ""
    if [ ${#FAILED_CASES[@]} -gt 0 ]; then
        echo "## Failed cases"
        echo ""
        for f in "${FAILED_CASES[@]}"; do echo "- $f"; done
        echo ""
    fi
    echo "## Matrix rows"
    echo ""
    printf '%s\n' "${ROWS[@]}" | sed 's/^/  /'
} >"$REPORT"

printf '[%s]\n' "$(IFS=,; echo "${ROWS[*]}")" >"$JSON_REPORT"

echo "════════════════════════════════════════════"
echo "RESULT: $pass PASS / $fail FAIL — report: $REPORT"
if [ "$fail" -gt 0 ]; then
    exit 1
fi
exit 0
