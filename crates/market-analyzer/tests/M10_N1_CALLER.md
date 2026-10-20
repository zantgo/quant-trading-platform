# M10 — an assembler survives with no production caller

**Status:** injected → detected → repaired, this cycle.
**Invariant:** V-216, N1 — every reduction has at least one caller outside
`#[cfg(test)]`; a reduction that only tests invoke is a specification, not a
product.
**Gate:** `crates/market-analyzer/tests/n1_production_callers.rs`.

---

## 1. Named

| | |
|---|---|
| **Defect** | `assemble_cross_tf_state` is called only from tests; the daemon's L7 task never builds level 2 |
| **Site** | `crates/execution-daemon/src/main.rs`, the per-instance L7 loop |
| **Injected form** | the production call renamed `assemble_cross_tf_state_REMOVED(` |
| **Correct form** | `market_analyzer::model::assemble_cross_tf_state(&inst.symbol(), &durations)` |

Through P10-C all three reductions (`assemble_cross_tf_state`,
`assemble_cross_pair_state`, `assemble_model_state`) were exercised by the
aggregation suite and **never by the binary**. That is precisely the failure
mode N1 names: the model can be correct in tests while production answers an
empty overview, and no behavioural assertion can see the absence — there is no
wire payload to disagree with.

## 2. Isolated

The backup and the edit are one source expression; nothing else in the tree
changed:

```
cp crates/execution-daemon/src/main.rs /tmp/opencode/main.rs.p10d.bak
perl -pi -e 's/market_analyzer::model::assemble_cross_tf_state\(/…_REMOVED(/' \
  crates/execution-daemon/src/main.rs
```

The gate scans `crates/*/src/**` only, so integration tests and the `target/`
tree cannot supply the missing caller. (An earlier draft scanned all of
`crates/`, including `tests/`, and **survived the injection** — the test was
counting its own sibling suites. The first repair was to the gate; the injection
was then re-run.)

## 3. Demonstrated failing

**The targeted assertion fired, with the exact count:**

```
---- every_reduction_has_a_production_caller ----
panicked at crates/market-analyzer/tests/n1_production_callers.rs:77:
N1: `assemble_cross_tf_state` has the definition but no production call site
(found 1; the threshold is definition + >=1 caller in crates/*/src)
test result: FAILED. 0 passed; 1 failed
```

`1` is the definition itself. The other two assemblers stayed at their
definition-plus-caller counts, which is the scope check: the mutation removed
one caller, not the concept of calling.

## 4. Demonstrated repaired

```
cargo test -p market-analyzer --test n1_production_callers
test result: ok. 1 passed; 0 failed; 0 ignored
```

The daemon now builds `PairAssembly` per active instance from the completed
snapshots' `timeframe_state`, calls `assemble_cross_tf_state` for level 2 and
`assemble_model_state` for the envelope, stores the result in
`AppState.model_state`, and reuses the envelope's `cross_pair_state` in
`/api/overview`. All three reductions have a production caller.
