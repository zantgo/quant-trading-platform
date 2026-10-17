# OB / FVG Mitigation Rate

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum · Structure |
| **Retention tier** | C |
| **Data source** | OHLCV candles |

## Purpose

How much of a zone's opportunity has already been spent. A structural zone that price has
traversed and left behind has different forward meaning from one price is currently
consuming.

## Formula

```
mitigation = sum(Volume traded inside zone) / sum(Volume of the zone-forming impulse)
```

## Range

0 to 1 — the fraction of the impulse's volume that has been retested into the zone.

## Signals

| Kind | Fires when |
|---|---|
| Threshold | Mitigation crosses a band (for example 0.5, half-spent) |
| Threshold | Mitigation crosses 1.0 (zone fully mitigated) |

## Divergence

Not supported.

## Levels and other outputs

- Mitigation ratio per active zone
- Zone creation bar and timestamp
- Cumulative volume inside the zone

## Notes

- Requires a **zone lifetime** — the volume of the impulse that created the zone — and the
  bounded Tier C journal to accumulate retest volume between events.
- A zone created long ago and never revisited has low mitigation and is still live; a zone
  created this bar has none. These are different states, so the ratio must be reported
  together with the zone's age.
- Full mitigation does not mean the zone is dead. It means the imbalance that created it has
  been filled. Report the ratio, not a conclusion.
- This is the reading that distinguishes a *fresh* zone from a *worn* one. The present set
  tracks a single slot per zone type with no creation bar and no hit count, so it cannot be
  computed without new state.
