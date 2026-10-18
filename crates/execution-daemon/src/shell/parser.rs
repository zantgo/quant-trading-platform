//! The command grammar (P6 W6.1).
//!
//! The grammar is a **value**, not a side effect of the REPL: `parse` turns a
//! line into a [`Command`] and nothing else touches stdin. That is what lets
//! `shell_commands.rs` test the surface as data — every command's shape, its
//! flags, and its refusals — without a terminal, an `AppState`, or a runtime.
//!
//! ## Why the parser refuses rather than guesses
//!
//! A shell that silently ignores a typo answers a question the operator did not
//! ask. `model state BTC --tf` with no value is a *refusal*, not "the default",
//! because the default here would be a different duration and therefore a
//! different answer — a flag must never quietly change what a command means.

use std::collections::BTreeMap;

/// One parse failure, with the text that caused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub input: String,
    pub message: String,
}

impl ParseError {
    fn new(input: &str, message: impl Into<String>) -> Self {
        Self {
            input: input.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Everything the shell can be asked, as a closed set.
///
/// A closed enum is the point: `help` is generated from it, the parity manifest
/// is checked against it, and a command that exists but is not declared here is
/// a compile error rather than a surprise at the prompt.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Help,
    Exit,
    Version,

    // ── data (Data Engine) ──────────────────────────────────────────
    Data(DataCommand),

    // ── model (Model Engine) ────────────────────────────────────────
    Model(ModelCommand),

    // ── config (per-engine ownership, CR-23) ───────────────────────
    ConfigShow,
    ConfigSet {
        engine: String,
        path: String,
        value: String,
    },
}

impl Command {
    /// The command's head — `data`, `model`, `config`, `help` …
    ///
    /// The key the parity manifest is indexed by, so a caller can ask what a
    /// command *is* without matching on its payload.
    pub fn head(&self) -> &'static str {
        match self {
            Command::Help => "help",
            Command::Exit => "exit",
            Command::Version => "version",
            Command::Data(_) => "data",
            Command::Model(_) => "model",
            Command::ConfigShow | Command::ConfigSet { .. } => "config",
        }
    }

    /// The exact command string, e.g. `data candles`. Used by the manifest check.
    pub fn full_name(&self) -> String {
        match self {
            Command::Data(d) => format!("data {}", data_sub_name(d)),
            Command::Model(m) => format!("model {}", model_sub_name(m)),
            Command::ConfigShow => "config show".to_string(),
            Command::ConfigSet { .. } => "config set".to_string(),
            other => other.head().to_string(),
        }
    }
}

fn data_sub_name(d: &DataCommand) -> &'static str {
    match d {
        DataCommand::Overview => "overview",
        DataCommand::Exchange => "exchange",
        DataCommand::Connectivity => "connectivity",
        DataCommand::MarketData => "market-data",
        DataCommand::Ntp => "ntp",
        DataCommand::DataQuality => "data-quality",
        DataCommand::Distribution => "distribution",
        DataCommand::ConnectionSettings => "connection-settings",
        DataCommand::Instances => "instances",
        DataCommand::Status => "status",
        DataCommand::Candles { .. } => "candles",
    }
}

fn model_sub_name(m: &ModelCommand) -> &'static str {
    match m {
        ModelCommand::Instances => "instances",
        ModelCommand::Overview => "overview",
        ModelCommand::State { .. } => "state",
        ModelCommand::Alignment { .. } => "alignment",
        ModelCommand::Consensus { .. } => "consensus",
        ModelCommand::Leg { .. } => "leg",
        ModelCommand::Events { .. } => "events",
        ModelCommand::Diagnostics { .. } => "diagnostics",
        ModelCommand::Metrics { .. } => "metrics",
        ModelCommand::Indicator { .. } => "indicator",
        ModelCommand::Signals { .. } => "signals",
        ModelCommand::Divergences { .. } => "divergences",
        ModelCommand::Levels { .. } => "levels",
    }
}

/// `data …`
#[derive(Debug, Clone, PartialEq)]
pub enum DataCommand {
    Overview,
    Exchange,
    Connectivity,
    MarketData,
    Ntp,
    DataQuality,
    Distribution,
    ConnectionSettings,
    Instances,
    Status,
    Candles {
        pair: String,
        tf: TfSelector,
        limit: usize,
    },
}

