# SignalKind: VolumeClimax

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Category:** Volume
**Purpose:** Specification for the `VolumeClimax` SignalKind — the event where trading volume surges abnormally above its baseline, marking capitulation, breakout fuel, or exhaustion.

---

## 1. Definition

A **VolumeClimax** fires when relative volume spikes far above its rolling average, indicating an outsized participation event. Volume is a non-directional gate — a climax confirms the significance of a concurrent price move but does not itself imply direction.

| Interpretation | Context |
|----------------|---------|
| Breakout fuel | Climax + [Breakout](05-02-04-breakout.md) → high-conviction expansion. |
| Capitulation | Climax at a swing extreme → potential exhaustion/reversal. |
| Distribution | Climax on a failed breakout → trap. |

---

## 2. Producing Indicators

Declared by 2 registry entries: `volume`, `rvol`.

---

## 3. Detection Semantics

```
rvol = current_volume / average_volume
IF rvol ≥ rvol_threshold_institutional (default 1.5) → elevated participation
IF rvol ≥ rvol_threshold_climax        (default 3.0) → VolumeClimax
```

Thresholds are configurable (`config.toml` `[indicators.rvol_threshold_*]`). The climax is emitted on the bar the threshold is breached.

---

## 4. Confirmation

VolumeClimax is a **momentary** signal: it is emitted on the climax bar only (`age_bars = 0`) and does not persist as `Active` on subsequent bars. Its meaning is entirely context-dependent, so downstream layers combine it with price structure (breakout vs rejection) on that same bar to determine whether it is fuel or exhaustion.

---

## 5. Evaluation-Axis Projection

| Axis | Value |
|------|-------|
| Signal Type | `VolumeClimax` (via `label`). |
| Direction | Neutral (gate — modulates confidence). |
| Strength | RVOL magnitude above threshold. |
| Market Regime | Confirms EXPANSION / capitulation context. |
| Priority | High as a confirmation multiplier. |

---

## 6. Cross-References

- [Signals Guide](../03-02-10-mme-signals-guide.md)
- [volume.md](../indicators/04-02-18-volume.md) · [rvol.md](../indicators/04-02-19-rvol.md)
- [SignalKind: Breakout](05-02-04-breakout.md) · [SignalKind: CompressionRelease](05-02-07-compression-release.md)
