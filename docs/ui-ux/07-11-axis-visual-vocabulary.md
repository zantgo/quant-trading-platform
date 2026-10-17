# Visual Vocabulary

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Cross-cutting (the model contract)

How the four axes are rendered. This document is normative — the rules in §7 are invariants,
not guidance.

---

## 1. The primitive

All four axes share **one** visual primitive: **a dot on a shared horizontal track, one row per
duration.**

```
AXIS NAME          low value ◄─────────────────────────► high value

  1s        │
  3s        │
  5s        │                    ●
 15s        │          ●
  1m        │  ●
  1h        │                        ●
  4h        │  ●
```

Earlier drafts proposed five distinct glyph shapes — a ring for Direction, a bar for
Progression, a distinct shape each. **That design is superseded.** Separate shapes destroy the
vertical read: you cannot compare across axes if each axis looks different.

---

## 2. The three channels

Every dot carries exactly three things.

| Channel | Carries | Never carries |
|---|---|---|
| **Position** (x) | The axis's positional variable | Nothing else |
| **Radius** (size) | The axis's magnitude variable | Nothing else |
| **Outline** (style) | The readiness flag | Nothing else |

**Fill is uniform everywhere.** It carries no information on any axis.

---

## 3. Shared chrome

| Property | Rule |
|---|---|
| **Rows** | One per active duration, ascending `1s → 1d`, matching the timeframe rail |
| **Track** | Five aligned tracks, identical x-origin and identical width, neutral grey |
| **Dot radius** | Minimum ~2px (legible), maximum ~7px |
| **Reading direction** | Always left → right. **No axis reverses.** |
| **Fill** | Uniform |
| **Outline** | Solid = ready · Dashed = warming |

**Track geometry is identical across all four axes.** Any deviation destroys the vertical read,
which is the reason the primitive is unified in the first place.

### Why the vertical read matters

```
              DIR      MOM      STRUCT     PROG
  4h            ●        ●          ●         ●            ●
  1h            ●        ●          ●         ●            ●
 15s            ●        ●          ●         ●         ●
  1m            ●        ●          ●         ●            ●
```

Reading down the `1m` row — momentum decelerating, structure bullish, progression terminal,
volatility contracting — is a confirmed mature move, in one glance, with no numbers.

---

## 4. Readiness is not confidence

The outline flag answers a narrow question, and the distinction matters because the framework
excludes the broader one.

| | **Readiness** | **Calibrated confidence** |
|---|---|---|
| Asks | Are our **inputs** complete? | Are we likely **right**? |
| Source | bar counts, revision state | outcome data |
| Verifiable | yes, mechanically | no |
| Output | boolean or ordinal | a probability |
| Layer | measurement | forecast |

**Readiness is a fact about our own state machine.** "The anchor was confirmed three bars ago,
so this number can still move" is checkable.

**Confidence is a claim about the future.** "68% likely to reach target" needs outcome data.
That is excluded by principle P1 and invariant I10.

Naming the readiness flag "confidence" would smuggle the excluded component back in under a
friendly label. It is never called that.

### Why Progression carries the flag most visibly

Every axis has inputs that can be incomplete. What differs is what triggers it, and how long
the chain is.

| Axis | Trigger |
|---|---|
| Direction | any confluence constituent still warming |
| Momentum | fewer than three readings of the level |
| Structure | most recent structural event unconfirmed |
| **Progression** | **origin provisional or reference revising** |

Progression has the longest chain because its anchor is confirmed late and can be revised
backwards. That is why its flag is the one most likely to fire.

---

## 5. Axis specifications

### 5.1 Direction

**Position variable — directional bias**

| | |
|---|---|
| Domain | `[−1, +1]`, centre = neutral |
| Source | Confluence of the normalised directional readings for that duration |
| Scale | Linear, full track width |

**Size variable — persistence**

| | |
|---|---|
| Measurement | `direction_persistence_bars` - bars since the last genuine sign reversal, or since first establishment. Domain `[0, inf)`, **unbounded** |
| Rendered size | `direction_persistence_pct` - percentile of the raw count against its own Tier A history. Domain `[0, 100]` |
| Origin kind | `persistence_origin_kind` = `none` / `first_establishment` / `reversal` + `persistence_origin_bar` |
| Pre-establishment | **`UNDEFINED`** - outline only (readiness V4). Never `0`, never seeded. |
| Producer | **The Direction aggregation stage** - not any indicator |
| Type | **Axis output.** No reading ID, no aggregation weight |
| Normalisation | `0` = flipped this bar - `100` = held longer than any prior state |

> **This is persistence *through time*, not agreement across durations.** Those are different
> quantities at different granularities - section 9 aggregates Direction across the ladder but
> renders Momentum per duration, so the two are not even computed at the same level. Full
> definition, and the name collision this previously had:
> [`change-requests.md`](../conceptual-foundations/01-17-change-requests.md) **CR-7**.

> **The raw count never drives the radius.** **V3** requires every rendered size to be a
> percentile against its own history. The raw bar count is the honest *measurement*; the
> percentile is the honest *comparison*. Rendering the raw count would make "held 100 bars"
> mean the same thing on a quiet instrument and a violent one - the absolute comparison P2
> forbids.

> **`sign(bias) = 0` is a corridor, not a state.** Neutrality does **not** reset persistence;
> only a genuine sign reversal (`+ -> -` or `- -> +`) does. A bias oscillating around zero
> without reversing is **one continuous state**, not a series of flips. Otherwise the reading
> would measure noise proximity rather than persistence.

