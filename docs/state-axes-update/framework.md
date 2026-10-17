# Framework Specification

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

## 1. Purpose

The market-state layer answers questions about **what the market is doing right now**. It
does not answer questions about **what will happen next**.

It replaces the single-axis approach — where every indicator collapses into one directional score
— with a **four-axis design**, in which each axis answers a **distinct semantic question** about
the same market.

> **Distinct is not statistically independent.** The axes have separate semantic
> responsibilities; their measurements are **not** claimed to be uncorrelated. RSI's level, slope
> and divergence are three transformations of related data — their distinct *meanings* justify
> exposing them on different axes, but they do **not** make them three independent votes.
> Aggregation must therefore account for correlated evidence both within an axis and across
> axes (§5.8, and §10.1).

The axes can disagree by construction. A market can be strong on one axis and exhausted on
another simultaneously, and that combination is the most informative reading the system can
produce.

---

## 1.1 The model and the strategy

Two different things share one vocabulary, and conflating them is the most common way a system
like this is misused. This section defines both and fixes the boundary.

| | **The model** | **The strategy** |
|---|---|---|
| **What it is** | This package — the four axes, the reading registry, the leg record, the event journal | Any set of rules that **acts** on what the model reports |
| **Question it answers** | *What is the market doing right now?* | *What should be done about it?* |
| **Output** | Observed state: ordinal stages, percentile ranks, signed levels, dated events | Actions: entries, exits, sizing, allocation, risk limits |
| **Contains** | Formulae, thresholds, ranks, lifecycles | Judgement, rules, parameters, positions |
| **Predicts?** | **No.** It has no horizon and no likelihood | Often — and that is legitimate *strategy* behaviour |
| **Ownership of risk** | None | Entirely |
| **Verified by** | **Causal replay** — re-execution, never trading | **Backtest / paper / live** — which says nothing about the model |

### The interface runs one way

```
   candles  ──►  MODEL  ──►  read-only state  ──►  STRATEGY  ──►  action
                              (this package)         (not this package)

   <-- the model never reads the strategy
```

> **The model is a function from candles to state. It is not a function from candles to orders.**
> Anything that turns state into an order is strategy, and lives outside this specification.

### What the model must never contain

| Forbidden inside the model | Why it is strategy, not observation |
|---|---|
| An entry or exit trigger | A trigger is a decision |
| Position sizing | Sizing is risk allocation |
| Capital or leverage allocation | That is a portfolio decision |
| A win-rate, expectancy or profit target | An outcome claim, and it needs calibration |
| "Buy", "sell", "long", "short" as a recommendation | Direction is *reported*, never advised |
| Any rule that consumes the strategy's own output | Feedback turns an observer into a participant |

> **Direction is reported, never advised.** The Direction axis may read −1 and say *bearish*. It
> may not say *sell*. The difference between those two sentences is the entire boundary.

### Naming discipline

The word **model** is used in exactly one sense in this package — **the descriptive state layer
defined here.** Where the architectural *shape* is meant, this document says **four-axis design**
or **axis model**, never a bare "model". A reviewer's first question when reading "the model is
wrong" should be *"do you mean the state layer, or the strategy on top of it?"*

### Why the distinction is load-bearing

Without it, two failures follow, and both look reasonable:

1. **Strategy failure read as model failure.** A strategy loses money. The natural conclusion —
   "the model is wrong" — does not follow. But neither does the opposite. A strategy can lose
   because its **decisions** are poor, because **costs** are understated, because **execution
   assumptions** are unrealistic, or because the **market state** was genuinely misread. **The
   loss alone cannot isolate which.** That is precisely why the two layers require separate
   evaluation: an observed trading outcome cannot separate the quality of the description from
   the quality of the decision made using it.
2. **Model failure read as strategy opportunity.** A backtest shows a profitable configuration.
   The tempting conclusion — *"this reading works"* — is a category error (**P10**). It shows that
   *this strategy* did well using that reading, subject to the limits of the experiment. **No
   individual reading is independently validated by it.**

**Neither layer can be validated by the other's instrument.** The model is validated by
re-execution; the strategy is evaluated by trading it. Swapping the instruments produces a
conclusion that looks empirical and is not.

---

## 2. Design principles

Constraints on the whole framework. Any proposal violating one is out of scope.

| # | Principle | Consequence |
|---|---|---|
| **P1** | **Descriptive, not predictive.** Every axis reports observed state. No axis outputs a probability. | Continuation risk and calibrated confidence are excluded (§8). |
| **P2** | **Rank-based, self-referential.** "Mature" means late *against the instrument's own recent history*, never against an absolute number. | No absolute numeric threshold may be tuned per duration. Formulas must be unitless ratios, ordinals, or percentile ranks. |
| **P3** | **Duration-independent.** A 1-second candle and a 1-day candle read the same axes through the same rules. | Readings differ across durations; **rules do not**. |
| **P4** | **Orthogonal to direction.** Axes 2–4 must not contribute to the directional score and must never be coloured green (bull) or red (bear). | Only Direction carries direction colour. Other axes use neutral, white, and amber. |
| **P5** | **Discrete over continuous where precision is not real.** When a quantity is estimate-dependent or revision-prone, report an ordinal stage plus a stability flag — never a continuous percentage. | Applies to Progression (§6.4). |
| **P6** | **Literature-grounded.** Every indicator is a published, documented formula. No invented mathematics. | Each spec cites its family. |
| **P7** | **All retention bounded.** No stored series, event journal, or state may grow without limit. | See [`retention.md`](retention.md). |
| **P8** | **Candle-only.** Every reading is computed from OHLCV candles — historical or live — and from nothing else. | Derivatives, order-book, and forced-close data are out of scope (§9). |
| **P9** | **Rule-based and deterministic.** Every reading is a published formula applied to available data. Nothing is trained, fitted, sampled at random, or simulated stochastically. | Same inputs and configuration produce bit-identical outputs. Enforced by **G12**. |
| **P10** | **Backtesting validates strategy, never the model.** The layer is descriptive; it makes no claim that is settled by being tested for profitability. | A backtest may exercise a **strategy** built on these readings. It may **not** be used to accept, reject or tune a reading, an axis, or the model. See §8.2. |

