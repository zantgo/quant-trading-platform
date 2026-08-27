# DIE Candle Pipeline State Machine

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Specified — target of record (implementation status: README §Feature Status)
**Engine:** Data Infrastructure Engine (DIE)
**Owner:** market-analyzer + portfolio-supervisor

---

## §1 Purpose

Defines the operational lifecycle of one per-timeframe candle pipeline. Each `TimeframePipeline` (micro / fast / slow / macro) is a state machine; its state is published on every emitted `MarketSnapshot` so the dashboard, the frontend charts, and the cross-TF synthesis layer can all reason about whether the pipeline's outputs are trustworthy, warming up, or unusable.

The state machine is **per-TF**, not per-instance. A reload of the micro pipeline does not change the fast / slow / macro pipelines' states (CB-11 in [08-08](../../operations-and-compliance/08-08-candle-buffer-spec.md)).

## §2 Frozen decisions (DCP-01 … DCP-15)

| ID | Decision |
|----|----------|
| **DCP-01** | New enum **`CandlePipelineState`** with exactly five values: **`INITIALIZING`, `LOADING`, `LIVE`, `STALE`, `FAILED`**. All serialization uses SCREAMING_SNAKE_CASE matching the corpus convention. |
| **DCP-02** | `INITIALIZING` is a transient state from `TimeframePipeline` construction to the moment bootstrap returns. No `MarketSnapshot` is emitted in this state — there is nothing to broadcast yet. |
| **DCP-03** | `LOADING` is entered immediately after bootstrap returns. The pipeline now has at least zero candles and may emit shadow `MarketSnapshot`s with `is_completed = false`; completed snapshots begin to flow as soon as the first candle closes. Every indicator in the emitted `MarketSnapshot.indicator_lifecycle` map is `Loading` (or `Failed` if a calculator raised — see DCP-08). |
| **DCP-04** | `LIVE` is entered when the buffer has reached the live floor `max(candle_buffer.size / 10, 50)` bars (PRI-05 — the v6.10.7 formula; earlier revisions claimed the full `size`) **and** the parent `ConnectionStatus` is `Connected` (DCP-09). All 52 indicators in `indicator_lifecycle` must be either `Live` or `Stale`; if any indicator is `Loading` or `Failed` the pipeline cannot be `Live`. |
| **DCP-05** | `STALE` is entered when the pipeline was `Live` and then no completed candle has been emitted for `candle_buffer.stale_threshold_secs` (default 300 s, configurable). The pipeline auto-recovers to `LIVE` on the next completed candle. Reconstruction candles count toward `STALE → LIVE` recovery. |
| **DCP-06** | `FAILED` is entered on any of: (a) the parent `ConnectionStatus` reports `Failed` for more than the supervisor's `FailedThreshold` (5 cycles per [08-03](../../operations-and-compliance/08-03-connection-resilience.md) §Retry Budgets), (b) bootstrap returned an error and a cold restart was elected, (c) the per-indicator aggregator in DCP-10 reports any non-self-recoverable `Failed` indicator. |
| **DCP-07** | The pipeline state is published on every emitted `MarketSnapshot` via the new `tf.pipeline_state` field. The history endpoint (`GET /api/history`) returns the state of the **most recent** snapshot, plus the state of every historical snapshot in the response window — so a UI reconnection can paint accurate per-bar status badges on historical candles. |
| **DCP-08** | A calculator panic / `Err` return propagates: the indicator transitions to `Failed` ([03-02-15](../market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md) ILS-04), and the pipeline transitions to `FAILED` only if the indicator cannot self-recover. Self-recovery means: a subsequent successful calculator update flips the indicator back to `Live`; until that happens, the parent pipeline is `FAILED`. |
| **DCP-09** | The parent `ConnectionStatus` (see [03-01-01 §4.2](03-01-01-die-overview-spec.md)) is **conjunctive** with the pipeline state. While the parent is not `Connected`, the pipeline is at best `STALE` (still has history, but no fresh data); once `Connected` the pipeline resumes `LIVE` if it was `STALE`, or stays `FAILED` until the operator reloads. |
| **DCP-10** | **Per-indicator aggregation rule** for the pipeline-level state: the pipeline's state is the **most-severe** state across all 52 indicators in the `indicator_lifecycle` map. Severity ordering is `FAILED > STALE > LOADING > LIVE`. A single `Failed` indicator that is not self-recoverable forces the pipeline to `FAILED`. |
| **DCP-11** | `INITIALIZING → LOADING` happens the moment bootstrap returns, regardless of whether the buffer is full. A sub-minute pipeline in `LOADING` with a 0-candle buffer is a valid state (CB-05, CB-06). |
| **DCP-12** | `LOADING → LIVE` happens when the buffer reaches `candle_buffer.size` (DCP-04). Reconstructed candles count toward `bars_seen` but are **not enough on their own** — at least `candle_buffer.size` of true live candles must also be present (CB-06). |
| **DCP-13** | `STALE → LIVE` happens on the next completed candle of any kind (live or reconstructed). |
| **DCP-14** | `FAILED → LOADING` happens only via the `reload_timeframe` API ([08-08 §5](../../operations-and-compliance/08-08-candle-buffer-spec.md)) — operator action, never automatic. |
| **DCP-15** | Transitions write a row to a new `candle_pipeline_state_events` SQLite table (`06-02` §DB). Active-table count 26 → 27. The table mirrors `instance_lifecycle_events` ([03-03-06 §5](../trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md)) in shape. |

