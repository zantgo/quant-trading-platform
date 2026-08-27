# Instance Lifecycle (v7)

**Version:** 11.0 (2026-08-26) — v7: Gate-0 / stance / veto references removed; lifecycle now drives the Setup Executor directly.
**Status:** Specified — implemented (registry + lifecycle manager + automation conditions); v7 wiring in progress.
**Engine:** Trade Automation Engine (TAE)
**Owner:** portfolio-supervisor + execution-daemon

---

## §1 Frozen decisions (IL-01 … IL-15)

| ID | Decision |
|----|----------|
| **IL-01** | New enum **`LifecycleState`** with exactly four live values: **`RUNNING`, `PAUSED`, `STOPPING`, `STOPPED`**. Deletion is **not** a lifecycle state; it is an irreversible tombstone (IL-08). |
| **IL-02** | **`STOPPED`** = the instance holds **no positions and no open orders**, its executor loop is halted, and **all read/analytics surfaces remain fully available** (history, monitor, dashboards, PAE). A STOPPED instance can be **restarted** (`/start`) or **deleted**. |
| **IL-03** | **`STOPPING`** is a mandatory transitional state. A stop command moves RUNNING/PAUSED → STOPPING: entries hard-close immediately, **all open orders are cancelled and all positions are market-closed**. Transition to STOPPED occurs **only when zero positions and zero open orders are confirmed**. If flatten is incomplete after the configured timeout, the instance remains STOPPING and surfaces WARN. |
| **IL-04** | **`PAUSED`** = the entry gate is closed (no new setups, pending entries cancelled), but **the executor loop keeps running** and **existing positions continue to be managed and closed** (TP/SL/invalidation exits remain armed). |
| **IL-05** | The Setup Executor admits entries only when `lifecycle_state = RUNNING`. Exits (bracket fills, signal-flip close, stop flatten) are never blocked by lifecycle. Blocked entries are logged in the automation activity log with reason `lifecycle`. |
| **IL-06** | Per-instance axes are **`LifecycleState` × `safety_state`** (informational). Entries require `RUNNING` and safety state ∉ {`DRAWDOWN_STOP`, `SUSPENDED`}. |
| **IL-07** | Three commands — **start / pause / stop** — map to endpoints: `POST /api/instances/:instance_id/start`, `POST /api/instances/:instance_id/pause`, `POST /api/instances/:instance_id/stop`. **Manual commands are always available regardless of automation configuration** (operator supremacy). |
| **IL-08** | **Deletion is manual-only, irreversible, requires STOPPED.** `DELETE /api/instances/:instance_id` returns `409` for RUNNING/PAUSED/STOPPING instances. Delete writes a `deleted_at_ms` tombstone; telemetry rows are retained. |
| **IL-09** | **Creation is manual-only** (`POST /api/instances` or UI create bar). No rule, schedule, or automation may create or delete an instance. |
| **IL-10** | Initial state at creation: created **without** a start condition → **RUNNING immediately**. Created **with** an `automation.start` condition → **STOPPED (armed)**. Column default is `STOPPED` (fail-closed). |
| **IL-11** | Automation configuration schema — three optional condition objects (`start`, `pause`, `stop`), each with optional keys `at_price_above`, `at_price_below`, `at_time` (RFC3339 UTC), `after_duration_secs` (pause/stop only). Multiple keys inside one condition are **OR** (first to fire wins). |
| **IL-12** | Evaluation & arming: price conditions evaluate on DIE mid-price ticks; time conditions on the disciplined wall clock. `after_duration_secs` measured from **most recent transition into RUNNING** (`entered_state_at_ms`). Each condition is **one-shot latching**: after firing it is inert until operator edits it (any edit re-arms that condition). Same-tick collisions resolve **stop > pause > start**. Saving a past `at_time` is rejected `422`. |
| **IL-13** | Persistence: two tables — `instance_lifecycle` (registry) and `instance_lifecycle_events` (every transition). |
| **IL-14** | Stop-during-safety-block proceeds (flatten is emergency path). Start-during-DRAWDOWN_STOP is allowed (RUNNING) but the safety soft gate still blocks entries until the state clears. |
| **IL-15** | UI: instance rows carry a lifecycle badge (RUNNING / PAUSED / STOPPING-flashing / STOPPED); Stop is danger-styled; STOPPED instances keep every analytics page fully navigable; deleted instances vanish. |

---

## §2 State machine

```
                    create (manual, IL-09)
                   ┌───────────┴────────────┐
            no start condition        start condition armed
                   │                        │
                   ▼                        ▼
   ┌────────►  RUNNING ◄────start────►  STOPPED (armed)          ◄──┐
   │             │   ▲                    ▲    │                   │
   │             │   │                    │    │ start condition   │
   │    pause    │   │ start              │    │ fires             │
   │             ▼   │                    │    ▼                   │
   │     instance PAUSED ┼────start───────────────┘                       │
   │             │   │                                            │
   │             │   │ stop (manual or condition)                 │
   └─start───────┘   ▼                                            │
     (from           STOPPING ──flatten complete──►  STOPPED ─────┘
      PAUSED)          │                            │
                       │  (re-issue stop = no-op)   │ DELETE (manual, STOPPED only)
                       ▼                            ▼
                   stays STOPPING              DELETED (tombstone, irreversible)
```

### Transition table

| # | From | To | Trigger | Side effects |
|---|------|----|---------|--------------|
| 1 | — | RUNNING | manual create, no start condition | executor loop starts; entries admitted |
| 2 | — | STOPPED | manual create, start condition armed | loop halted; automation armed |
| 3 | RUNNING | PAUSED | `pause` command or pause condition | pending entries cancelled; loop keeps running; open positions managed |
| 4 | PAUSED | RUNNING | `start` command or start condition | entries admitted again |
| 5 | STOPPED | RUNNING | `start` command or start condition | executor loop starts; entries admitted |
| 6 | RUNNING | STOPPING | `stop` command or stop condition | entries hard-blocked; cancel all open orders; market-close all positions |
| 7 | PAUSED | STOPPING | `stop` command or stop condition | same as #6 |
| 8 | STOPPING | STOPPED | flatten confirmed (0 positions ∧ 0 open orders) | executor loop halts; analytics remain readable |
| 9 | STOPPED | DELETED | manual DELETE only | tombstone; 404 everywhere; telemetry retained |

Every transition writes one `instance_lifecycle_events` row.

---

## §3 Trader behavior summary

| State | New setups | Pending entries | Open positions | Automation conditions |
|-------|-----------|-----------------|----------------|----------------------|
| RUNNING | Accepted | Managed (fills, invalidation) | Managed (bracket, invalidation) | Evaluated |
| PAUSED | Blocked | Cancelled | Managed (bracket, invalidation) | Evaluated (start/pause/stop) |
| STOPPING | Blocked | Cancelled | Flattening at market | Ignored |
| STOPPED | Blocked | None | None (flattened) | Armed (start) |

---

## §4 Cross-References

- [TAE Overview](03-03-01-tae-overview-spec.md) — lifecycle within the executor.
- [TAE Layer ④ — Execution](03-03-03-tae-layer2-execution.md) — flatten mechanics.
- [Simulation Backend & Persistence](03-03-05-tae-paper-trading-spec.md) — recovery of open state across restarts.
