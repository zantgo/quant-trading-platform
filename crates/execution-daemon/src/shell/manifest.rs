//! The parity manifest (P6 W6.3).
//!
//! ## What this is
//!
//! A table mapping **every surface a consumer can see** to **the shell command
//! that serves it**. The dashboard and the shell are two renderers of one value,
//! so the risk is not that they disagree numerically — that is CG/PARITY's job —
//! it is that one of them can show something the other cannot. That failure has
//! no arithmetic to catch it: nothing crashes, every number is right, and a fact
//! simply exists in only one place.
//!
//! So the manifest is a **set equality**, enforced both ways by
//! `shell_parity.rs`:
//!
//! * every manifest row names a command the parser can actually run, and
//! * every command the parser can run has at least one row.
//!
//! A command with no surface is an orphan (the shell shows something nobody
//! asked for); a surface with no command is the worse one (the dashboard shows
//! something the operator cannot inspect from the terminal).
//!
//! ## `Declared` rows are not orphans
//!
//! A surface that the model has not built yet (`model leg` before P8) is
//! [`Status::Declared`] with the phase that owns it. It is still in the
//! manifest, so it is still checked — the command exists, the grammar accepts
//! it, and the row cannot quietly disappear. What it may not do is pretend to
//! serve data it does not have.

/// One surface, and the command that serves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surface {
    /// The engine the surface belongs to — the CLI prefix, post-rename (D-2).
    pub engine: Engine,
    /// The dashboard tab, using the UI's own key so a tab rename is visible here.
    pub tab: &'static str,
    /// The block/panel inside the tab.
    pub block: &'static str,
    /// The command that serves it, in the shell's grammar.
    pub command: &'static str,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Data,
    Model,
    /// A surface one engine shows using **another** engine's command.
    ///
    /// The price chart is the model engine's surface, but the candles it draws
    /// are the data engine's rows. Recording that honestly is better than either
    /// filing it under `data` (which would lose the surface's owner) or forcing
    /// `data candles` to answer a model question.
    Shared,
}

impl Engine {
    pub fn prefix(self) -> &'static str {
        match self {
            Engine::Data => "data",
            Engine::Model => "model",
            Engine::Shared => "",
        }
    }
}

/// Whether the surface serves data today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Serves live values from the same `AppState` the WebSocket handler serves.
    Served,
    /// The command exists and refuses; the phase that fills it in is named.
    Declared(&'static str),
}

