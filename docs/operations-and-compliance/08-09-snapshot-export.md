# Snapshot Export Operator Manual

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Audience:** Operators configuring the periodic JSON dump that feeds the offline data-science pipeline.

---

## 1. What this does

The snapshot export is a daemon-owned background task that periodically writes one JSON file per
**(instance, timeframe, tab)** triple into a configured directory tree. The output is the raw input for
offline data science and strategy development — exactly the same matrices that the dashboard's
"Export Data" button emits per-tab, but written automatically and without operator intervention.

```
[Market Monitor daemon]
  └── Snapshot Export task (every N seconds, hot-reloadable)
        └── writes <output_path>/<YYYY-MM-DD>/<HHhMMmSSs>/<pairKey>.<slot>.<tab>.json
              for every active instance × every TF slot × every enabled tab
```

The task is configured by two paths (both write the same `SnapshotExportRuntime` shared with the
daemon's HTTP handlers):

| Path | Use when |
|---|---|
| **GUI modal** (bottom-left "SCHEDULE SNAPSHOTS" button on the Market Overview dashboard) | The operator has the dashboard open and wants to toggle the schedule interactively. |
| **CLI `--save` flag** (`cargo run --bin execution-daemon -- --mode cli --save`) | The operator is on a headless machine and wants the terminal monitor to also write JSON dumps. `--save` enables the task even when `[snapshot_export]` is disabled in `config.toml`. |

The two paths share the same on-disk state — `config.toml` is the boot-time source of truth, the GUI
modal is the runtime source of truth, and `GET /api/snapshot-export/status` exposes both as a single
JSON document (CLI mode has no HTTP surface; `--save` simply flips the runtime flag at boot).

---

## 2. Quick start

### 2.1 GUI

1. Open the dashboard at <http://127.0.0.1:3000> and ensure at least one workspace instance is running.
2. Scroll to the bottom of the Market Overview page.
3. Click the new **"SCHEDULE SNAPSHOTS"** button (immediately to the left of "SCAN WATCHLIST").
4. In the modal:
   - Toggle **Enable**.
   - Adjust the **Output directory** if `./snapshots` isn't right.
   - Set **Interval (seconds)** — 60s is a sensible default. Floor 5s, ceiling 3600s.
   - (Optional) Tweak **Max snapshots retained** — the scheduler prunes the oldest timestamped
     directories once this threshold is reached. Default 1000.
   - (Optional) Untick tabs you don't want — the per-tab count defaults to all 9.
5. Click **Save** — the change takes effect on the next tick (no daemon restart).
6. Click **Run Now** if you want an immediate snapshot.

### 2.2 CLI (`--mode cli`)

```bash
# 1. Launch the terminal monitor with snapshot export enabled.
cd /path/to/quant-trading-platform
cargo run --bin execution-daemon -- --mode cli --save

# ── Interactive launch prompts ─────────────────────────────────────
# Exchange (hyperliquid / bitget) [hyperliquid]: hyperliquid
# Settlement currency forced to USDC for hyperliquid.
# Instance #1 base symbol (blank = done) [BTC]: BTC
#   micro timeframe_secs (default 60s): 60
#   fast timeframe_secs (default 300s): 300
#   slow timeframe_secs (default 900s): 900
#   macro timeframe_secs (default 3600s): 3600
# Instance #2 base symbol (blank = done):            ← Enter finishes
# ── Summary ───────────────────────────────────────────────────────
# 💾 Snapshot export ENABLED (--save) → ./snapshots
```

The CLI's instances are validated against the live exchange REST endpoint by
`registry::add_instance` (same call the GUI's `POST /api/instances` makes). Snapshots land in
`<output_path>` (default `./snapshots`) exactly as described in §3.

The retired `setup` subcommand (`--dry-run`, `--auto-start`, `--sub status`) was removed in v7.2 —
see [`docs/conceptual-foundations/01-09-cli-setup-flow.md`](../conceptual-foundations/01-09-cli-setup-flow.md)
for the replacement flow.

---

## 3. On-disk file layout

Every snapshot tick creates one subdirectory per UTC timestamp, with one JSON file per
`(pairKey, slot, tab)` triple:

```text
<output_path>/
  2026-08-13/
    14h30m05s/                          ← one tick (UTC timestamp)
      BTC-USDT.micro.alignment.json
      BTC-USDT.micro.analysis.json
      BTC-USDT.micro.advisory.json
      BTC-USDT.micro.decision.json
      BTC-USDT.micro.metrics.json
      BTC-USDT.micro.mtf.json
      BTC-USDT.micro.opportunity.json
      BTC-USDT.micro.recommendation.json
      BTC-USDT.micro.risk.json
      BTC-USDT.fast.alignment.json
      ...
      BTC-USDT.slow.alignment.json
      ...
      BTC-USDT.macro.alignment.json
      ...
      ETH-USDT.micro.alignment.json
      ...
```

At default 60s cadence × 9 tabs × 4 TF slots × N instances, that's `36 × N` files per minute. The
**Max snapshots retained** knob bounds the total: at 1000 snapshots retained and 60s cadence, the
directory tree grows to ~24 hours of history, then rotates.

Each file is a JSON document with this top-level envelope:

```json
{
  "snapshot_metadata": {
    "datetime_utc": "2026-08-13T14:30:05.123456+00:00",
    "timestamp_ms": 1755090605123,
    "tab": "alignment",
    "pair_key": "BTC-USDT",
    "timeframe_slot": "slow",
    "timeframe_secs": 900
  },
  "payload": { /* the AlignmentMatrix / AnalysisMatrix / etc. */ }
}
```

Data-science consumers can glob (`<output>/**/*.json`) and join on `snapshot_metadata.timestamp_ms`
+ `pair_key` + `timeframe_slot` + `tab`.

---

## 4. Configuration reference

The schedule lives at the top level of `config.toml` under `[snapshot_export]`:

```toml
[snapshot_export]
enabled = true                          # master toggle
output_path = "./snapshots"             # created at startup if missing
interval_secs = 60                      # floor 5, ceiling 3600
max_snapshots_retained = 1000           # prune oldest when exceeded
# tabs = [...]                         # omitted == all 9 tabs
```

### 4.1 Defaults

| Field | Default | Notes |
|---|---|---|
| `enabled` | `false` | First-boot safety. Operator must opt in. |
| `output_path` | `"./snapshots"` | Relative to daemon CWD. Resolved to absolute on startup. |
| `interval_secs` | `60` | Clamped to `[5, 3600]`. |
| `max_snapshots_retained` | `1000` | Clamped to `[10, 100000]`. |
| `tabs` | all 9 | `metrics`, `mtf`, `alignment`, `opportunity`, `risk`, `analysis`, `advisory`, `decision`, `recommendation`. Unknown tab IDs are silently dropped. |

### 4.2 Per-tab selection

The 9 tabs are exhaustive — every per-TF matrix the engine publishes is covered. The `tabs`
field accepts any subset of these IDs:

```toml
[snapshot_export]
tabs = ["metrics", "alignment", "risk"]   # only emit these three
```

If `tabs = []` is provided the scheduler falls back to all 9 (with a warning in `engine.log`).

---

## 5. Hot-reload & runtime contract

`SnapshotExportRuntime` is held in a single `Arc<RwLock<…>>` shared between:

- the periodic task (reads config every tick),
- the HTTP handlers (`/api/snapshot-export/status`, `.../config`, `.../run-now`),
- the GUI modal (`SnapshotSchedulerModal.svelte`),
- the CLI `--status` command.

Mutations via `PUT /api/snapshot-export/config` are picked up on the next tick — **no daemon restart
required**. Mutations to `config.toml` directly require a restart (the boot-time hydration is one-shot).

The `POST /api/snapshot-export/run-now` endpoint fires a `tokio::sync::Notify` that wakes the task for
an immediate tick. The next scheduled tick proceeds as usual.

---

## 6. REST API

| Method | Path | Body | Description |
|---|---|---|---|
| `GET` | `/api/snapshot-export/status` | — | Returns the live `SnapshotExportRuntime` as JSON. |
| `PUT` | `/api/snapshot-export/config` | `SnapshotExportConfigPatch` | Partial-patch the runtime. All fields optional; omitted fields are unchanged. |
| `POST` | `/api/snapshot-export/run-now` | — | Forces an immediate tick. Returns the destination path + a confirmation message. |

`SnapshotExportConfigPatch` shape (every field optional):

```json
{
  "enabled": true,
  "output_path": "/data/snapshots",
  "interval_secs": 30,
  "max_snapshots_retained": 500,
  "tabs": ["alignment", "risk"]
}
```

Validation rules (clamped, not rejected):

- `output_path` empty or whitespace-only → 400.
- `interval_secs` outside `[5, 3600]` → clamped.
- `max_snapshots_retained` outside `[10, 100000]` → clamped.
- `tabs` filtered to the canonical 9; unknown IDs silently dropped; empty list falls back to all 9.

---

## 7. CLI reference

The retired `setup` subcommand was removed in v7.2. Snapshot export in terminal-only
deployments is enabled with the `--save` flag on the CLI monitor mode:

```bash
# Terminal monitor with snapshot JSON dumps enabled (default output ./snapshots).
execution-daemon --mode cli --save

# Custom interval + custom config.
execution-daemon --mode cli --save --interval 10 --config /path/to/config.toml
```

The CLI launch prompts (in order):

1. **Exchange** — `hyperliquid` (default) or `bitget`. Unknown values are rejected.
2. **Settlement currency** — forced per exchange (HL=USDC, Bitget=USDT).
3. **Instances** — repeated: base symbol + per-TF `timeframe_secs` (`[10, 86400]`), until
   blank. Pre-seeded from `workspace.instances[]` — pressing Enter keeps them.
4. **Confirm** — instances just run; the L7 overview redraws in the terminal.

`--save` flips the snapshot-export runtime flag at boot (even when `[snapshot_export]` is
disabled in `config.toml`); the output path defaults to `./snapshots`. The GUI modal remains
the runtime source of truth for web deployments — both paths write the same files.

---

## 8. Troubleshooting

### 8.1 The modal shows `loading…` forever

The `SnapshotSchedulerButton` polls `GET /api/snapshot-export/status` every 3s. If the modal shows
the loading placeholder:

- Confirm the daemon is running (`./manage.sh status`).
- Open the browser dev console and check for HTTP errors on `/api/snapshot-export/status`.
- Tail `engine.log` — the snapshot-export task logs a startup banner on launch.

### 8.2 Status shows `ERROR` (red dot)

The scheduler caught an error on the last tick and stored it in `runtime.last_error`. The modal's
"Last error" block shows the message. Common causes:

- **`write <path>: permission denied`** — the daemon can't write to `output_path`. Check
  filesystem permissions. The CLI defaults to `./snapshots` (relative to daemon CWD); if the daemon
  is launched via systemd with `WorkingDirectory=/var/lib/trading-platform`, the path resolves
  to `/var/lib/trading-platform/snapshots`.
- **`create_dir_all: <path>: No such file or directory`** — the parent of `output_path` doesn't
  exist. The scheduler creates `output_path` at startup, but a missing parent is a pre-condition
  the scheduler can't fix on its own. Create the parent directory and re-enable.

### 8.3 No files appear despite `enabled: true`

- Confirm the workspace has at least one running instance (the scheduler iterates
  `workspace.list()` per tick).
- Confirm each instance has at least one TF slot with a recent `MarketSnapshot` (the scheduler
  only emits files for slots that have a non-`None` snapshot). The dashboard's bottom-right
  per-instance status pill shows the latest snapshot age.
- Check `last_instance_count` in the modal — should be ≥ 1.

### 8.4 Disk fills up too fast

Lower `max_snapshots_retained`. The default 1000 snapshots × 36 (instance × slot × tab) files per
snapshot × ~5KB per file = ~180MB. For a single-instance dashboard this is harmless; for a 20-instance
production deployment, lower the retention to 200 (~72MB) or move the output to a dedicated volume.

---

## 9. Cross-references

- [`docs/integration-and-api/06-01-api-gateway-contract.md`](../integration-and-api/06-01-api-gateway-contract.md) §2 — REST endpoint table.
- [`docs/integration-and-api/06-03-snapshot-export-schema.md`](../integration-and-api/06-03-snapshot-export-schema.md) — Per-tab on-disk JSON schema reference.
- [`docs/conceptual-foundations/01-09-cli-setup-flow.md`](../conceptual-foundations/01-09-cli-setup-flow.md) — Full text + UX rationale of the CLI setup flow.
- [`crates/core-domain/src/snapshot_export.rs`](../../crates/core-domain/src/snapshot_export.rs) — Shared types (`SnapshotExportRuntime`, `ALL_TABS`, `runtime_from_config`).
- [`crates/execution-daemon/src/snapshot_export.rs`](../../crates/execution-daemon/src/snapshot_export.rs) — Periodic task implementation.
- [`crates/api-gateway/src/handlers/snapshot_export.rs`](../../crates/api-gateway/src/handlers/snapshot_export.rs) — HTTP handlers.
- [`ui/src/components/SnapshotSchedulerButton.svelte`](../../ui/src/components/SnapshotSchedulerButton.svelte) — Bottom CTA button.
- [`ui/src/components/SnapshotSchedulerModal.svelte`](../../ui/src/components/SnapshotSchedulerModal.svelte) — Configuration modal.

## 5. Grace-band validation sweep (v6.10.16)

The snapshot corpus is the offline data path for the L3 bias **grace-band validation sweep** — the institutional calibration tool that answers "is the (15, 20] band better than (10, 20], and is the grace hypothesis directionally accurate?" with evidence instead of preference:

```
cargo run -p core-domain --example grace_sweep -- <snapshot_dir>
```

For every snapshot tick the sweep re-derives the L3 bias under each swept constant set (band edges {10, 12, 15, 18} × 20, vote ratios {2/4, 3/4, 4/4}, agreement {60, 75, 90}, signal breadth {2, 3}) and labels each directional call against the **forward price** of the same pair (horizons 1/3/6/12 samples). Output: directional accuracy, coverage, and flip rate (Bullish↔Neutral↔Bearish transitions per sample) per rule, plus the engine's shipped rule (grace + hysteresis) vs its no-hysteresis twin.

**Process rule:** widen the band (e.g. to (10, 20]) only if the sweep shows graced accuracy in the lower zone at or above plain-threshold accuracy — expected to be below, because a composite in (10, 15] is *genuinely weak* (trend/momentum near zero), not drag-suppressed like (15, 20]. The full constant re-tune should use a time-split holdout (first 70% calibration, last 30% validation). No snapshots yet? Enable `[snapshot_export]` in `config.toml` and let the daemon run; each tick writes one envelope per tab.