/// `model …`
#[derive(Debug, Clone, PartialEq)]
pub enum ModelCommand {
    Instances,
    Overview,
    State {
        pair: String,
        tf: TfSelector,
        axis: Option<String>,
        readiness: Option<String>,
        json: bool,
    },
    Alignment {
        pair: String,
    },
    Consensus {
        pair: String,
    },
    Leg {
        pair: String,
        tf: TfSelector,
        level: Option<String>,
    },
    Events {
        pair: String,
        tf: TfSelector,
        kind: Option<String>,
    },
    Diagnostics {
        pair: String,
        tf: TfSelector,
    },
    Metrics {
        pair: String,
        tf: u64,
        group: Option<String>,
    },
    Indicator {
        key: String,
        pair: String,
        tf: u64,
        limit: usize,
    },
    Signals {
        pair: String,
        tf: Option<u64>,
        kind: Option<String>,
    },
    Divergences {
        pair: String,
        tf: Option<u64>,
    },
    Levels {
        pair: String,
        tf: u64,
    },
}

/// Which duration(s) a command applies to.
///
/// `All` is a real selector, not a sentinel: `data candles BTC-USDT --tf all`
/// and `model state BTC-USDT --tf all` answer different questions from
/// `model state --tf 300`, and collapsing them would be the flag-changes-the-
/// meaning failure the grammar exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TfSelector {
    One(u64),
    All,
}

/// A parsed line: the command plus its flags, with no unknown leftovers.
#[derive(Debug, Clone, PartialEq)]
pub struct Invocation {
    pub command: Command,
    /// The flags as written, for the audit line the shell prints in `--json`.
    pub flags: BTreeMap<String, String>,
}

