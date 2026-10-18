# M11 — `state_label` reintroduced as a decision key

**Status:** injected → detected → repaired, this cycle.
**Invariant:** V-108 — `state_label` is display output, never a decision input.
**Gate:** `crates/market-analyzer/tests/no_state_label_key.rs`.

---

## 1. Named

| | |
|---|---|
| **Defect** | The model path branches on a free-text label instead of a typed value |
| **Site** | `crates/market-analyzer/src/model/live.rs:135`, inside `progression_stage_of` |
| **Injected form** | `if value.state_label.contains("coiling") { return Some(Stage::Late); }` |
| **Correct form** | `Stage::from_label(&value.state_label)` |

This is the shipped defect class, not a synthetic shape. Before this batch,
`progression_stage_of` was itself a five-arm `match` over `state_label` string
literals — so the *ordinal*, the one value the model guarantees has no
arithmetic (V-37/V-38 prove `stage + 1` will not compile), entered the type
system through a `String` that nothing type-checked. The type-level guarantee
held and the guarantee was void.

The mutant is the natural next step from there: add one convenience branch that
maps a *regime* word onto a *stage*. It reads as reasonable, it compiles, and it
makes `coiling` silently mean `Late`.

## 2. Isolated

One function in a scratch copy of `live.rs`, never production at rest:

```
cp crates/market-analyzer/src/model/live.rs /tmp/opencode/live.rs.orig
```

## 3. Demonstrated failing

```
---- v108_no_model_code_branches_on_a_state_label stdout ----
thread '...' panicked at crates/market-analyzer/tests/no_state_label_key.rs:156:5:
state_label is display output, not a decision input (V-108). Offenders:
  crates/market-analyzer/src/model/live.rs:135 — `.contains()` on state_label:
  `if value.state_label.contains() { return Some(Stage::Late); }`

test result: FAILED. 3 passed; 1 failed
```

The message names the file, the line, the shape, and quotes the offending code.

## 4. Demonstrated repaired

```
$ cp /tmp/opencode/live.rs.orig crates/market-analyzer/src/model/live.rs
$ cargo test -p market-analyzer --test no_state_label_key --test typed_dispatch
test result: ok. 4 passed; 0 failed
test result: ok. 8 passed; 0 failed
```

---

## The thing worth recording

**No other test in the suite caught this mutant.** `typed_dispatch.rs` runs in
the same binary and passed all 8 of its assertions while the mutant was live —
its `v106_every_signal_kind_has_a_decided_classification` test checks signal
kinds, not labels.

That is the argument for having this as a dedicated sweep rather than trusting
review. A semantic test asserts what a function *does*; a branch on a free-text
word is invisible to every assertion you would naturally write about that
function's behaviour, because the function still returns a `Stage` and the
type is still `Option<Stage>`. Only a sweep over *how* the value was obtained
sees it.

## Scope note — what this gate does **not** claim

The sweep covers `market-analyzer/src/{model,leg,replay,indicators}`. Six
production `state_label` comparisons exist elsewhere and are **not** covered:

| site | retired by |
|---|---|
| `api-core/src/types.rs:419` (`contains("CROSSOVER")`) | P10-J (UI block removal) |
| `core-domain/src/alignment.rs:181,183` | **V-225** — alignment is rewritten as 4 axis rows |
| `core-domain/src/risk.rs:540` (`contains("COMPRESSION")`) | **V-228…V-231** — `AnalysisMatrix` dissolved |
| `analyzer/mod.rs:3397` (`== "COMPRESSION_COILING"`) | **V-110/V-111** — CR-11, regimes → diagnostics |

Sweeping them now would make the gate assert something that is not yet true.
The gate states what is true today; the table states what retires the rest, so
the remaining work is tracked rather than quietly unenforced.