## §3 State machine

```
                          construct
                             │
                             ▼
                      ┌────────────────┐
                      │ INITIALIZING   │ (transient, no emissions)
                      └───────┬────────┘
                       bootstrap returns
                              │
                              ▼
                      ┌────────────────┐
                      │   LOADING      │◄──────── reload_timeframe ────┐
                      └────┬──────┬────┘                                │
                  buffer   │      │ DCP-08 calculator failure           │
                  full     │      ▼                                     │
                  (DCP-04) │  ┌────────┐                                │
                           │  │ FAILED │────reload_timeframe─────────────┤
                           │  └────────┘                                │
                           ▼                                             │
                      ┌────────────────┐                                 │
            ┌────────►│     LIVE       │                                 │
            │         └───────┬────────┘                                 │
   next candle              │ no candle for                             │
   (live or                 │ stale_threshold_secs                     │
   reconstructed,           ▼                                          │
   DCP-13)            ┌────────────────┐                                │
            └─────────│     STALE     │──── reload_timeframe ───────────► LOADING
                      └────────────────┘
```

### Transition table

| # | From | To | Trigger | Side effects |
|---|------|----|---------|--------------|
| 1 | — | INITIALIZING | `TimeframePipeline::new()` | none |
| 2 | INITIALIZING | LOADING | bootstrap returns (any result) | first shadow snapshot may be emitted |
| 3 | LOADING | LIVE | `buffer.len() == candle_buffer.size` AND parent `ConnectionStatus = Connected` AND all 52 indicators ≥ `Live` (DCP-04, DCP-10, CB-12) | `candle_pipeline_state_events` row; UI badge flips blue |
| 4 | LOADING | FAILED | non-self-recoverable calculator panic (DCP-08) OR bootstrap elected cold-fail (DCP-06) | `candle_pipeline_state_events` row; UI shows grey badge; reload required |
| 5 | LIVE | STALE | no completed candle for `stale_threshold_secs` (DCP-05) | `candle_pipeline_state_events` row; UI shows amber badge |
| 6 | STALE | LIVE | next completed candle (live OR reconstructed, DCP-13) | `candle_pipeline_state_events` row; UI badge flips blue |
| 7 | any | FAILED | parent `ConnectionStatus = Failed` for > `FailedThreshold` (DCP-06, DCP-09) | `candle_pipeline_state_events` row; snapshot channel may lag |
| 8 | FAILED | LOADING | `reload_timeframe` operator action (DCP-14) | full tear-down + rebuild per [08-08 §5](../../operations-and-compliance/08-08-candle-buffer-spec.md) |
| 9 | LIVE / STALE / FAILED | LOADING | operator TF change (CB-11) | single-TF tear-down + rebuild |

## §4 Configuration schema

```toml
# config.toml — pipeline state behavior
[candle_buffer]
size = 500
stale_threshold_secs = 300                # DCP-05 / DCP-13
```

`stale_threshold_secs` is the only new tunable. The parent `ConnectionStatus` threshold (DCP-06) lives in `[adapters.<exchange>]` per the existing reconnect-resilience spec.

## §5 Database changes (`06-02`, DCP-15)

```sql
-- candle_pipeline_state_events — per-TF pipeline state transition audit
CREATE TABLE IF NOT EXISTS candle_pipeline_state_events (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  instance_id    TEXT NOT NULL,
  symbol         TEXT NOT NULL,
  timeframe_slot TEXT NOT NULL
                 CHECK (timeframe_slot IN ('micro','fast','slow','macro')),
  from_state     TEXT CHECK (from_state IS NULL OR
                 from_state IN ('INITIALIZING','LOADING','LIVE','STALE','FAILED')),
  to_state       TEXT NOT NULL
                 CHECK (to_state IN ('INITIALIZING','LOADING','LIVE','STALE','FAILED')),
  trigger        TEXT NOT NULL
                 CHECK (trigger IN (
                   'bootstrap_return','buffer_full','stale_threshold',
                   'connection_failed','reload','timeframe_change',
                   'calculator_panic'
                 )),
  bars_seen      INTEGER NOT NULL,
  bars_required  INTEGER NOT NULL,
  timestamp_ms   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pipeline_state_events_lookup
  ON candle_pipeline_state_events(instance_id, timeframe_slot, timestamp_ms DESC);
```

Active-table count in `06-02 §3` changes **26 → 27**.

The `market_snapshots` table also gains two columns for per-row state visibility:

