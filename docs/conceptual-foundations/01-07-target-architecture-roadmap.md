# Target Architecture Roadmap

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** This document is the canonical home for "Target Architecture (Not Yet Implemented)" callouts scattered across the corpus. It enumerates each target state, its current status, blocking requirements, and target version. Future revisions update this single document instead of duplicating target notes across layer docs.

> **Implementation status (v10.1).** This roadmap covers **future design improvements** that go beyond the as-spec'd target (e.g. layout changes, SoA history). It is **orthogonal** to the implementation roadmap at [`docs/ROADMAP.md`](../ROADMAP.md), which covers the phased delivery of the as-spec'd engines. Both are required for the v10.1 documentation set.

---

## 1. Target architecture inventory

| Target | Current status | Blocking requirement | Target version | Owner |
|--------|----------------|----------------------|----------------|-------|
| **DOD hot-path (≥ 50,000 events/sec)** | **Staged migration started (v6.5).** The `Indicator` trait contract (`traits.rs`) declares `BarInput` as `f64`. Per-indicator `update()` methods remain `Decimal`-signatured; the conversion happens at the trait boundary (`Indicator::update()` converts `&BarInput` (f64) → Decimal → calls specialized `update()`) until each indicator's per-migration commit lands. Tracking: `AUDIT-V8-400` through `AUDIT-V8-407`. | Per-indicator migration of `update()` signatures from Decimal to f64 (51 modules, ~50-70 line changes each); SoA layout for MME Layer 1 per `docs/engines/market-monitoring-engine/03-02-02-mme-layer1-metrics.md` (target: `[IndicatorEvaluation; 51]` flat array). | Progressive (v6.6: staged indicator commits) | MME team |
| **AoS → SoA candle history** | Not started. History is `Vec<NormalizedCandle>` (AoS). | Decide per-indicator strategy: SIMD-vectorize on SoA, or accept AoS for indicators that can't vectorize. | Unscheduled | MME team |
| **Zero-copy MME distribution** | Not started. Internal distribution uses cloned `MarketSnapshot` structs. | Establish a stable ABI between DIE and MME; introduce a binary-serialised intermediate format. | Unscheduled | DIE + MME |
| **Multi-venue failover** | Not supported. `SymbolMapper` binds each internal symbol to exactly one venue. | Define a "primary venue" model with N-second failover timeout; introduce cross-venue reconciliation (currently listed in [03-01-03 §5](../engines/data-infrastructure-engine/03-01-03-die-layer2-market-data.md) as `cross_venue_offset` but not implemented). | Unscheduled | DIE team |
| **WASM per-instance connection-quality scoring** | Not started. Tracker is process-wide; target: per-(pair, timeframe). | Move tracker to a WASM module to isolate per-instance memory; profile overhead. | Unscheduled (AUDIT-V4-078) | DIE team |
| **Pre-dispatch crash-recoverable persistence** | Not implemented. `PRE_DISPATCH` orders live in process memory only. | Add `pre_dispatch_orders` SQLite table; recovery path on daemon restart. | Unscheduled (per [README §Feature Status](../README.md#feature-status)) | TAE team |
| **`cascade_risk_index` aggregation** | Placeholder field. Not aggregated into `systemic_risk_score`. | Define aggregation formula; produce L7 sample rows. | Unscheduled (AUDIT-V4-005) | PAE team |

### Removed in v6.5

The following items previously appeared in this table and are now **shipped** (moved to their owning engine spec):

- ~~**Unified candle formation across exchanges + per-indicator lifecycle states**~~ — **Shipped in v6.5.** Specs of record: [08-08-candle-buffer-spec.md](../operations-and-compliance/08-08-candle-buffer-spec.md), [03-01-06-die-candle-pipeline-states.md](../engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md), [03-01-07-die-historical-fetch-policy.md](../engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md), [03-02-15-mme-indicator-lifecycle-states.md](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md), and the conceptual overview [01-08-candle-buffer-and-indicator-lifecycle.md](../conceptual-foundations/01-08-candle-buffer-and-indicator-lifecycle.md). `[candle_buffer] size` (default 500) is the canonical warmup depth across every exchange — one of three independent candle tiers (300 = `INDICATORS_MAX_BARS_REQUIRED` indicator floor, 500 = warmup, 1000 = `HIST_BUFFER_MAX` absolute cap); per-TF state machine + per-indicator state machine make warm-up visible to operators. AUDIT-V7-300 … AUDIT-V7-334.

---

## 2. Migration principles

When migrating a target into the implementation:

1. **Update this document first.** Mark the row "In progress" and link the implementation PR.
2. **Edit the layer doc to remove the "Target Architecture" callout.** The current implementation becomes the only normative content. A short pointer to this roadmap remains.
3. **Add an acceptance criterion** (see `AC-DIE-NN` in [03-01-01](../engines/data-infrastructure-engine/03-01-01-die-overview-spec.md) §1.3).
4. **Close the AUDIT entry** in `docs/CHANGELOG.md`.

---

## 3. Non-targets

The following are sometimes confused with target architecture but are intentionally out of scope:

- **Cross-engine shared mutable state** — the engine boundary contract forbids this; any apparent sharing must be `Arc<…>` of read-only-after-construction or synchronised primitives (see [03-01-05 §2.1](../engines/data-infrastructure-engine/03-01-05-die-layer4-data-distribution.md)).
- **Strategy-aware ingestion** — the DIE does not interpret data; strategy logic is TAE/PAE territory.

---

## 4. Cross-References

- 01-02 (Global Architecture): [01-02 §6 Hybrid Memory and Math Architecture](../conceptual-foundations/01-02-global-architecture.md)
- DIE overview (DOD target callouts): [03-01-01](../engines/data-infrastructure-engine/03-01-01-die-overview-spec.md)
- DIE L1 (DOD callout): [03-01-02](../engines/data-infrastructure-engine/03-01-02-die-layer1-raw-data.md)
- DIE L2 (SoA callout): [03-01-03](../engines/data-infrastructure-engine/03-01-03-die-layer2-market-data.md)
- DIE L3 (DOD callout): [03-01-04](../engines/data-infrastructure-engine/03-01-04-die-layer3-data-quality.md)
- DIE L4 (zero-copy callout): [03-01-05](../engines/data-infrastructure-engine/03-01-05-die-layer4-data-distribution.md)
- Open items: [docs/CHANGELOG.md §Open Items](../CHANGELOG.md)