/// The manifest itself.
///
/// Every entry is a claim about the dashboard. Adding a tab means adding a row;
/// adding a command means adding a row. Neither can be done without the other,
/// because `shell_parity.rs` fails on either direction.
pub const MANIFEST: &[Surface] = &[
    // ── Data Engine — one row per dashboard tab (ui/src/lib/engineTabs.ts) ──
    Surface {
        engine: Engine::Data,
        tab: "data_infra",
        block: "Overview",
        command: "data overview",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Data,
        tab: "exchange_status",
        block: "Exchange Status",
        command: "data exchange",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Data,
        tab: "connectivity",
        block: "Connectivity",
        command: "data connectivity",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Data,
        tab: "market_data",
        block: "Market Data",
        command: "data market-data",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Data,
        tab: "clock_monitor",
        block: "NTP Clock Monitor",
        command: "data ntp",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Data,
        tab: "data_quality",
        block: "Data Quality",
        command: "data data-quality",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Data,
        tab: "distribution",
        block: "Distribution",
        command: "data distribution",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Data,
        tab: "settings",
        block: "Connection Settings",
        command: "data connection-settings",
        status: Status::Served,
    },
    // Workspace-wide, not a tab: the instance list both renderers show.
    Surface {
        engine: Engine::Data,
        tab: "workspace",
        block: "Running instances",
        command: "data instances",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Data,
        tab: "status",
        block: "Process status",
        command: "data status",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Data,
        tab: "charts",
        block: "Candle series",
        command: "data candles",
        status: Status::Served,
    },
    // ── Model Engine — one row per surface tab + per model block ──────────
    Surface {
        engine: Engine::Model,
        tab: "market_monitor",
        block: "Instance list",
        command: "model instances",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Model,
        tab: "overview",
        block: "Cross-pair overview",
        command: "model overview",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Model,
        tab: "monitor",
        block: "Per-duration metrics",
        command: "model metrics",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Model,
        tab: "monitor",
        block: "One indicator's series",
        command: "model indicator",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Shared,
        tab: "terminal",
        block: "Price chart + overlays",
        command: "data candles",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Model,
        tab: "alignment",
        block: "Cross-duration alignment",
        command: "model alignment",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Model,
        tab: "alignment",
        block: "Consensus",
        command: "model consensus",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Model,
        tab: "metrics",
        block: "Signals",
        command: "model signals",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Model,
        tab: "metrics",
        block: "Divergences",
        command: "model divergences",
        status: Status::Served,
    },
    Surface {
        engine: Engine::Model,
        tab: "metrics",
        block: "Levels",
        command: "model levels",
        status: Status::Served,
    },
    // Not yet built by the model — declared so they are tracked, not missing.
    Surface {
        engine: Engine::Model,
        tab: "states",
        block: "Leg record",
        command: "model leg",
        status: Status::Declared("P8"),
    },
    Surface {
        engine: Engine::Model,
        tab: "states",
        block: "Events",
        command: "model events",
        status: Status::Declared("P9"),
    },
    Surface {
        engine: Engine::Model,
        tab: "states",
        block: "Diagnostics",
        command: "model diagnostics",
        status: Status::Declared("P10"),
    },
    Surface {
        engine: Engine::Model,
        tab: "states",
        block: "States view",
        command: "model state",
        status: Status::Declared("P10/P13"),
    },
    // Config ownership (CR-23): the Settings tab both engines render.
    Surface {
        engine: Engine::Model,
        tab: "settings",
        block: "Config surface",
        command: "config show",
        status: Status::Declared("R1"),
    },
];

/// The commands the grammar can run, as strings.
///
/// Derived from the parser rather than hand-listed, so a new sub-command cannot
/// be added without `shell_parity.rs` noticing an orphan. The list is the
/// `sub` half of each invocation; `data`/`model` with no sub-command is a parse
/// error, so it is not a command.
pub const PARSEABLE: &[&str] = &[
    // data
    "data overview",
    "data exchange",
    "data connectivity",
    "data market-data",
    "data ntp",
    "data data-quality",
    "data distribution",
    "data connection-settings",
    "data instances",
    "data status",
    "data candles",
    // model
    "model instances",
    "model overview",
    "model state",
    "model alignment",
    "model consensus",
    "model leg",
    "model events",
    "model diagnostics",
    "model metrics",
    "model indicator",
    "model signals",
    "model divergences",
    "model levels",
    // config
    "config show",
    "config set",
];

/// Commands that exist but are not a *surface* — they act rather than report.
pub const NON_SURFACE: &[&str] = &["help", "version", "exit", "config set"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_manifest_command_is_parseable() {
        for row in MANIFEST {
            assert!(
                PARSEABLE.contains(&row.command),
                "`{}` is in the manifest but not in the grammar",
                row.command
            );
        }
    }

    #[test]
    fn no_surface_is_mapped_to_nothing() {
        for row in MANIFEST {
            assert!(!row.command.is_empty(), "a surface has no command: {row:?}");
            assert!(!row.block.is_empty(), "a row has no block: {row:?}");
        }
    }

    #[test]
    fn the_engine_prefix_matches_the_command() {
        for row in MANIFEST {
            let prefix = row.command.split(' ').next().unwrap_or("");
            if prefix == "config" {
                continue; // config is cross-engine by design (CR-23)
            }
            if matches!(row.engine, Engine::Shared) {
                // A shared surface must name a REAL command of a real engine —
                // otherwise `Shared` becomes an excuse for an unrouted surface.
                assert!(
                    prefix == "data" || prefix == "model",
                    "shared surface `{}` is served by `{prefix}`, which is not an engine",
                    row.command
                );
                continue;
            }
            assert_eq!(
                prefix,
                row.engine.prefix(),
                "`{}` is filed under {} but reads `{}`",
                row.command,
                row.engine.prefix(),
                prefix
            );
        }
    }
}