/// Split a line into words, honouring double quotes so a pair key or an engine
/// path containing a space survives.
pub fn tokenize(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in line.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Parse a whole line.
pub fn parse(line: &str) -> Result<Invocation, ParseError> {
    let tokens = tokenize(line);
    if tokens.is_empty() {
        return Err(ParseError::new(line, "empty command"));
    }
    let head = tokens[0].as_str();
    let rest = &tokens[1..];

    let (command, flags) = match head {
        "help" | "?" => (Command::Help, BTreeMap::new()),
        "exit" | "quit" | "\\q" => (Command::Exit, BTreeMap::new()),
        "version" => (Command::Version, BTreeMap::new()),
        "data" => (Command::Data(parse_data(line, rest)?), flags_of(rest)),
        "model" => (Command::Model(parse_model(line, rest)?), flags_of(rest)),
        "config" => parse_config(line, rest)?,
        other => {
            return Err(ParseError::new(
                line,
                format!(
                    "`{other}` is not a command. Try `help`, or one of: {}",
                    TOP_LEVEL.join(", ")
                ),
            ))
        }
    };
    Ok(Invocation { command, flags })
}

/// The prefixes a first word may have, for the error message.
pub const TOP_LEVEL: [&str; 5] = ["help", "version", "data", "model", "config"];

/// Every flag on the line, as `name -> value` (`""` for a valueless flag).
pub fn flags_of(tokens: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if let Some(name) = t.strip_prefix("--") {
            if let Some((k, v)) = name.split_once('=') {
                out.insert(k.to_string(), v.to_string());
                i += 1;
                continue;
            }
            let next = tokens.get(i + 1);
            match next {
                Some(v) if !v.starts_with("--") => {
                    out.insert(name.to_string(), v.clone());
                    i += 2;
                }
                _ => {
                    out.insert(name.to_string(), String::new());
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Positional arguments (non-flag tokens after the sub-command).
fn positionals(tokens: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].starts_with("--") {
            // Skip a flag's value too, unless it was inline (`--k=v`).
            if !tokens[i].contains('=') {
                match tokens.get(i + 1) {
                    Some(v) if !v.starts_with("--") => i += 1,
                    _ => {}
                }
            }
            i += 1;
            continue;
        }
        out.push(tokens[i].as_str());
        i += 1;
    }
    out
}

fn parse_data(line: &str, tokens: &[String]) -> Result<DataCommand, ParseError> {
    let sub = tokens
        .first()
        .map(|s| s.as_str())
        .ok_or_else(|| ParseError::new(line, "`data` needs a sub-command: `data status`"))?;
    let flags = flags_of(&tokens[1..]);
    let cmd = match sub {
        "overview" => DataCommand::Overview,
        "exchange" => DataCommand::Exchange,
        "connectivity" => DataCommand::Connectivity,
        "market-data" | "market_data" => DataCommand::MarketData,
        "ntp" => DataCommand::Ntp,
        "data-quality" | "data_quality" => DataCommand::DataQuality,
        "distribution" => DataCommand::Distribution,
        "connection-settings" | "connection_settings" => DataCommand::ConnectionSettings,
        "instances" => DataCommand::Instances,
        "status" => DataCommand::Status,
        "candles" => {
            let pos = positionals(&tokens[1..]);
            let pair = pos
                .first()
                .ok_or_else(|| {
                    ParseError::new(line, "`data candles` needs a pair: `data candles BTC-USDT`")
                })?
                .to_string();
            let tf = parse_tf(flags.get("tf"), line, TfSelector::All)?;
            let limit = parse_limit(flags.get("limit"), line, 100)?;
            DataCommand::Candles { pair, tf, limit }
        }
        other => {
            return Err(ParseError::new(
                line,
                format!(
                    "`data {other}` is not a data command. Try one of: {}",
                    DATA_SUBCOMMANDS.join(", ")
                ),
            ))
        }
    };
    Ok(cmd)
}

fn parse_model(line: &str, tokens: &[String]) -> Result<ModelCommand, ParseError> {
    let sub = tokens
        .first()
        .map(|s| s.as_str())
        .ok_or_else(|| ParseError::new(line, "`model` needs a sub-command: `model state`"))?;
    let flags = flags_of(&tokens[1..]);
    let pos = positionals(&tokens[1..]);
    let pair = || -> Result<String, ParseError> {
        pos.first().map(|s| s.to_string()).ok_or_else(|| {
            ParseError::new(line, format!("`model {sub}` needs a pair, e.g. BTC-USDT"))
        })
    };
    let cmd = match sub {
        "instances" => ModelCommand::Instances,
        "overview" => ModelCommand::Overview,
        "state" => ModelCommand::State {
            pair: pair()?,
            tf: parse_tf(flags.get("tf"), line, TfSelector::All)?,
            axis: flags.get("axis").cloned(),
            readiness: flags.get("readiness").cloned(),
            json: flags.contains_key("json"),
        },
        "alignment" => ModelCommand::Alignment { pair: pair()? },
        "consensus" => ModelCommand::Consensus { pair: pair()? },
        "leg" => ModelCommand::Leg {
            pair: pair()?,
            tf: parse_tf(flags.get("tf"), line, TfSelector::All)?,
            level: flags.get("level").cloned(),
        },
        "events" => ModelCommand::Events {
            pair: pair()?,
            tf: parse_tf(flags.get("tf"), line, TfSelector::All)?,
            kind: flags.get("kind").cloned(),
        },
        "diagnostics" => ModelCommand::Diagnostics {
            pair: pair()?,
            tf: parse_tf(flags.get("tf"), line, TfSelector::All)?,
        },
        "metrics" => ModelCommand::Metrics {
            pair: pair()?,
            tf: parse_tf_one(flags.get("tf"), line)?,
            group: flags.get("group").cloned(),
        },
        "indicator" => {
            let key = pos
                .first()
                .ok_or_else(|| {
                    ParseError::new(
                        line,
                        "`model indicator` needs a key: `model indicator rsi BTC-USDT`",
                    )
                })?
                .to_string();
            let pair = pos
                .get(1)
                .ok_or_else(|| {
                    ParseError::new(line, "`model indicator <key> <pair>` needs a pair")
                })?
                .to_string();
            ModelCommand::Indicator {
                key,
                pair,
                tf: parse_tf_one(flags.get("tf"), line)?,
                limit: parse_limit(flags.get("limit"), line, 500)?,
            }
        }
        "signals" => ModelCommand::Signals {
            pair: pair()?,
            tf: parse_tf_optional(flags.get("tf"), line)?,
            kind: flags.get("kind").cloned(),
        },
        "divergences" => ModelCommand::Divergences {
            pair: pair()?,
            tf: parse_tf_optional(flags.get("tf"), line)?,
        },
        "levels" => ModelCommand::Levels {
            pair: pair()?,
            tf: parse_tf_one(flags.get("tf"), line)?,
        },
        other => {
            return Err(ParseError::new(
                line,
                format!(
                    "`model {other}` is not a model command. Try one of: {}",
                    MODEL_SUBCOMMANDS.join(", ")
                ),
            ))
        }
    };
    Ok(cmd)
}

fn parse_config(
    line: &str,
    tokens: &[String],
) -> Result<(Command, BTreeMap<String, String>), ParseError> {
    let pos = positionals(tokens);
    match pos.first().copied() {
        Some("show") => Ok((Command::ConfigShow, flags_of(tokens))),
        Some("set") => {
            let engine = pos.get(1).ok_or_else(|| {
                ParseError::new(line, "`config set` needs an engine: `config set model ...`")
            })?;
            let path = pos.get(2).ok_or_else(|| {
                ParseError::new(line, "`config set <engine>` needs a path: `config set model ladder 5`")
            })?;
            let value = pos.get(3).ok_or_else(|| {
                ParseError::new(line, "`config set <engine> <path>` needs a value")
            })?;
            Ok((
                Command::ConfigSet {
                    engine: engine.to_string(),
                    path: path.to_string(),
                    value: value.to_string(),
                },
                flags_of(tokens),
            ))
        }
        other => Err(ParseError::new(
            line,
            format!(
                "`config {}` is not a config command. Try `config show` or `config set <engine> <path> <value>`",
                other.unwrap_or("<nothing>")
            ),
        )),
    }
}

/// `--tf <secs|all>`, with a default. `all` is spelled, never implied.
fn parse_tf(
    raw: Option<&String>,
    line: &str,
    default: TfSelector,
) -> Result<TfSelector, ParseError> {
    match raw {
        None => Ok(default),
        Some(v) if v == "all" => Ok(TfSelector::All),
        Some(v) => v.parse::<u64>().map(TfSelector::One).map_err(|_| {
            ParseError::new(
                line,
                format!("`--tf {v}` is not a duration in seconds or `all`"),
            )
        }),
    }
}

/// `--tf <secs>` for commands that address exactly one duration.
fn parse_tf_one(raw: Option<&String>, line: &str) -> Result<u64, ParseError> {
    match raw {
        None => Err(ParseError::new(
            line,
            "`--tf` is required here: this command addresses ONE duration, and \
             guessing which would answer a different question",
        )),
        Some(v) if v == "all" => Err(ParseError::new(
            line,
            "`--tf all` is not meaningful for a per-duration command; name the duration",
        )),
        Some(v) => v
            .parse::<u64>()
            .map_err(|_| ParseError::new(line, format!("`--tf {v}` is not a duration in seconds"))),
    }
}

fn parse_tf_optional(raw: Option<&String>, line: &str) -> Result<Option<u64>, ParseError> {
    match raw {
        None => Ok(None),
        Some(v) if v == "all" => Ok(None),
        Some(v) => v.parse::<u64>().map(Some).map_err(|_| {
            ParseError::new(
                line,
                format!("`--tf {v}` is not a duration in seconds or `all`"),
            )
        }),
    }
}

/// `--limit`, bounded by the one retention cap (500 — 01-16 T1).
fn parse_limit(raw: Option<&String>, line: &str, default: usize) -> Result<usize, ParseError> {
    let n = match raw {
        None => return Ok(default),
        Some(v) => v
            .parse::<usize>()
            .map_err(|_| ParseError::new(line, format!("`--limit {v}` is not a number")))?,
    };
    if n == 0 || n > 500 {
        return Err(ParseError::new(
            line,
            format!("`--limit {n}` is out of range: 1..=500 (the one retention cap, 01-16 T1)"),
        ));
    }
    Ok(n)
}

pub const DATA_SUBCOMMANDS: [&str; 11] = [
    "overview",
    "exchange",
    "connectivity",
    "market-data",
    "ntp",
    "data-quality",
    "distribution",
    "connection-settings",
    "instances",
    "status",
    "candles",
];

pub const MODEL_SUBCOMMANDS: [&str; 13] = [
    "instances",
    "overview",
    "state",
    "alignment",
    "consensus",
    "leg",
    "events",
    "diagnostics",
    "metrics",
    "indicator",
    "signals",
    "divergences",
    "levels",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_command_parses() {
        assert_eq!(
            parse("help").unwrap().command,
            Command::Help,
            "help is a command, not a side effect"
        );
        assert_eq!(parse("version").unwrap().command, Command::Version);
        assert_eq!(parse("exit").unwrap().command, Command::Exit);
    }

    #[test]
    fn an_unknown_command_is_a_refusal_not_a_guess() {
        let err = parse("model strat").unwrap_err();
        assert!(
            err.message.contains("not a model command"),
            "{}",
            err.message
        );
        let err = parse("wat").unwrap_err();
        assert!(err.message.contains("not a command"), "{}", err.message);
    }

    #[test]
    fn candles_takes_a_pair_a_duration_and_a_bounded_limit() {
        let inv = parse("data candles BTC-USDT --tf 300 --limit 40").unwrap();
        assert_eq!(
            inv.command,
            Command::Data(DataCommand::Candles {
                pair: "BTC-USDT".into(),
                tf: TfSelector::One(300),
                limit: 40
            })
        );
    }

    #[test]
    fn the_limit_is_bounded_by_the_one_retention_cap() {
        assert!(parse("data candles BTC-USDT --limit 501").is_err());
        assert!(parse("data candles BTC-USDT --limit 0").is_err());
        assert!(parse("data candles BTC-USDT --limit 500").is_ok());
    }

    #[test]
    fn tf_all_is_spelled_never_implied() {
        assert_eq!(
            parse("data candles BTC-USDT").unwrap().command,
            Command::Data(DataCommand::Candles {
                pair: "BTC-USDT".into(),
                tf: TfSelector::All,
                limit: 100
            })
        );
    }

    #[test]
    fn a_per_duration_command_refuses_to_guess_its_duration() {
        // The flag-changes-the-meaning failure: silently defaulting here would
        // answer about a different duration than the operator asked for.
        let err = parse("model levels BTC-USDT").unwrap_err();
        assert!(err.message.contains("ONE duration"), "{}", err.message);
        assert!(parse("model levels BTC-USDT --tf all").is_err());
        assert!(parse("model levels BTC-USDT --tf 60").is_ok());
    }

    #[test]
    fn state_carries_every_filter() {
        let inv = parse("model state BTC-USDT --tf 60 --axis direction --readiness solid --json")
            .unwrap();
        match inv.command {
            Command::Model(ModelCommand::State {
                tf,
                axis,
                readiness,
                json,
                ..
            }) => {
                assert_eq!(tf, TfSelector::One(60));
                assert_eq!(axis.as_deref(), Some("direction"));
                assert_eq!(readiness.as_deref(), Some("solid"));
                assert!(json);
            }
            other => panic!("expected a state command, got {other:?}"),
        }
    }

    #[test]
    fn an_inline_flag_value_is_accepted() {
        let inv = parse("model indicator rsi BTC-USDT --tf=60 --limit=5").unwrap();
        assert_eq!(
            inv.command,
            Command::Model(ModelCommand::Indicator {
                key: "rsi".into(),
                pair: "BTC-USDT".into(),
                tf: 60,
                limit: 5
            })
        );
    }

    #[test]
    fn config_set_needs_all_three_parts() {
        assert!(parse("config set").is_err());
        assert!(parse("config set model").is_err());
        assert!(parse("config set model ladder").is_err());
        assert_eq!(
            parse("config set model ladder 5").unwrap().command,
            Command::ConfigSet {
                engine: "model".into(),
                path: "ladder".into(),
                value: "5".into()
            }
        );
    }

    #[test]
    fn a_pair_key_may_be_quoted() {
        let inv = parse("data candles \"BTC-USDT\"").unwrap();
        match inv.command {
            Command::Data(DataCommand::Candles { pair, .. }) => assert_eq!(pair, "BTC-USDT"),
            other => panic!("expected candles, got {other:?}"),
        }
    }

    #[test]
    fn a_bad_duration_is_a_refusal_with_the_value_named() {
        let err = parse("data candles BTC-USDT --tf 5m").unwrap_err();
        assert!(err.message.contains("5m"), "{}", err.message);
    }

    #[test]
    fn the_flag_map_records_what_was_written() {
        let inv = parse("model state BTC-USDT --tf 60 --json").unwrap();
        assert_eq!(inv.flags.get("tf").map(String::as_str), Some("60"));
        assert!(inv.flags.contains_key("json"));
    }
}