---

## 3. The four axes

| # | Axis | Question | Output type | Characteristic | Needs history? |
|---|---|---|---|---|---|
| 1 | **Direction** | Which way, and with what magnitude? | Signed value + magnitude | Instantaneous, level-based | Yes — for percentile |
| 2 | **Momentum** | How forcefully, and is it accelerating or decelerating? | Signed force + acceleration state | Rate-based | **Yes — required** |
| 3 | **Structure** | How is the move unfolding — impulse, pullback, break, reversal? | Discrete events + relationships | Event-based | **Yes — event journal** |
| 4 | **Progression** | How far along is the current movement? | Ordinal stage + reference stability | Revision-prone | **Yes — current-leg state** |

---

## 4. Reading an indicator across axes

| Marker | Meaning |
|---|---|
| **✓** | Present today |
| **○** | Possible from this source, **not implemented** |
| **·** | Not a meaningful contribution to that axis |

**Axis codes** used throughout: `D` Direction · `M` Momentum · `S` Structure · `P` Progression

Concepts that are **not axes** — Regime, terminality, confidence — are named and scoped in
§5.9 so they are neither axes nor accidents. 

---

## 5. Axis definitions

### 5.1 Direction

**Represents** which way price is moving and how strongly.

**Output** signed directional bias in `[-1, +1]`, plus a magnitude.

**Rule** the magnitude is a confluence of many **distinct** readings. No single indicator may
dominate. The reading is instantaneous — it describes the present bar, with no memory of where
the move came from.

**Distinct, not uncorrelated.** Several Direction readings are transforms of the same underlying
move. Their separate meanings justify separate inclusion; they do **not** justify independent
weighting. Correlated evidence is handled by **family membership** (§5.2, §5.8), not by treating
count as strength.

### 5.2 Momentum

**Represents** the *speed* of the move and whether that speed is building or fading. Distinct
from Direction: a move can be strongly directional and decelerating.

**Output** signed force `[-1, 1]` plus an acceleration state (`accelerating` / `stable` /
`decelerating`).

**Rule** force is a level; acceleration is a *rate of change of the level* and therefore
**requires retained history**. Without history this axis can only report force, and the
acceleration/deceleration distinction is lost.

#### Reading roles — the inventory is NOT the axis value

Momentum carries far more readings than it has outputs. Every reading is therefore assigned one
of three roles, and **filing a reading under Momentum does not make it an input**.

| Role | Meaning | Test |
|---|---|---|
| **ACCELERATION input** | Contributes to Momentum position | Is this a *rate of change* of a momentum level? |
| **FORCE input** | Contributes to Momentum size | Is this a *magnitude* of movement, force or intensity? |
| **DIAGNOSTIC** | Reported, but enters neither output | Does it describe duration, context or classification? |

| Inventory group | Default role | Note |
|---|---|---|
| Force and acceleration | ACCELERATION or FORCE | Must be assigned reading by reading |
| Participation | **DIAGNOSTIC by default** | A volume *level* is not force. Only a *rate* of volume change can be acceleration. |
| Volatility | FORCE for magnitude, ACCELERATION for its rate, DIAGNOSTIC for classification | Bandwidth percentile is magnitude; its rate of change is acceleration; regime bands are diagnostic |
| Quality | **DIAGNOSTIC** | R-squared, efficiency and quality composites describe *how clean* a move is, not how hard |

> **No reading may enter the Momentum value merely because it is filed under Momentum.** If
> several readings propose the same variable, they feed **one named signal family**, not
> separate votes — see 5.7.

#### Known equivalences — do not double-count

| Relationship | Consequence |
|---|---|
| **Williams %R = Stochastic %K − 100** | Mathematically the same series offset by 100. The same lookback yields identical information. |
| **Kaufman Efficiency Ratio** = `abs(N) / D` | **Unsigned** path efficiency. Answers "how direct was the path, **regardless of which way**". Owner: Structure. |
| **Path Efficiency** = `N / D` | **Signed** path efficiency — same quantity, direction retained. Answers "how direct, **and which way**". Owner: Progression. |
| The two efficiency readings | `PE = sign(N) · ER` whenever `D > 0`. **They differ in sign on any downward net move and are not duplicates.** Collapsing them would destroy Progression's direction or make Structure directional (**P4**). |
| `Directional Persistence Ratio` | **Removed** — byte-identical to Path Efficiency (both signed). Recorded in [`change-requests.md`](change-requests.md) **CR-6**. |
| Stochastic fast %K vs slow %D | **Not** equivalent — the smoothing carries real information |
| MACD, PPO, ROC, TRIX | Different smoothing and normalisation of related evidence. Useful, but **not independent votes** |

### 5.3 Structure

**Represents** how the move is unfolding: which swing it is in, where pullbacks occur, what has
been broken, and what has been invalidated.

**Output** discrete structural events with timestamps, plus current swing relationships.

**Rule** structure is a **sequence**, not a level. It is fundamentally event-driven and
first-class outputs, not derived conveniences.

**Every retained event carries a lifecycle** — activation, persistence, expiration,
invalidation, reactivation — specified in [`event-lifecycle.md`](event-lifecycle.md). A storage
bound alone is not enough: a bounded journal can still hold events that are no longer true.

### 5.4 Progression

**Represents** how far the current *movement* has developed, measured against an anchored
reference.

**Output** an **ordinal stage** (early / early-mid / mid / late / terminal) plus a
**reference stability flag**. Never a continuous percentage.

**The anchoring requirement.** A progression reading is meaningless without a defined
movement. Every reading requires:

1. An **origin** — a confirmed swing point
2. A **direction** — which way the leg runs
3. A **reference target** — an explicitly stated estimate of extent
4. A **completion criterion** — what declares the movement finished
5. An **invalidation criterion** — what forces a restart

**Reference must be published with the reading.** A stage reported without its reference and
its revision state is not interpretable.

**Completion criteria** — a movement may be declared finished by any of these; they are not
equivalent:

