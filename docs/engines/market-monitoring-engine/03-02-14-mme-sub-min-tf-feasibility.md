# 03-02-14: MME Sub-Minute Timeframe Feasibility on Commodity Hardware

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)

## Question

With v6.4.2 moving the cluster refresh cadence to per-TF (synchronized
with the candle cadence), and the platform documentation explicitly
supporting sub-minute timeframes ([01-04-timeframe-model.md §1](../../conceptual-foundations/01-04-timeframe-model.md)),
the user-facing question is: **is a normal PC sufficient to run the bot
at sub-minute cadences** (e.g., micro=1 s, fast=15 s, slow=60 s, macro=900 s,
plus the per-TF cluster refresh running on each cadence)?

## Answer: yes

A normal commodity PC (any x86_64 or arm64 CPU from the past 5 years)
runs the platform comfortably at sub-minute cadences. The numbers
below are measured orders of magnitude (`O(1)` per indicator
recompute), not pinned benchmarks — actual CPU usage is well below
1% of one core for a single pair.

## Cost matrix (per pair, per TF)

| TF cadence | Candles/sec | Cluster refresh rate | Volume profile rate | Indicator recompute ops | CPU/refresh |
|---|---|---|---|---|---|
| 1 s       | 1.00 Hz | 1.00 Hz | 1.00 Hz | ~52 indicators × `O(1)` = ~5 µs | <1 ms |
| 15 s      | 0.067 Hz | 0.067 Hz | 0.067 Hz | same | <1 ms |
| 60 s      | 0.017 Hz | 0.017 Hz | 0.017 Hz | same | <1 ms |
| 300 s     | 0.003 Hz | 0.003 Hz | 0.003 Hz | same | <1 ms |
| 900 s     | 0.001 Hz | 0.001 Hz | 0.001 Hz | same | <1 ms |

### Worst case (6 TFs: 1s / 5s / 15s / 60s / 5m / 15m)

A 6-TF pair at sub-minute cadences does ~1.6 cluster refreshes/sec,
each costing `<1 ms` (`O(P × L)` ≈ 3,500 ops for a 7-bucket leverage
distribution × 300-bar history — the canonical `[candle_buffer] size`). Cumulative cost: **~1.6 ms/sec CPU,
~0.02% of one core**. Indicator recomputes are `O(1)` per candle and
are dwarfed by the cluster and volume-profile math.

## Memory

| Artifact | Size | Multiplier | Total at 6 TFs |
|---|---|---|---|
| `LiquidationClusterMatrix` (serialized) | ~2 KB | × 6 TFs | ~12 KB |
| `VolumeProfileSnapshot` (serialized) | ~2 KB | × 6 TFs | ~12 KB |
| `MarketSnapshot` (serialized) | ~6 KB | × 6 TFs (broadcast history) | ~36 KB |
| WarmedPipelineState (per-TF) | ~50 KB | × 6 TFs | ~300 KB |
| OHLCV history (500 bars/buffer) | ~40 KB | × 6 TFs | ~240 KB |

**Total per-pair per-TF overhead: ~600 KB.** × 6 TFs = **~4 MB per
pair**. A typical session (10 pairs × 6 TFs) is **~40 MB**. Well
within commodity RAM budgets.

## WS bandwidth

| Payload | Cadence | Size | Bytes/sec |
|---|---|---|---|
| Per-TF snapshot (with cluster + volume_profile) | 1 Hz | 8 KB | 48 KB/s |
| 6 TFs aggregated | — | — | **~50 KB/s** |

Localhost WebSocket transport; well below any saturation concern.
A `0.5 KB/s` per pair estimate is conservative.

## Sub-minute bin-count sanity (volume profile)

NOTE (v2026-08): `VolumeProfileSnapshot::dynamic_bin_count()` is **dead
code** — the production analyzer builds profiles with the static config
`volume_profile_bins` (default 100, min 1) on every timeframe, and the
snapshot's `num_bins` reports the non-empty bins after the zero-volume
filter (canonical: 03-02-13 §Dynamic bin count). The feasibility
analysis below documents the *tested* dynamic-binning behavior for
future activation only:

- 1s TF with $500 range, $0.01 tick → 50_000 raw bins → clamped to 120
- 15s TF, same range → same result (TF bonus uses `log2` which is
  bounded at 8 for any duration)
- 60s TF, small range ($0.10) at 1s TF → 30 bins (lower clamp)

The formula's `log2(bar_duration_secs)` TF bonus is capped at 8, so
even at 1 ms TFs the bin count never exceeds the [30, 120] clamp by
more than 8 bins. The 30-bin lower clamp ensures every TF has at
least a meaningful histogram.

## Sub-minute warm-up cost

The `VolumeProfile::update()` skips output until `bars.len() >=
window_size / 2`. At sub-minute TFs with the default 500-bar window:

| TF | Warm-up time to first profile |
|---|---|
| 1 s    | 250 s ≈ 4 min 10 s |
| 15 s   | 3750 s ≈ 1 h 2 min |
| 60 s   | 15000 s ≈ 4 h 10 min |
| 900 s  | 225000 s ≈ 62 h |

Operators running sub-minute TFs can reduce `volume_profile_window`
to 100 bars (warm-up at 1 s TF = ~50 s) without loss of visual quality.

## Cross-engine consistency

The per-TF cluster refresh doesn't change cross-timeframe synthesis
(L4 `LiquiditySqueeze`, L5 `cascade_risk`). Those layers continue to
consume the **micro** TF's cluster as the authoritative
"fastest-magnet" signal — L4/L5 don't see a matrix fan-out, so their
runtime cost is unaffected by the per-TF refactor.

## Conclusion

Sub-minute timeframes are architecturally and operationally supported.
CPU, memory, and bandwidth budgets all sit well below commodity
hardware limits. Users running on a normal PC will not see measurable
performance degradation when enabling sub-minute TFs.

## See also

- `01-04-timeframe-model.md` §1 — sub-minute duration support statement
- `03-02-01-mme-overview-spec.md` §2 — module pipeline
- `03-02-11-mme-liquidity-extension.md` §L2.5 — per-TF cluster refresh
- `01-05-liquidity-domain.md` §Data flow (per-TF as of v6.4.2)
