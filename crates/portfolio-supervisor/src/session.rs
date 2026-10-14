use std::sync::atomic::AtomicBool;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Currency {
    USDT,
    USDC,
}

impl Currency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Currency::USDT => "USDT",
            Currency::USDC => "USDC",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExchangeChoice {
    Hyperliquid,
    Bitget,
    /// Bybit V5 `category=linear`. Both settlement families are served by the
    /// one product (USDT perps `BTCUSDT`, USDC perps `BTCPERP`); the wizard
    /// and `supports_currency` gate to USDT because the USDC family uses a
    /// `PERP` suffix rather than a quote-currency suffix, so the pair-key
    /// derivation differs and is out of scope for this port.
    Bybit,
}

impl ExchangeChoice {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExchangeChoice::Hyperliquid => "Hyperliquid",
            ExchangeChoice::Bitget => "Bitget",
            ExchangeChoice::Bybit => "Bybit",
        }
    }

    /// The venue-native symbol derived purely from the pair components.
    ///
    /// For Hyperliquid this is only correct on the DEFAULT crypto perp dex.
    /// Since HIP-3 the venue also runs builder-deployed dexes whose markets are
    /// named `<dex>:<coin>` (`xyz:TLT`, `mkts:USBOND`), and those names cannot
    /// be derived from the base — only the venue's `resolve_venue_coin` knows
    /// which dex owns a given base. Callers that talk to Hyperliquid must pass
    /// the resolved `venue_coin` through explicitly instead of calling this;
    /// it is retained for the pair-key-side derivations that genuinely only
    /// need the components.
    pub fn raw_symbol(&self, base: &str, quote: &Currency) -> String {
        match self {
            ExchangeChoice::Hyperliquid => base.to_string(),
            ExchangeChoice::Bitget => match quote {
                Currency::USDT => format!("{}USDT", base),
                Currency::USDC => format!("{}USD", base),
            },
            // USDT linear perps are `{BASE}USDT`, byte-identical to Bitget's
            // USDT-FUTURES convention (`AAPLUSDT`, `XAUUSDT`, `EURUSDUSDT`).
            // USDC perps use a `{BASE}PERP` suffix but USDC is unsupported
            // here, so that arm is unreachable.
            ExchangeChoice::Bybit => format!("{}USDT", base),
        }
    }

    /// The HIP-3 perp dex a Hyperliquid market lives on, derived from a
    /// *venue* wire name. `None` means the default crypto dex.
    pub fn hyperliquid_dex(&self, venue_coin: &str) -> Option<String> {
        match self {
            ExchangeChoice::Hyperliquid => {
                core_domain::symbol_rules::split_dex_qualifier(venue_coin)
                    .map(|(dex, _)| dex.to_string())
            }
            ExchangeChoice::Bitget | ExchangeChoice::Bybit => None,
        }
    }

    pub fn internal_symbol(&self, base: &str, quote: &Currency) -> String {
        format!("{}-{}", base, quote.as_str())
    }

    /// The venue's product-type parameter, when the venue has one. Bitget
    /// partitions its mix API by `USDT-FUTURES` / `USDC-FUTURES`; Hyperliquid
    /// and Bybit have no equivalent (Bybit uses one `category=linear`).
    pub fn product_type(&self, quote: &Currency) -> Option<&'static str> {
        match self {
            ExchangeChoice::Bitget => Some(match quote {
                Currency::USDT => "USDT-FUTURES",
                Currency::USDC => "USDC-FUTURES",
            }),
            ExchangeChoice::Hyperliquid | ExchangeChoice::Bybit => None,
        }
    }

    pub fn supports_currency(&self, quote: &Currency) -> bool {
        match self {
            ExchangeChoice::Hyperliquid => *quote == Currency::USDC,
            ExchangeChoice::Bitget => matches!(quote, Currency::USDT | Currency::USDC),
            ExchangeChoice::Bybit => *quote == Currency::USDT,
        }
    }
}

/// AUDIT-C1: the inverse of [`ExchangeChoice::as_str`], case-insensitive.
/// Needed because the exchange is persisted as free text in two places — the
/// `sessions` table and `[workspace].default_exchange` in `config.toml` — and
/// neither is a typed field.
pub fn exchange_from_str(raw: &str) -> Option<ExchangeChoice> {
    if raw.eq_ignore_ascii_case("hyperliquid") {
        Some(ExchangeChoice::Hyperliquid)
    } else if raw.eq_ignore_ascii_case("bitget") {
        Some(ExchangeChoice::Bitget)
    } else if raw.eq_ignore_ascii_case("bybit") {
        Some(ExchangeChoice::Bybit)
    } else {
        None
    }
}