| Criterion | Meaning |
|---|---|
| Structural | The opposing swing confirms |
| Extension | Price reaches an extension reference (1.272 / 1.618 / 2.0 / 2.618) |
| Character change | A change-of-character event fires against the leg |
| Volume climax | An extreme-volume, extreme-range bar fails to extend |

**Storage.** Progression needs no *long* history, but it does need **persistent movement
identity**. All four axes use causally available historical data — Direction for percentile
ranking, Momentum because rate-of-change is *defined* by history, Structure for its event
journal. What is distinct about Progression is that it tracks **a specific leg** across
observations, preserving that leg's origin, direction, reference target and lifecycle.

It is therefore computed from **one current-leg record per
`(instrument, timeframe_secs, structure_level)`** (Tier D, [`retention.md`](retention.md))
plus a short scalar window (Tier A) for velocity. Historical events and revisions follow their own
retention policies (§7, [`event-lifecycle.md`](event-lifecycle.md)). A movement older than the
Tier B window is no longer a *current* movement — the record is replaced when a new leg begins,
and the old one discarded.

**The record is fully specified in [`leg.md`](leg.md)** — field contract, versioning, the
`progress` arithmetic `(P - O) / (T - O)`, target revision handling, and the lifecycle state
machine. Its two defining properties:

- **Causal.** Every field states when it became *knowable*, not only when the thing happened.
  A pivot that printed on bar *n* is knowable at bar *n + k*. Replays may display a field
  retrospectively; they may never consume it early.
- **Revision-tracked.** When the reference target moves, `target_version` increments. A change
  in `progress` caused by a retarget is explicitly **not** evidence that price travelled.

**Known limitation — anchors confirm late.** A swing point is not confirmed until subsequent
bars prove it. The origin can therefore be revised *backwards*, and the reference target
ratchets outward as a movement extends. Progression is inherently a **provisional,
revision-prone estimate**. This is why P5 requires a stage plus a stability flag rather than a
number.

### 5.5 Volatility x Participation diagnostic

A four-state diagnostic **outside** the four-axis model. It is not an axis and it is not
derived from the Momentum variables - see the warning below.

**Its inputs are its own, and must be measured explicitly:**

| Input | Readings that supply it |
|---|---|
| Volatility change | ATR rate of change - Bollinger bandwidth rate of change - Chaikin Volatility |
| Participation change | RVOL acceleration - Volume expansion / contraction rate |

| | **Participation DOWN** | **Participation UP** |
|---|---|---|
| **Volatility UP** | **Chopping** - movement nobody is participating in | **Expanding** - real expansion, direction resolving |
| **Volatility DOWN** | **Settling** - quiet, drifting | **Coiling** - energy building under a flat surface |

> **These are not the Momentum variables, and must not be substituted for them.**
> Momentum position is the **acceleration of the momentum level**; Momentum size is the
> **force**, which is that level. Volatility change and participation change are neither.
> Momentum acceleration is not volatility change, and force is not participation change. They
> may correlate; they are not the same quantity, and deriving these labels from
> acceleration x force would produce different boundaries and different names.

The diagnostic is never applied as a weight to any axis. Until a multiplicative or additive
relationship is demonstrated, any such use is a hypothesis.

### 5.6 Numeric representation

Every axis can be represented numerically. Three reduce to a single number across the ladder;
one cannot.

| Axis | Position | Size | Numeric type |
|---|---|---|---|
| **Direction** | bias [-1, 1] | persistence | Continuous |
| **Momentum** | acceleration [-1, 1] | force | Continuous |
| **Structure** | net bias [-1, 1] | event density | Continuous |
| **Progression** | stage 1...5 | velocity | **Ordinal** |

**Aggregation rules across the duration ladder:**

| Axis | Legal aggregate | Forbidden |
|---|---|---|
| Direction | mean bias + agreement percentage | - |
| Momentum | mean acceleration **+** fast-slow gradient | mean alone - it erases the gradient |
| Structure | mean net bias + net event count | - |
| **Progression** | **the distribution** - count per stage, plus gradient direction | **mean stage, interpolated stage** |

A mean stage of 3.4 is meaningless - there is no such movement, and "halfway between mid and
late" is not a state. This is principle **P5** in numeric form.

> **The guard must be structural, not advisory.** Represent stage as a typed enum with no
> arithmetic operators, not as `f64`. A compile error is worth more than a note in a document.

---
### 5.7 Ownership, consumption, emission

Three concepts that must never be conflated. Conflating them is how one indicator ends up
counted three times.

| Concept | Question | Answer |
|---|---|---|
| **Ownership** | Which axis defines the *semantics* of a reading? | Exactly one axis. A reading has one owner. |
| **Consumption** | Which other axes may use it as an *input*? | Zero or more, each with a declared role. |
| **Emission** | Which events does the underlying indicator *produce*? | Independent of both. Events have a canonical identity. |

An indicator may legitimately serve several purposes. What it may not do is be **counted**
several times. Ownership counts once; consumption references; emission is a separate stream with
its own identity (see `vocabulary.md`, *One canonical event identity*).

#### The registry is authoritative for reading identity

The 160-reading inventory is **machine-checkable, not hand-maintained**. Every reading declares
its producer; readings with no single producer declare their inputs explicitly.

**There is exactly one authoritative record of a reading, and it lives in the registry.** Every
other appearance — [`inventory.md`](inventory.md), the README totals, the per-axis counts — is
**generated from it**. A second hand-maintained list is a defect, not documentation.

##### The reading record

