# M8 — `readings[]` hardcoded empty

**Status:** injected → detected → repaired, this cycle.
**Invariant:** V-201 — an axis publishes the typed signal history it has.
**Gate:** `crates/market-analyzer/tests/axis_readings.rs`.

---

## 1. Named

| | |
|---|---|
| **Defect** | The assembler builds the history and then throws it away |
| **Site** | `crates/market-analyzer/src/model/mod.rs:189`, inside `assemble_axes` |
| **Injected form** | the `for signal in &axis_signals[i] { state.push_reading(…) }` loop replaced with `let _ = &axis_signals[i];` |
| **Correct form** | push every filed signal, letting `AxisState::push_reading` enforce the cap |

This is the original defect, removed and restored. As shipped through P10 step 1:

```rust
AxisState { axis: *axis, position, size, readiness, readings: Vec::new() }
```

`AxisState::push_reading` existed, was documented, capped at 50, and had **zero
production callers** — it appeared only in `core-domain/tests/axes_contract.rs`.
So every published `TimeframeState` asserted that each axis had *no observable
typed signal history*, on every candle, for the life of the platform.

## 2. Isolated

One loop in a scratch copy of `model/mod.rs`, never production at rest:

```
cp crates/market-analyzer/src/model/mod.rs /tmp/opencode/model_mod.rs.orig
```

## 3. Demonstrated failing

**Seven of twelve assertions fired.**

```
---- v201_the_published_axis_carries_its_signal_history stdout ----
thread '...' panicked at crates/market-analyzer/tests/axis_readings.rs:99:5:
no axis carried any signal history — `readings[]` is hardcoded empty again (V-201)
```

```
---- v201_the_pipeline_kind_is_preserved_typed_not_as_a_word stdout ----
thread '...' panicked at crates/market-analyzer/tests/axis_readings.rs:149:5:
the pipeline kind must survive onto the model signal as a typed value: []
```

```
---- v203_every_reading_carries_the_bar_it_fired_on_and_its_anchor stdout ----
thread '...' panicked at crates/market-analyzer/tests/axis_readings.rs:237:5:
assertion `left == right` failed: three signals with three different ages: []
  left: 0
 right: 3
```

```
test result: FAILED. 5 passed; 7 failed
```

The five that still passed are the ones about the *choke point* rather than the
emitted document — `v202_push_reading_trims_from_the_oldest_end` calls
`push_reading` directly, and the two `v202` cap tests pass vacuously when
nothing is ever pushed. That asymmetry is the point of the mutation protocol:
it shows which assertions are load-bearing.

## 4. Demonstrated repaired

```
$ cp /tmp/opencode/model_mod.rs.orig crates/market-analyzer/src/model/mod.rs
$ cargo test -p market-analyzer --test axis_readings
test result: ok. 12 passed; 0 failed
```

---

## The defect this mutation was written for

The gate suite is **13/13 green** against a model whose published `readings[]` is
empty on every candle. That is not a contradiction — it is the reason this phase
exists. Every causal gate asserts on the *assembler's inputs* and on the
*reduction laws*; none asserted on what the assembler *emitted*. A gate that
passes while the shipped document says "this axis has no signal history" is a
gate that cannot fail.

**V-248 … V-252 (P10-K) re-express every CG gate against the live path.** Until
that lands, the 13/13 figure should be read as "the reduction laws hold", not as
"the product is correct".

## A real defect this phase's own test found

`v204_a_duplicate_signal_does_not_inflate_the_history` failed on its **first
run**, before any mutation. The producer filed the same occurrence twice when an
indicator reported the same `(kind, age)` more than once — one divergence
inflating the axis history, which is precisely the C/Q4 violation the test
exists to catch.

Fixed in `signals_by_axis` by deduplicating on `(producer, kind, at_bar)` — the
part of the canonical event identity (01-15) resolvable from a single snapshot.
Note the distinction the fix preserves: **two indicators** firing the same kind
on the same bar are two facts and are both filed; **one indicator** reporting
the same kind on the same bar twice is one fact reported twice and is filed
once.

A test written from the specification caught a defect in the implementation on
first execution. That is the property S2 is for.