/// AUDIT-C1: the inverse of [`Currency::as_str`], case-insensitive.
pub fn currency_from_str(raw: &str) -> Option<Currency> {
    if raw.eq_ignore_ascii_case("USDT") {
        Some(Currency::USDT)
    } else if raw.eq_ignore_ascii_case("USDC") {
        Some(Currency::USDC)
    } else {
        None
    }
}

/// The environment a restart must bring back up, and one diagnostic per
/// persisted instance that could not be safely derived.
#[derive(Debug, Clone, PartialEq)]
pub struct BootEnvironment {
    pub exchange: ExchangeChoice,
    pub currency: Currency,
    /// Instances whose persisted `symbol` disagrees with what the resolved
    /// environment would derive. These MUST be skipped rather than spawned,
    /// or the registry appends a duplicate entry to `config.toml` under a
    /// different key (AUDIT-C1).
    pub mismatches: Vec<BootSpawnMismatch>,
    /// Which source won, for the boot log line.
    pub source: BootEnvSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootEnvSource {
    /// The interrupted session's `sessions` row — the environment the
    /// operator actually ran (v11.12.23's `update_session_environment`).
    InterruptedSession,
    /// The first Running instance's own persisted quote suffix.
    PersistedInstance,
    /// `[workspace].default_exchange` / `.default_currency`.
    WorkspaceDefaults,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BootSpawnMismatch {
    /// The symbol as persisted in `config.toml`.
    pub declared: String,
    /// The key the resolved environment would actually produce.
    pub derived: String,
}

/// AUDIT-C1: resolve the environment a restart must use.
///
/// The defect this replaces: `POST /api/session/init` (the wizard) persists the
/// operator's real exchange/currency to the `sessions` DB row only — it never
/// writes `config.toml`. At boot the daemon therefore called
/// `init_session(cli.resolve_exchange(&workspace.default_exchange), …)`, i.e.
/// the *stale template defaults* (Hyperliquid / USDC), and `add_instance` then
/// FORCED that quote onto every persisted instance (`registry/mod.rs`), so
/// `BTC-USDT` was re-created as `BTC-USDC` on Hyperliquid and **appended** to
/// `config.toml` as a duplicate. The next restart then hit "already exists",
/// which the boot loop could not distinguish from a transient network error,
/// so it retried 20 × 30 s per entry before every instance after it.
///
/// Precedence:
///   1. The interrupted session's row — what the operator actually ran.
///   2. The first Running instance's own `symbol` quote suffix.
///   3. `[workspace].default_exchange` / `.default_currency`.
///
/// Returns the exchange/currency plus a mismatch per instance that cannot be
/// derived safely, so the caller reports loudly instead of spawning a duplicate.
pub fn resolve_boot_environment(
    default_exchange: &str,
    default_currency: &str,
    instances: &[config_models::InstanceEntry],
    interrupted: Option<(&str, &str)>,
) -> BootEnvironment {
    let running: Vec<&config_models::InstanceEntry> = instances
        .iter()
        .filter(|i| i.status == config_models::InstanceStatus::Running)
        .collect();

    let mut mismatches = Vec::new();

    // Try each source in precedence order; the first parseable one wins.
    let mut all: Vec<(BootEnvSource, ExchangeChoice, Currency)> = Vec::new();

    if let Some((ex, cur)) = interrupted {
        if let (Some(e), Some(c)) = (exchange_from_str(ex), currency_from_str(cur)) {
            all.push((BootEnvSource::InterruptedSession, e, c));
        }
    }
    if let Some(first) = running.first() {
        if let Some(quote) = first.symbol.rsplit_once('-').map(|(_, q)| q) {
            if let Some(c) = currency_from_str(quote) {
                if let Some(e) = interrupted
                    .and_then(|(ex, _)| exchange_from_str(ex))
                    .or_else(|| exchange_from_str(default_exchange))
                {
                    all.push((BootEnvSource::PersistedInstance, e, c));
                }
            }
        }
    }
    if let (Some(e), Some(c)) = (
        exchange_from_str(default_exchange),
        currency_from_str(default_currency),
    ) {
        all.push((BootEnvSource::WorkspaceDefaults, e, c));
    }

    // Precedence is STRICT: take the first PARSEABLE candidate.
    //
    // Deliberately NOT filtered by `supports_currency`. That would reorder
    // precedence — a stale-but-valid lower source could displace the
    // environment that actually ran, which is precisely the bug this fixes. A
    // settlement combination the venue cannot host cannot arise from the
    // wizard (`init_session` gates it), and if one ever reaches the boot loop
    // anyway, `add_instance` fails fast with a PERMANENT error
    // (`spawn_error_is_permanent`) instead of the loop silently substituting a
    // different venue behind the operator's back.
    let chosen = all.first().copied();

    let Some((source, exchange, currency)) = chosen else {
        // Nothing parseable at all — keep the documented defaults so boot still
        // initialises a session rather than panicking.
        return BootEnvironment {
            exchange: ExchangeChoice::Hyperliquid,
            currency: Currency::USDC,
            mismatches: Vec::new(),
            source: BootEnvSource::WorkspaceDefaults,
        };
    };

    for entry in &running {
        let Some((base, _declared_quote)) = entry.symbol.split_once('-') else {
            mismatches.push(BootSpawnMismatch {
                declared: entry.symbol.clone(),
                derived: entry.symbol.clone(),
            });
            continue;
        };
        // The registry strips a HIP-3 `dex:BASE` qualifier before deriving the
        // key, so the comparison must do the same.
        let base = core_domain::symbol_rules::split_dex_qualifier(base)
            .map(|(_, b)| b.to_string())
            .unwrap_or_else(|| base.to_string());
        let derived = exchange.internal_symbol(&base, &currency);
        if derived != entry.symbol {
            mismatches.push(BootSpawnMismatch {
                declared: entry.symbol.clone(),
                derived,
            });
        }
    }

    BootEnvironment {
        exchange,
        currency,
        mismatches,
        source,
    }
}

/// AUDIT-C1: classify a boot-spawn failure as PERMANENT (never heals on retry)
/// or TRANSIENT (network blip, venue timeout — worth another attempt).
///
/// `registry::add_instance` returns `Result<_, String>` and both classes arrive
/// as free text, so this matches on the stable message prefixes it produces.
/// The boot loop previously retried everything 20 × 30 s ≈ 9.5 minutes per
/// entry, SERIALLY, blocking every instance after it — and because
/// `GET /api/instances` reads only the live map, a permanently-failing entry
/// was invisible in the dashboard the entire time.
///
/// Anything not recognised as permanent is treated as transient: a new error
/// message must not be able to make a recoverable boot failure give up early.
///
/// Substring (not prefix) matching is required because two of these messages
/// are venue-prefixed by construction: `add_instance` renders the settlement
/// error as `"{} does not support {} settlement."` and the symbol probe as
/// `"'{}' isn't available on {} ..."`. Each phrase below is semantically
/// definitive, so a substring match cannot collide with a transient transport
/// failure.
pub fn spawn_error_is_permanent(msg: &str) -> bool {
    const PERMANENT_SUBSTRINGS: &[&str] = &[
        "already exists",
        "no active session",
        "does not support",
        "no saved config for pair",
        "symbol too long",
        "isn't available on",
        "is not a valid symbol",
        "invalid symbol",
    ];
    let lower = msg.to_ascii_lowercase();
    PERMANENT_SUBSTRINGS.iter().any(|p| lower.contains(p))
}

pub struct SessionState {
    pub active: AtomicBool,
    /// v11.12: OPERATOR-INTENT activation — the flag `/api/session/status`
    /// reports as `active`. Only an explicit operator action flips it
    /// (`POST /session/init`, `POST /session/recover` set it; quit/discard
    /// clear it). The BOOT auto-init intentionally does NOT: it exists only
    /// to let the background instance respawn pass the session-active gate,
    /// and it previously raced the browser's first status poll into
    /// skipping the mandatory Welcome screen after a Ctrl+C restart.
    pub ui_active: AtomicBool,
    pub base_currency: RwLock<Option<Currency>>,
    pub exchange: RwLock<Option<ExchangeChoice>>,
    /// v7.1 follow-up: the operator's chosen execution mode at session
    /// start ("paper" | "live") — becomes the default for created instances.
    pub mode: RwLock<Option<String>>,
    /// v7.1 follow-up: the operator's paper-session capital (USD) — the
    /// default `portfolio_capital_usd` for instances created in this session.
    pub portfolio_capital_usd: RwLock<Option<f64>>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            ui_active: AtomicBool::new(false),
            base_currency: RwLock::new(None),
            exchange: RwLock::new(None),
            mode: RwLock::new(None),
            portfolio_capital_usd: RwLock::new(None),
        }
    }

    /// The UI-facing activation (see `ui_active`).
    pub fn ui_active(&self) -> bool {
        self.ui_active.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_ui_active(&self, on: bool) {
        self.ui_active
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub async fn session_mode(&self) -> Option<String> {
        self.mode.read().await.clone()
    }

    pub async fn session_capital(&self) -> Option<f64> {
        *self.portfolio_capital_usd.read().await
    }

    pub async fn set_session_defaults(&self, mode: Option<String>, capital: Option<f64>) {
        *self.mode.write().await = mode;
        *self.portfolio_capital_usd.write().await = capital;
    }
}