| Field | Type | Meaning |
|---|---|---|
| `reading_id` | id | **Stable, never renumbered.** A retired reading is tombstoned, not reused. |
| `name` | str | Human label. |
| `producer` | id \| none | The indicator that produces it, or `none`. |
| `derived_from` | [id] | Explicit inputs when `producer` is `none`. Non-empty iff `producer` is `none`. |
| `owner_axis` | `D`\|`M`\|`S`\|`P` | **Exactly one.** Ownership is single. |
| `consumers` | [`D`,`M`,`S`,`P`] | Which axes may use it as an input. Zero or more. |
| `signals` | [kind] | Signal kinds it may emit. |
| `supports_divergence` | bool | Whether divergence detection is defined for it. |
| `tier` | `A`\|`B`\|`C`\|`D` | Retention tier it requires. |
| `role` | `acceleration`\|`force`\|`diagnostic`\|`position`\|`magnitude`\|`stage` | Its role on its owner axis, or `diagnostic`. |
| `family` | id | Aggregation family. **Declared, never inferred from count.** |
| `owner_class` | `measurement`\|`derived`\|`event`\|`diagnostic` | What kind of thing it is. |
| `q1_unique_information` | str | **What it captures that no other reading captures.** Non-empty for `KEEP`. |
| `verdict` | `keep`\|`merge`\|`demote`\|`field`\|`move`\|`cut` | Outcome of the measurement-quality review. |
| `status` | `present`\|`new` | Shipped today, or to be added. |

A reading **derived from a leg or structure field is a category error, not a weak reading** — see
[`measurement-quality.md`](measurement-quality.md) §6.2. The `field` verdict exists for exactly
that case.

##### Generated documents

| Artifact | Generated from |
|---|---|
| [`inventory.md`](inventory.md) | the full reading registry |
| README axis counts and totals | the same registry |
| Per-axis present/new splits | the same registry |

> **A generated document is never hand-edited.** Editing it produces drift, and drift is a
> **failing check**, not a merge conflict to resolve by hand. If a document is wrong, the
> **registry** is wrong, and the fix lands there and regenerates.

**The drift check is a build step, not a review step.** `./manage.sh test-doc` fails when any
generated artifact differs from the registry, byte for byte. This is what stops documentation
diverging from implementation a second time — which is exactly how the previous hand-maintained
inventory came to disagree with the registry in the first place.

| # | Rule |
|---|---|
| I27 | Every reading resolves to a declared producer **or** an explicit `derived_from` list. No orphans. |
| I28 | Reading IDs are stable and never renumbered. A retired reading is tombstoned, never reused. |
| I29 | Every producer key referenced by a reading exists in the registry. Dangling references fail. |
| I30 | `owner_axis` is exactly one axis and is a member of `consumers`. Ownership is single; consumption is many. |
| I31 | Registry totals reconcile exactly: axis counts sum to 160; present/new split is 76/84. |
| I32 | A producer declaring N readings supplies **N-1 families or fewer**. Declaring more readings never multiplies weight by default. |
| I33 | `derived_from` is non-empty **iff** `producer` is `none`. Never both, never neither. |
| I34 | `role` is not `diagnostic` **iff** the reading enters its owner axis's aggregation. |
| I35 | **Every generated artifact is byte-identical to freshly generated output.** Drift fails the build. |
| I36 | No hand-maintained document restates a registry total. Where a total appears, it is generated. |
| I37 | No reading enters at `keep` without a recorded `q1_unique_information` answering Q1. |
| I38 | A `derived` reading contributes **one** vote through its parent. Its container is not evidence. |
| I39 | No reading has a formula identical to another reading's. A collision is `merge`, never two entries. |
| I40 | A reading whose quantity is already a field of the leg or structure record carries the `field` verdict and is **not** an indicator. |
| I41 | No reading count changes for balance. Verdicts follow evidence, never symmetry. |
| I42 | Incremental value (Q4) is established chronologically by ablation on the **axis output**. Never by in-sample fit, profitability, or generalisation claims. |
| I43 | **Nothing is trained, fitted, sampled at random, or simulated stochastically.** Every reading is a published formula over available candles (**P9**). |
| I44 | Same inputs and configuration produce **bit-identical** outputs. No wall-clock, no unseeded randomness, no iteration-order dependence. |
| I45 | **No backtest result may be cited as justification for a reading, its weight, or its formula.** Backtesting settles strategy questions only (**P10**, §8.2). |
| I46 | The model contains **no entry, exit, sizing, allocation, or profit target.** Anything that turns state into an order is strategy (§1.1). |
| I47 | The model **never reads the strategy's output.** The interface is one-directional: candles → model → state → strategy. |
| I48 | Direction is **reported, never advised.** An axis may read *bearish*; it may not emit *sell*. |
| I49 | **Causal correctness is not semantic correctness.** Passing every causality gate does not establish that a reading captures its intended market characteristic (§8.4). |
| I50 | A reading may not be marked verified on causal evidence alone. Semantic validation is required. |
| I51 | A percentile rank is conditional on its **reference set**. No rank asserts a probability of a future event, and none is independent of how its reference set was defined. |