```sql
ALTER TABLE market_snapshots ADD COLUMN pipeline_state TEXT
  CHECK (pipeline_state IS NULL OR pipeline_state IN ('LOADING','LIVE','STALE','FAILED'));
ALTER TABLE market_snapshots ADD COLUMN indicator_lifecycle TEXT
  CHECK (indicator_lifecycle IS NULL OR json_valid(indicator_lifecycle));
```

`pipeline_state` is set on every persisted snapshot; `indicator_lifecycle` is the JSON serialization of the `IndicatorLifecycleStatus` map ([03-02-15](../market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md)). Both columns are absent on rows written before this migration; the SQLite migration marks them nullable and the read paths treat `NULL` as `Loading` for backward compatibility.

## §6 Interaction matrix

| Situation | Result |
|-----------|--------|
| `ConnectionStatus = Failed` for > `FailedThreshold` cycles | Pipeline transitions `LIVE/STALE/LOADING → FAILED` (DCP-07, DCP-09); the indicator `bars_seen` freezes; reconstruction does not run while parent is `Failed` |
| `ConnectionStatus = Reconnecting` | Pipeline stays in current state (does not auto-degrade to `STALE`); the parent of reconstruction decisions |
| Bootstrap returns empty Vec (≥1min TF, exchange had no history) | Pipeline → `LOADING` and stays `LOADING` until buffer fills (live candles count toward the cap); eventually `LIVE` |
| Sub-minute bootstrap returns empty Vec (CB-05) | Pipeline → `LOADING` with 0 candles; same path as above |
| Indicator self-recovers after `Failed` | Indicator → `Live`; if it was the only `Failed` indicator, pipeline transitions `FAILED → STALE` (not directly to `LIVE`) and then `STALE → LIVE` on the next completed candle (DCP-13) |
| Operator reload during `LIVE` | Pipeline transitions `LIVE → LOADING` (DCP-09 transition row #9); UI shows amber loading badges during the rebuild |
| Two indicators fail simultaneously | Pipeline → `FAILED`; both must self-recover before pipeline → `LIVE` (DCP-10 severity rule) |
| `InstanceState = STOPPED` | Pipeline state is published on the most recent snapshot but the channel is paused (existing behavior); `STOPPED → RUNNING` does not reload pipelines |

**Scoped-enum rule.** This state machine is **per-TF pipeline scope**. It is distinct from the `InstanceLifecycle` enum in [03-03-06](../trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md) (per-instance scope), the `ConnectionStatus` enum in [03-01-01](03-01-01-die-overview-spec.md) (per-adapter scope), and the `ReconnectState` enum in [08-03](../../operations-and-compliance/08-03-connection-resilience.md) (per-reconnect-loop scope). On first use in any document section, qualify the scope ("TF pipeline LOADING", "instance STOPPED", "adapter Connected").

## §7 Implementation work items

Tracked in `docs/CHANGELOG.md §Open Items` with `AUDIT-V7-NN` identifiers.

- `AUDIT-V7-310` — `core-domain`: introduce `CandlePipelineState` enum + `IndicatorLifecycleStatus` map type; extend `MarketSnapshot` with `pipeline_state` + `indicator_lifecycle` fields.
- `AUDIT-V7-311` — `database-storage`: migration `00XX_add_candle_pipeline_state_events.sql` + `00XX_alter_market_snapshots.sql`; bump `user_version`.
- `AUDIT-V7-312` — `market-analyzer`: in `TimeframePipeline`, track `pipeline_state`; transition on every bootstrap return, on every completed candle (DCP-04/DCP-13), on stale-timer tick (DCP-05), on connection-status callback (DCP-09).
- `AUDIT-V7-313` — `portfolio-supervisor`: implement `reload_timeframe` API + cascade transitions per CB-11.
- `AUDIT-V7-314` — `api-gateway`: add `POST /api/instances/:instance_id/reload?slot=`; extend `/api/history` to include per-row `pipeline_state` and `indicator_lifecycle`.

## §8 Cross-References

- [08-08 Candle Buffer Spec](../../operations-and-compliance/08-08-candle-buffer-spec.md) — the master contract (CB-01 … CB-12).
- [03-01-07 DIE Historical Fetch Policy](03-01-07-die-historical-fetch-policy.md) — exchange-independent fetch contract (HFP-01 … HFP-10).
- [03-01-01 DIE Overview](03-01-01-die-overview-spec.md) — `ConnectionStatus` (§4.2) interaction.
- [03-01-04 DIE Layer 3 — Data Quality](03-01-04-die-layer3-data-quality.md) — quality dimensions + `INSUFFICIENT_DATA` linkage.
- [03-02-15 MME Indicator Lifecycle States](../market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md) — per-indicator lifecycle (ILS-01 … ILS-15).
- [03-03-06 TAE Instance Lifecycle](../trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md) — orthogonal lifecycle axis (IL-06).
- [08-03 Connection Resilience](../../operations-and-compliance/08-03-connection-resilience.md) — `ReconnectState` interaction.
- [08-04 Candle Reconstruction](../../operations-and-compliance/08-04-candle-reconstruction.md) — gap-fill source.
- [06-02 Database Schema](../../integration-and-api/06-02-database-schema-spec.md) — `market_snapshots` table.