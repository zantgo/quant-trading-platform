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
