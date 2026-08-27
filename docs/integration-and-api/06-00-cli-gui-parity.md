# CLI ↔ GUI Feature Parity — Contract (v9)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Locked for implementation

The CLI is the machine-checkable mirror of the GUI. Its "visualization" is
structured logging + saved JSON artifacts. Every GUI action has a CLI twin
producing the same JSON an export would.

| GUI surface | CLI command | JSON artifact |
|---|---|---|
| Account home | `--account-summary` | stdout JSON |
| Capital set (paper) | `--account-set-capital <usd>` | confirmation JSON |
| Portfolio reset | `--account-reset` | confirmation JSON |
| Strategies list | `--strategy-list` | table + `strategies.json` |
| Strategy create/update | `--strategy-create <file.json>` · `--strategy-update <name> <file.json>` | saved strategy JSON |
| Strategy delete/clone | `--strategy-delete <name>` · `--strategy-clone <src> <dst>` | confirmation JSON |
| Strategy export/import | `--strategy-export <name> [path]` · `--strategy-import <path>` | strategy JSON file |
| Instance strategy bind | `--instance-set-strategy <id> <name>` | recharge confirmation |
| Instance lifecycle | `--instance-start/pause/terminate <id>` | status JSON |
| Backtest launcher | `--backtest --strategy <name> --portfolio-capital <usd> --exchange … --symbols … --tf … --depth …` | run envelope + DS tables |
| Backtest history | `--backtest-list` · `--backtest-show <id>` | run JSON |
| Coverage | `--backtest-coverage` | coverage JSON |

**Parity invariant:** GUI-initiated and CLI-initiated equivalent runs (same
strategy JSON + same window) produce byte-identical run content (modulo
timestamps).

**CLI backtest flags (v9):** `--portfolio-capital` replaces `--capital`;
`--strategy <name>` (default `default`); NHST params are derived from the
bound strategy's `pae.verdict` bar (no separate flags).
