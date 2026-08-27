# BTE Layer 2 — Historical Runner (deep-history multi-symbol simulation)

**Version:** 11.0 (2026-08-26)
**Engine:** Backtesting Engine
**Code:** `crates/backtesting-engine/src/historical.rs`

## 1. What it replays

The historical runner reproduces the **entire MME pipeline** over
archived candles — no re-implementation:

```text
candle_archive (per symbol × per TF windows, burn-in inclusive)
  → warm_indicators_for_timeframe     (the SAME warm path the live daemon
                                       boots through — full per-candle
                                       snapshots: 52 indicators, normalized
                                       maps, market context)
  → synthesize_cross_tf               (the SAME pure MTF synthesizer the
                                       live L4/L5 assembly calls)
     + DecisionContext::compute       (the same decision assembly)
  → run_tick                          (the SAME per-tick session body as
                                       paper/live — fills, sizing, safety)
```

## 2. Pipeline details

1. **Window loading** — per symbol × ladder TF, `query_archive_window`
   loads `[from − burn_in, to]` where `burn_in = warmup_bars × max(tf_secs)`
   (default 300 bars × the ladder's macro TF). The burn-in only warms
   indicator windows; it produces no decisions and no trades.

2. **Exact MME parameters (v8.1 fidelity)** — the run builds the per-slot
   `TimeframeConfig`s (micro / fast / slow / macro, with the same
   registry fallbacks) and the `ActiveSet` from the global + per-instance
   `[activation]` union exactly like `registry::add_instance` /
   `registry/pipelines.rs` — so the replay uses the same indicator
   periods, weights and activation toggles the live MME uses.

3. **Chunked warm replay** — the warm path is CPU-bound (~50–60 ms per
   candle); the platform replays in chunks of 800 candles with a
   300-candle overlap (every indicator lookback in this platform is
   ≤ 300 bars, so chunk tails are mathematically identical to a
   continuous replay). Per-TF warms run in parallel on the tokio
   blocking pool.

4. **MTF synthesis** — per symbol's tick, the latest aligned snapshot
   from every ladder TF feeds `synthesize_cross_tf` (same state
   carry-over of score/regime/volume-dim/bias as live). The unsigned
   3-factor confluence blend and `DecisionContext::compute` mirror the
   live assembly exactly.

5. **Multi-symbol replay clock (v8.2)** — the entry series are each
   symbol's **smallest ladder TF** completed snapshots, filtered to
   `[from, to]`, merged k-way into one globally timestamp-ordered event
   stream. Each event ticks **only that symbol's** aligned TF set through
   the shared executor + engine (state keyed by symbol), so the
   multi-instance live architecture is reproduced exactly.

6. **Replay parity (v8.2)** — the synthesized snapshot (opportunity,
   decision context, analysis, advisory) drives `run_tick` with a virtual
   `candle_ts` and:

   - a **simulated `SafetyManager` per instance** fed by replayed equity
     (the soft gate blocks new entries in `DRAWDOWN_STOP`/`SUSPENDED`
     exactly like paper),
   - **funding settlement at simulated 8h boundaries**
     (`funding_rate_8h × notional`),
   - **end-of-run force-close**: open positions are closed at the final
     replayed candle close with `exit_reason = "end_of_backtest"`, so the
     ledger and trade statistics are complete.

7. **Progress + cancel (v8.2)** — the run reports phase progress
   (`fetching → warming → replaying → analyzing` with % at warm-chunk and
   replay-loop boundaries) through a `RunProgress` callback, and checks a
   cancel flag at the loop head and between warms. The run executes as a
   spawned task; cancel aborts cleanly with no partial persistence.

## 3. Honest limitations

- **No derivatives** — the archive stores no order-book / OI / funding /
  liquidation data; `synthesize_cross_tf` runs with `liquidity_flow =
  None`, `cluster = None`, `signals = []`. The synthesized decision is
  candle-based; live decisions additionally see the derivatives layers.
- **Candle-granular ticks** — the replay ticks once per completed candle
  of the smallest ladder TF; the live loop ticks every second. Fills
  evaluate at candle close (no intrabar path). This boundary is part of
  the parity contract (08-04 §3).
- **Determinism** — no wall clock, no randomness in the executor path;
  Monte Carlo is seeded (42). Identical inputs ⇒ identical outputs
  (bit-identical reruns are a test invariant).
- **Cost** — a 7-day window at the default ladder is minutes of CPU
  (parallel warms); runs execute asynchronously under the global run lock.

## 4. Coverage + ceiling validation

`POST /api/backtest/run` (mode `historical`) fails with
`400 not_enough_data` when any symbol's ladder TF lacks archive rows in
`[from − burn_in, to]` — the response carries the per-TF coverage detail
so the operator can shrink the depth or run a backfill first. For
Hyperliquid, the **5,000-candle ceiling** is validated per TF
(`max_candles_per_tf × tf_secs`); a depth beyond the ceiling fails with a
clear message naming the limiting TF.

## 5. Cross-references

- [BTE overview](08-01-bte-overview.md)
- [Archive & backfill](08-02-archive-and-backfill.md)
- [Parity contract](08-04-parity-contract.md)