> **Why persistence and not magnitude.** Magnitude is `|position|` - the same variable used
> twice. Persistence is independent: a `+0.9` reading two bars old and a `+0.9` reading forty
> bars old are the same direction at the same strength but not the same reliability.

**Readiness** — every constituent of the confluence past its ready threshold. Not merely
computable: past warm-up. One warming constituent taints the whole.

**Colour** — the only axis permitted green and red. `red ◄ ► green`.

**Edge cases**

- Score exactly neutral → dot at centre, contributes nothing
- Contradictory constituents → dot near centre at normal size. Reads as *undecided*, which is
  the correct interpretation

---

### 5.2 Momentum

**Position variable — acceleration**

| | |
|---|---|
| Domain | Continuous: `decelerating ◄ ─ centre ─ ► accelerating` |
| Source | Second derivative of the momentum level over a short window |
| Centre | Flat — no acceleration |

**Size variable — force**

| | |
|---|---|
| Domain | `[0, 100]` |
| Formula | The momentum level itself |
| Normalisation | Percentile rank within its own history |

**Readiness** — needs at least three readings of the level before any derivative is meaningful.

**Edge cases**

- **Zero acceleration with high force** → large dot at centre. This is a real and important
  state: *strong but not building*. It is what a mature move looks like immediately before it
  turns.

---

### 5.3 Structure

**Position variable — net structural bias**

| | |
|---|---|
| Domain | `bearish ◄ ─ neutral ─ ► bullish` |
| Source | Structural classification over the window on that duration |

**Size variable — event density**

| | |
|---|---|
| Domain | `[0, 100]` |
| Formula | `count(BOS + CHoCH + sweeps + divergences) / window length` |
| Normalisation | Percentile within its own history |

**Readiness** — the most recent structural event must be confirmed. An unconfirmed swing is
provisional.

**Edge cases**

- **Zero events** → minimum radius. Position still readable from the classification. Reads as
  *quiet*
- **Contradictory events within one window** → position near neutral, radius large. Reads as
  **contested**, which is exactly when structure deserves least weight

---

### 5.4 Progression

**Position variable — stage**

| | |
|---|---|
| Domain | **Ordinal, five discrete anchors** — early · early-mid · mid · late · terminal |
| Source | Extent, velocity, efficiency and reference state combined into a bucket |
| Scale | Snaps to anchor positions. **Never interpolated.** |

**Size variable — velocity**

| | |
|---|---|
| Domain | `[0, 100]` |
| Formula | Extent accumulated per bar since origin |
| Normalisation | Percentile within its own history |

**Readiness — reference stability**

| | |
|---|---|
| Solid outline | Reference stable — no revision inside the window |
| Dashed outline | Reference being revised |

**This is the flag that matters most in the system.** The anchor confirms late, the origin can
be revised backwards, and the reference target ratchets outward as the movement extends.

**Edge cases**

- **No confirmed origin** → dot at `early`, dashed outline, minimum radius. Reading is
  provisional
- A movement can regress in stage. Animate nothing — it is a new state, not a transition

**The two variables produce a four-cell reading for free**

|  | **Low velocity** | **High velocity** |
|---|---|---|
| **Early stage** | Coiling | Ignition |
| **Terminal stage** | Exhausted | Still extending |

---


## 6. Summary

| Axis | Position | Size | Readiness trigger |
|---|---|---|---|
| **Direction** | directional bias | persistence | confluence constituents warming |
| **Momentum** | acceleration | force | fewer than three level readings |
| **Structure** | net structural bias | event density | last event unconfirmed |
| **Progression** | stage — five anchors | velocity | reference revising |

### Numeric type

| Axis | Position | Size | Numeric type |
|---|---|---|---|
| **Direction** | bias `[-1, 1]` | persistence | Continuous |
| **Momentum** | acceleration `[-1, 1]` | force | Continuous |
| **Structure** | net bias `[-1, 1]` | event density | Continuous |
| **Progression** | stage `1…5` | velocity | **Ordinal** |

**Three axes reduce to a single number across the ladder. One does not** — Progression is ordinal, and averaging them produces a state that does not exist. Their legal
aggregate is the **distribution**, not a mean. Full rule table in
[`framework.md`](../conceptual-foundations/01-13-state-axes-framework.md) §5.6.

---

## 7. Invariants

| # | Rule |
|---|---|
| **V1** | Every axis carries exactly **two market variables** - one positional, one magnitude. |
| **V2** | The readiness flag is never one of the two. It is a third channel, not a variable. |
| **V3** | Every size is a **percentile against its own history**. Never an absolute value. |
| **V4** | Readiness renders as **outline**. Never as fill, never as radius. |
| **V5** | Fill is uniform on every axis and carries no information. |
| **V6** | **Green and red are Direction's alone.** Amber marks caution on any axis and means nothing else. |
| **V7** | Categorical axes **snap; they never interpolate.** |
| **V8** | Track geometry is **identical** across all four axes. |
| **V9** | No axis reverses reading direction. |
| **V10** | The flag is **never named "confidence"** and never becomes a probability. |

## 8. Relationship to the wider framework

The rendering carries the same commitments as the data:

- **V3** is the visual expression of principle **P2** (rank-based, self-referential)
- **V8** is what makes the per-duration gradient legible, which is the reason Momentum,
  Structure and Progression render **per duration** rather than aggregated
- **V10** is the visual expression of **P1** (descriptive, not predictive)
- **V1** is what keeps the axes orthogonal — no axis smuggles a third market quantity into its
  geometry

Aggregating these axes to a single symbol-level value would satisfy none of these. That is why
the ladder is shown rather than collapsed.
