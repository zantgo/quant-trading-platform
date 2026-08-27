# Strategies Builder (UI) — Spec (v9)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Locked for implementation
**Surface:** Profile tab 2 (`strategies`) — `StrategyListPanel.svelte` +
`StrategyEditorPanel.svelte` (+ per-section sub-components, ≤1000 lines
each). Full CRUD: create, clone, edit, delete, export JSON, import JSON —
CLI-compatible format (see 06-00-cli-gui-parity.md).

## List panel

Card grid: name, description, `base` chip, active-instance count,
`schema_version`. `default` card locked (no delete). Actions: Create
(clone from `default` or any strategy) · Clone · Edit · Delete (confirm;
blocked with error when bound to instances) · Export JSON · Import JSON
(validate → errors rendered inline).

## Editor panel

Two panes:

```
┌─ Tree nav ───────────────┬─ Editor canvas ──────────────────────────┐
│ l1  Metrics              │  Section forms per node; provenance chip │
│ l1_5 Derivatives         │  per field: INHERITED (dimmed, base ref) │
│ l2  Alignment            │  vs OVERRIDDEN; "reset to inherit" per   │
│ l2_5 Liquidity Synthesis │  field; Advanced accordions collapsed    │
│ l3  Analysis             │  (l2.5/l3/l4/l5/l6 internals,            │
│ l4  Opportunity          │  probability group, Wyckoff,             │
│ l5  Risk                 │  edge_classification)                    │
│ l6  Decision             ├─ JSON view tab ──────────────────────────┤
│ l7  Overview             │  rendered raw JSON + copy/export +       │
│ tae Execution            │  import validation errors                │
│ pme Portfolio            │                                          │
│ pae Verdict              │                                          │
└──────────────────────────┴──────────────────────────────────────────┘
```

- Header: single `SettingsSaveButton` (idle→dirty→saving→saved|error) +
  Export + `ConfigSourceChip`.
- Save posts the strategy JSON; server responds 200/400 +
  `warnings: []` (coherence checks: setup enabled vs indicator disabled,
  `min_net_rr` vs fees, unreachable regime rows) → `WarningBanner`.
- Sections map 1:1 to the locked JSON (see
  `03-02-17-mme-strategy-config.md` §3).

## Settings strip-down (same phase)

| Surface | After |
|---|---|
| `WorkspaceSettings.svelte` | Timeframes only |
| `TradeAutomationSettings.svelte` | Instance-scoped only: bound strategy selector (recharge confirm copy), `allocation_pct` override, `vol_scale` auto badge + manual override |
| PME / PAE Settings tabs | Removed (Safety tab keeps resets; PAE Methodology stays) |
| `BacktestLauncher.svelte` | New Strategy step (pick + read-only summary); allocation input removed; capital field = `portfolio_capital_usd` |

## Conventions

Svelte 5 runes · `*.module.css` (kebab-case → camelCase) · `*.test.ts` ·
no hardcoded numbers in panels — everything reads the strategy JSON.