Decided in [`change-requests.md`](change-requests.md) CR-4 and extended in
[CR-5](change-requests.md#cr-5--the-registry-is-the-only-source-of-reading-totals). **CR-4 must
land before any gate that consumes reading identity** — cross-axis double-counting cannot be
tested without it.

### 5.8 Staged aggregation

An axis with ten readings from one family must not exert more influence than a family with
three genuinely distinct readings. Aggregation is therefore staged, and never a flat average.

| Stage | Action |
|---|---|
| 1. **Normalise** | Each reading is put on its own scale and, where appropriate, ranked against its own history. |
| 2. **Family** | Related readings are combined into a **named signal family**. Williams %R and Stochastic %K are one family, not two. |
| 3. **Axis** | Family results combine into the axis's **two defined outputs** only — acceleration and force. |
| 4. **Test** | Each additional family is checked for incremental information across the duration ladder before it is admitted. |

**Diagnostics never enter stage 3.** They are displayed and exported; they do not vote.

> **The axis value is assembled from families, not from files.** Adding a new indicator to an
> existing family costs nothing. Adding a new family requires evidence from stage 4.

### 5.9 Named concepts that are not axes

Three concepts recur in this domain and are worth having. None of them is a fifth axis, and
none of them may be silently introduced into one. Each is named here with an explicit role and
an explicit status so that it is **defined rather than either missing or smuggled**.

| Concept | Question it answers | Role | Status |
|---|---|---|---|
| **Regime** | How is the market behaving *as a whole*, and how does that behaviour persist? | **Contextual modulator** — conditions how much weight a reading carries; never contributes a value to an axis | **Defined, not built.** See below. |
| **Terminality** | Does the current movement look like it is resolving, or still developing? | **Explicit continuation-versus-termination assessment** — a named output of Progression, not a probability | **Definable now**, ordinal only |
| **Confidence** | How much do we trust this reading? | **Separate meta-measure** over the whole layer | **Reserved.** Requires calibration |

#### Regime — reserved as a contextual modulator

The word is deliberately not used for anything in this framework today. In the quantitative
literature *regime* means a **latent, persistent, inferred** state — Hamilton Markov-switching
regimes, HMM and GMM state posteriors, clustering of returns into volatility states. That is a
specific and useful idea, and it is **not** what the four axes are.

| | The four axes | A regime |
|---|---|---|
| Nature | Directly observed | Inferred |
| Memory | Instantaneous, or bounded by tier | Persistent across states |
| Computed | Every bar, from this bar | Fitted over a window |
| Identity | Descriptive | Latent |

**Do not name an axis, a reading or a diagnostic "regime."** The name is held for a genuine
latent-state layer, downstream, if one is ever built. Until then it is correct that the word
appears nowhere in this specification.

If a contextual modulator is later introduced, it must satisfy:

1. It modulates **how much weight a reading carries**, never **what value** the reading takes.
2. It never contributes to the Direction confluence score (invariant **I3**).
3. It never renders in direction colour.
4. Its inputs are declared explicitly, and are not silently drawn from the Momentum variables.

#### Terminality — definable now, ordinal only

An **explicit assessment** of whether the available structural evidence supports continuation or
supports an end to the current leg. It is a distinct question from Progression's stage, and the
distinction is worth keeping:

| | Progression stage | Terminality |
|---|---|---|
| Asks | How far along is it? | Do the evidence supports continuation, or an end? |
| Computed from | `progress` against the current reference target | **Completion criteria only** — opposing swing confirmed, character change, climax failure |
| Changes when | The reference target moves | A structural criterion fires |

A leg can be at stage `early` and carry termination evidence (a fast, sharp reversal that is
already resolving), or at stage `late` and carry none (an extension still running, with no
structural evidence of turn). The two are not derivable from each other, and neither implies
the other.

**The three states — and why the labels are what they are.**

| State | Meaning |
|---|---|
| `continuation_supported` | The structural evidence available supports further development of this leg. |
| `indeterminate` | The evidence does not distinguish continuation from termination. |
| `termination_evidence` | The evidence supports a potential end to the current leg. |

These labels name **the direction of the claim** rather than a mood. An earlier draft used
`developing` / `uncertain` / `resolving`, and `resolving` was genuinely ambiguous — it reads as
*the leg is terminating* and equally as *uncertainty about the leg's outcome is clearing in
favour of continuation*. Naming the claim removes that: `continuation_supported` and
`termination_evidence` each state which way the evidence points, and `indeterminate` states
plainly that it points neither way.

> **These are interpretations of evidence — not probabilities, and not guarantees.**
> `termination_evidence` means "structural evidence bearing on termination is present". It does
> not mean the leg will end, and it carries no likelihood. A leg may carry
> `termination_evidence` and then extend for a long time.

**Terminality is not the completion event.** These are separate outputs and must never be
collapsed into one:

| | Terminality | Completion |
|---|---|---|
| Type | Ordinal assessment, continuously re-evaluated | Discrete event with a timestamp and a `close_reason` |
| Terminal? | **No** — it changes as evidence changes | **Yes** — it ends the leg |
| Question | What does the evidence currently support? | Has the leg ended? |

A leg reaching `termination_evidence` is **not** complete. The leg completes when a named
criterion actually fires and a `close_reason` is recorded — see [`leg.md`](leg.md) §6. Conflating
the two would let a leg be reported as ended while it is still running, which is precisely the
failure mode a descriptive layer must not have.

Terminality is therefore an **ordinal** plus the **named criteria** that produced it. It is
**never** a probability, and it is **not** the excluded continuation-risk component in §8, because
it asserts no likelihood over any horizon.

#### Confidence — reserved, and separately named

Every reading in this layer is descriptive. None of them is a measure of how much to trust the
model. **State Consensus** (§6.1) and **State Stability** (§6.2) exist and are deliberately
**not** called confidence.

> **This is a scope decision, not a mathematical prohibition.** A confidence estimate derived
> from the same features it is meant to inform **is** circular — it restates those features and
> presents them as independent evidence. But confidence is not inherently invalid: it can be
> **calibrated against independently defined outcomes and evaluated on separate data**.

The layer simply does not do that work. Adding it would require its own target definition,
calibration methodology, and independent validation — none of which belongs in a descriptive
layer. Until then it is **reserved and named**, so that if it is added later it arrives as a
meta-measure over the layer and not as a fifth value on an existing axis.

## 6. Derived readings

Two additional readings, both descriptive, both permitted.

### 6.1 State Consensus

The percentage of the four axes currently in agreement. High means the market is internally
coherent; low means the axes disagree and the reading is fragile.

### 6.2 State Stability

Whether the current reading is **holding** or **oscillating**, derived from how often axes
change state over a recent window. Oscillating readings warrant lower trust regardless of how
strong the individual values are.

---

## 7. Scope of the data contract

**Every reading derives from OHLCV candles and nothing else.** Historical or
live-reconstructed candles are equally valid. No axis may consume open interest, funding,
mark/index, order-book depth, spread, or forced-close streams.

### 7.1 What OHLCV does and does not establish

| Available | Not available |
|---|---|
| OHLC per candle | — |
| **Volume** per candle — traded size | Signed order flow |
| Volume-based activity proxies: RVOL, volume profile, OBV, CMF, MFI, Force Index | Actual buying versus selling pressure |

> **Volume is not order flow.** Volume establishes that a size traded. It does not establish
> who was aggressor. Every buy/sell attribution in this framework — the CMF money-flow
> multiplier, the volume-profile buy/sell split, accumulation/distribution — is **derived from
> candle direction**, which is a proxy for aggression, not an observation of it. Any reading
> that depends on it must be labelled a price-derived estimate.

> **Participation is a volume-derived activity proxy.** It measures how much traded, never who
> wanted it. It is not a substitute for order-book or derivatives data, and it is the reason
> those are excluded rather than approximated.

> **Percentile ranks are measurements, not unconditional facts.** A rank is a deterministic
> result **conditional on the reference set** — the window, the sampling rule, and the data
> currently present. "Faster than 92% of its own recent history" is exactly reproducible and
> exactly true *of that reference set*. It asserts **no probability of any future event**, and it
> is not independent of how its reference set was defined. The reference definition is therefore
> part of the specification, not an implementation detail.

**Consequence for cross-venue comparison.** Volume conventions differ between venues and
between spot and perpetual products. Volume-derived readings are comparable in *shape* but not
in *level* across venues — which is a further reason every size variable is a percentile rank
against its own history rather than an absolute value.

### 7.2 Why candle-only

| Benefit | Consequence |
|---|---|
| **Uniform coverage** | Derivatives availability varies by venue and by interval. Candle-derived readings are identical everywhere. |
| **Historical replay** | Every reading is recomputable from any candle archive. Derivatives history is sparse, venue-specific, and often unrecoverable. |
| **Determinism** | Two runs over the same candles produce identical state. |
| **Single source** | One input stream, one warm path, one bootstrap. |

### 7.3 What candle-only cannot give you, and the substitute

| Removed capability | Candle-only substitute | Status |
|---|---|---|
| Forced-close cascade lifecycle | **Volume climax → exhaustion climax** — an extreme-volume, extreme-range bar that fails to extend | Established technique |
| Cascade intensity | Relative volume percentile + range expansion | Established technique |
| Leverage-cluster levels | Volume profile POC / VAH / VAL, swing extremes | Established technique |
| Order-book pressure | Volume-weighted directional pressure — CMF, Force Index, Ease of Movement | Established technique |
| Spread | ATR — the candle's own realized range | Established technique |

**These are not equivalents in precision.** Volume and range are the *footprint* of the
activity; they do not identify *who* was forced. Treat any such reading as a proxy and label it
as one.

---
### 8.1 The determinism execution contract

"Same input, same output" is only meaningful once **input** is enumerated and **equality** is
defined. Both belong to the specification.

#### The complete input set

Determinism is a property of the whole input tuple, not of the candles alone.

| # | Component | Why it is part of the input |
|---|---|---|
| 1 | **Candles** — OHLCV, with timestamps | The market observation itself |
| 2 | **Availability timestamps** | A field read before its `available_at` is a different computation |
| 3 | **Configuration** | Tiers, windows, thresholds, `STABILITY_WINDOW`, `REVISION_BUDGET` |
| 4 | **Reference-set definition** | The window and sampling rule behind every percentile rank (**I51**) |
| 5 | **Specification version** | Same code against a changed spec is a different computation |
| 6 | **Implementation version** | Build identity; required to make a report reproducible |

> **Changing any one of these is a different input.** A run is reproducible only if all six match.

#### How equality is judged

Two modes, because they answer different questions.

| Mode | Definition | Applies to |
|---|---|---|
| **Exact** | **Bit-for-bit identical** serialised output | Same binary, same platform, same input tuple |
| **Tolerance-bounded** | Identical within a **declared, per-quantity tolerance** | Across supported environments |

**Default: exact.** Canonical reproducibility mode is bit-for-bit. Floating-point differences
from a different platform, compiler, or optimisation level are **environmental**, not semantic.

| Rule | |
|---|---|
| **D1** | Reproducibility is claimed in **exact** mode unless the report explicitly declares tolerance mode. |
| **D2** | In tolerance mode **every tolerance is declared, per quantity, in the report.** An undeclared tolerance is not a tolerance. |
| **D3** | The cause of a difference is **not presumed**. Investigation must discriminate between environmental and semantic — and **floating-point behaviour is never the default explanation.** |

> **D3 is about order of operations.** You cannot classify a difference before you know what
> caused it. What is forbidden is the opposite habit — seeing a mismatch and reaching for
> "floating point" because it is the convenient explanation. The cause stays **open** until
> evidence narrows it.
| **D4** | `UNDEFINED`, `NaN`, and all identity fields (`leg_id`, event identity, `target_version`, `structure_version`, availability timestamps) are **exact even in tolerance mode.** Only continuous measurements may carry a tolerance. |

> **D4 is load-bearing.** A structural identity that differs within tolerance is not a rounding
> artefact — it is a **different history**. Tolerance must never blur the line between the same
> computation and a different one.

### 8.2 What backtesting may and may not settle

This deserves its own statement because it is the easiest boundary to erode by accident.

| Backtesting **may** | Backtesting may **not** |
|---|---|
| Compare two **strategies** that consume the same readings | Establish that a **reading** is correct |
| Establish that a **strategy** beats another on this data | Establish that the **model** predicts — the model predicts nothing |
| Tune a **strategy's** parameters | Tune a reading's formula, threshold, or weight |
| Reveal a strategy the model made **easier** to trade | Reveal a reading that is "more predictive" |
| Reject a **strategy** | Accept or reject an **axis** |

> **The thing under test is not the model.** A backtest result is evidence about the *strategy
> placed above* the state layer. Using one to justify a reading's existence, weight, or formula is
> a category error — it swaps a descriptive claim for a profitability claim this framework
> explicitly refuses to make.

**Corollary.** The causal replay gate ([`execution-plan.md`](execution-plan.md)) validates the
**model** — by re-executing it, never by trading it. That is the only permitted way to admit a
reading into the architecture.

### 8.3 Deliberately excluded

Two components were considered and **excluded from this framework**. They are named here so
they are not silently reintroduced.

| Excluded | Why |
|---|---|
| **Continuation / termination risk** | Answers "will this extend, stall, or reverse over a stated horizon." That is a probability claim. Divergence, exhaustion, and character change are *evidence* about continuation; none of them establishes it. Producing a probability requires calibration against outcomes, which is outside this layer. |
| **Calibrated confidence** | A **scope** decision. This layer reports defined measurements and does not claim a calibrated probability that its interpretation is correct. A confidence estimate built from the same features it informs would be circular — but confidence is not inherently invalid, only **uncalibrated here**. Adding it would require its own target definition, calibration methodology and independent validation. |

Both may be added later, in a layer downstream of this one, once outcome data exists.

> **This is not the same as deleting the questions.** §5.9 keeps both named and scoped.
> **Terminality** — the ordinal question *"do the available structural evidence support
> continuation, or a potential end to the current leg?"* — is answerable from completion
> criteria today and is specified there. What is excluded is **terminality as a probability over
> a stated horizon**, which is a different object with a different name and a different
> requirement. Likewise **confidence** is reserved as a meta-measure over the layer, not quietly
> substituted by State Consensus.

---

### 8.4 Four kinds of validation - they are not interchangeable

> **Causal correctness is not semantic correctness.** A deterministic, causal implementation can
> consistently compute the **wrong** value — because a formula was implemented incorrectly, a
> specification is ambiguous, or a market interpretation is poorly chosen. Passing every
> causality gate is entirely compatible with that.

| Validation | What it establishes | Can it be replaced by another? |
|---|---|---|
| **Specification conformance** | The implementation produces the outputs the specification requires | No |
| **Causal replay** | Outputs use **only** information available at the relevant time | No |
| **Semantic validation** | The readings **behave as intended** on known scenarios, boundary cases, and **independently defined reference examples** | No |
| **Strategy backtesting** | A particular **decision policy** performs under the tested conditions, assumptions and costs | No — and it validates nothing about the model |

### What the gate suite does and does not establish

| Gate suite covers | Partially covers | Does **not** cover |
|---|---|---|
| Specification conformance, in part | Semantic validation, for a **few** cases only (G3–G7, G10 assert intended behaviour) | Semantic validation in general |
| Causal replay, comprehensively | — | Whether the *intended* behaviour is the *correct* market interpretation |

**A fully green G1–G13 result therefore establishes:** the system uses only what it knew, does so
deterministically, and matches its written definitions on the cases the suite covers.

**It does not establish** that a reading captures the market characteristic it was meant to
capture. That is semantic validation, and it needs **independently defined reference examples** —
worked cases with a known-correct answer, derived from the published indicator literature rather
than from this system's own output.

> **Being a well-defined observer and being a useful observer are different properties.** The first
> is what the causal gate addresses. The second needs its own evidence, and it is the hardest
> part of the work.

### What a passing result establishes — and what it never establishes

> **Passing tests establish evidence for the cases tested. They never establish universal
> correctness, and a green suite is not a proof.**

| A green run supports this claim | It does **not** support this claim |
|---|---|
| The system used only information available at the time — *on the cases tested* | The system will always use only available information |
| Outputs are deterministic and reproducible — *for the tested input tuple* | The system is correct for untested inputs |
| The readings behaved as specified — *on the tested reference, adversarial and metamorphic cases* | The specification captures the intended market interpretation |
| The specified defects were detectable — *for the mutations injected* | No other defect class exists |

**Untested inputs and unexercised cases remain unverified.** This is not a caveat on the method; it
is what every empirical check is. The value of the suite is the **specific** evidence it produces,
which is why mutation coverage and case diversity matter more than the count of assertions.

**Corollary.** A gap in coverage is a gap in *knowledge*, not a proof of absence. When a class of
case is untested — as semantic validation largely is today — the honest statement is *"not
established"*, never *"verified"*.

### Semantic validation is not prediction

Adding semantic validation does **not** turn the model into a predictive system. A descriptive
model can be checked against explicit definitions and expected behaviour without forecasting
returns or placing trades. The reference examples are **descriptions of intended behaviour**, not
claims about future prices.

---

## 9. Cross-duration behaviour

Multiple durations run in parallel.

- **Direction** — aggregate across durations into a single reading.
- **Momentum, Structure, Progression** — **render per duration**. Do not aggregate.
  The *gradient* across the ladder is itself informative: fast durations rolling over while
  slow durations still rise is a distinct and highly informative state.
- **Tier B retention** is what makes cross-duration comparison possible.

No axis may carry a duration-specific parameter. Readings differ across durations because the
underlying behaviour differs; **rules do not** (P3).

---

## 10. Invariants

### 10.1 Axis boundaries

Each axis has a distinct **definition**. They do not need to be statistically independent -
that is an empirical question, not a design one - but each must answer a question no other axis
answers.

| Axis | Must describe | Must not silently substitute for |
|---|---|---|
| **Direction** | Signed directional bias and its consistency | Acceleration, or lifecycle stage |
| **Momentum** | Change in movement and its intensity | Volatility change, unless explicitly named as an input |
| **Structure** | Price-event sequence and relationships | Another copy of directional bias |
| **Progression** | The current movement's lifecycle stage | A generic trend-strength or path-efficiency score |

> **Structure net bias must be derived from structural events** - HH/LL classification, breaks,
> sweeps - and never from a normalised price slope. A slope-derived bias would make Structure
> answer Direction's question a second time.

> **The Efficiency Ratio is a Structure reading.** It measures how directly price travelled,
> which is a path property of how a movement unfolds - not where the movement is in its
> lifecycle, and not directional, since its standard form is direction-neutral. It is
> **consumed** by Progression as an input to stage classification, but it is not itself a
> stage and does not belong on the Progression axis.
>
> **Direction's persistence is a different quantity, and is defined in CR-7.** It is
> *persistence through time* — bars since the axis position last changed sign — not agreement
> across durations. It is an **axis-level output produced after aggregation**, with no reading ID
> and no aggregation weight. It is not the `Bars Since Trend Slope Flip` reading, which measures
> one indicator's slope *before* aggregation.

### 10.2 Data invariants

| # | Rule |
|---|---|
| I1 | No axis outputs a probability, percentage-of-certainty, or calibrated score. |
| I2 | No absolute numeric threshold in any axis formula is tuned per duration. |
| I3 | Axes 2-4 never contribute to the Direction confluence score. |
| I4 | Axes 2-4 never render in direction colour (green / red). |
| I5 | Every Progression reading is published together with its reference and stability flag. |
| I6 | Progression never renders as a continuous percentage. |
| I7 | Every indicator retains at least Tier A history on every duration. |
| I8 | Structural events are timestamped and retained in a **bounded** event journal, not recomputed. |
| I9 | Confidence and continuation risk do not appear in this layer. |
| I10 | No retained series, journal, or state grows without limit. |
| I11 | Progression is computed from a single current-leg record, not from accumulated history. |
| I12 | No axis, indicator, or reading may consume a non-OHLCV data source. |
| I13 | Every axis reading must be reproducible from a candle archive alone. |
| I14 | No field is consumed before its `available_at`. Retrospective display is permitted; retrospective *consumption* is not. |
| I15 | `progress` is never clamped. A value outside `[0, 1]` is information, not an error to be trimmed. |
| I16 | Every progress reading is published with `target_version` and `target_stable`. |
| I17 | Only events in state `active` contribute to an axis count. Activated-but-unconfirmed counts zero. |
| I18 | Absence of a contradicting event never extends an event's life. Expiry is independent of contradiction. |
| I19 | A reactivated event is a new canonical identity. No record is ever revived, and no event is counted twice. |
| I20 | Active count, retained count and emit count are separately exported and never conflated. |
| I21 | Terminality is ordinal and is never a probability over a horizon. |
| I22 | The word "regime" is not used for any axis, reading, or diagnostic in this layer. |
| I23 | Terminality and leg completion are separate outputs. `termination_evidence` never implies the leg has ended. |
| I24 | Every progression reading carries its `structure_level`. A reading without one is not publishable. |
| I25 | A degenerate reference (`T == O`, or `abs(T - O)` below the leg's own noise floor) yields `UNDEFINED`, never a number. |
| I26 | Target stability is never forced, widened, or bypassed to make a progression reading available. |
| I27–I36 | *(defined in §5.7, not here)* — reading identity is registry-declared, machine-checked, and the sole source of every displayed total |
| I37–I42 | *(defined in [`measurement-quality.md`](measurement-quality.md))* — measurement-quality review obligations |

---

## 11. Anti-patterns

| Anti-pattern | Why it is rejected |
|---|---|
| Adding a fifth axis | The four axes cover four distinct semantic dimensions. A fifth would re-express one of them — which is exactly what the removed fifth axis did. |
| Claiming the axes are statistically independent | They have separate meanings, not proven-zero correlation. Aggregation must handle correlated evidence (§5.8). |
| A combined "trend score" | Collapses four axes back into one number and destroys the orthogonality the framework exists to provide. |
| Direction colour on a non-direction axis | Green means long, red means short. That convention belongs to Direction alone. |
| Absolute thresholds per duration | Converts a self-referential reading into a tuned constant. |
| Calling consensus "confidence" | Implies calibration that does not exist. |
| A continuous progression percentage | Implies precision the estimate cannot deliver. |
| Unbounded retention | A tier that grows without limit is a memory leak with a justification. |
| An event journal bounded only by count | A slow-moving leg would grow the journal indefinitely. Bound by age as well. |
| An event journal bounded but **not** expired | A stored bound keeps history; it does not keep events *true*. An event that has stopped meaning anything still counts. See [`event-lifecycle.md`](event-lifecycle.md). |
| Directional colour as a status colour | Amber signals caution on any axis; green and red never do. |
| Clamping `progress` to `[0, 1]` | Destroys both readings that matter: price moved backwards, and the target was overrun. |
| Treating a target revision as movement | A change in `progress` with a change in `target_version` is not evidence that price travelled. |
| Reactivating an expired event record | Reviving a record lets one divergence be counted twice. Reactivation creates a new identity. |
| Naming any axis or reading "regime" | Regime means a latent, persistent, inferred state. This layer is directly observed. The name is reserved (§5.9). |
| Reporting stage from an unconfirmed origin | The origin can be revised backwards. A `forming` leg publishes no stage. |
| A terminality label that names a mood | `resolving` reads as *the leg is ending* and as *uncertainty is clearing*. Label the **direction of the claim** instead — see §5.9. |
| Reading terminality as completion | Terminality is a re-evaluated assessment. Completion is the discrete event that ends the leg. They are separate outputs. |
| A progression reading with no stated scale | A minor leg inside a major one has a perfectly valid origin, target and progress. Without `structure_level` the reading cannot be checked. |
| Guarding `T - O` with a default | A substituted denominator yields a number that looks like a measurement and is not one. `UNDEFINED` is the only valid result. |
| Widening the stability window because stages stopped publishing | Stability exists to keep the reference honest. Forcing it manufactures certainty the system does not have. |

---

## 12. Build order

Ordered by dependency, not by size.

| # | Work | Spec | Axis | Blocked by |
|---|---|---|---|---|
| 1 | **Retention** — Tier A series on every indicator | [`retention.md`](retention.md) | all | nothing |
| 2 | **Swing retention** — keep the pivots already computed | [`retention.md`](retention.md) | Structure, Progression | 1 |
| 3 | **Current-leg record** — Tier D, versioned, with `occurred_at` / `available_at` | [`leg.md`](leg.md) | Progression | 2 |
| 4 | **Progression arithmetic** — `progress`, `target_version`, `target_stable`, stage classification | [`leg.md`](leg.md) | Progression | 3 |
| 5 | **Event lifecycle** — activation, persistence, expiration, invalidation, reactivation | [`event-lifecycle.md`](event-lifecycle.md) | Structure | 2 |
| 6 | **Causal replay harness** — assert no field is read before its `available_at` | [`leg.md`](leg.md) §3 | all | 3, 5 |
| 7 | **Derived readings** — rate-of-change, R², efficiency | [`framework.md`](framework.md) §6 | Direction, Structure | 1 |
| 8 | **New indicators** — the 84 additions | [`indicators/`](indicators/) | all | 1–7 |

**Steps 1–6 require no new indicator at all.** They are retention, bookkeeping and arithmetic
over data already held, and they are the part that is expensive to retrofit: an
`available_at` that was never recorded cannot be recovered from a candle archive, and an event
lifecycle that was never applied cannot be reconstructed from a journal of expired events.

**Step 6 is a gate, not a task.** Nothing downstream is trustworthy until movement identity,
progression estimates and event lifecycles have been shown to behave consistently under causal
replay.
