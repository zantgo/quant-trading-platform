use crate::AppState;
use market_analyzer::analyzer;
use portfolio_supervisor::session::Currency;
use std::sync::Arc;

pub async fn get_active_pair(
    state: &AppState,
    pair_key: &str,
) -> Option<Arc<analyzer::ActivePair>> {
    state.get_active_pair(pair_key).await
}

/// Extract base symbol from a pair_key (e.g., "BTC-USDT" -> "BTC")
pub fn extract_base_symbol(pair_key: &str) -> String {
    pair_key.split('-').next().unwrap_or(pair_key).to_string()
}

/// Build a pair key from a config symbol entry, honouring the active session
/// quote so the read-side fallbacks on `/api/history`, `/api/monitor`, and
/// `/ws` agree with the frontend on USDC vs USDT. The historical
/// implementation hardcoded `-USDT` regardless of session, which silently
/// produced `BTC-USDT-USDT` (or failed lookups) on USDC sessions.
///
/// `symbol_entry` may be one of:
/// - `"BTC-USDT"` / `"BTC-USDC"` — full pair key as produced by
///   `ExchangeChoice::internal_symbol` (the only shape the daemon actually
///   stores today; see `crates/config-models/src/models.rs` and the
///   `[workspace.instances]` table).
/// - `"Hyperliquid:BTC"` — legacy form that some past configs used.
/// - `"BTC"` — bare base symbol. Returned as `<base>-<quote>`.
///
/// The output is always `<BASE>-<QUOTE>` using the session quote
/// (fallback `USDC`, which matches `registry::add_instance`'s default).
pub async fn default_pair_key(state: &AppState, symbol_entry: &str) -> String {
    let quote = (*state.session.base_currency.read().await).unwrap_or(Currency::USDC);
    let raw = symbol_entry
        .split_once(':')
        .map(|(_, s)| s)
        .unwrap_or(symbol_entry);
    // Strip any existing `-<quote>` suffix that callers like
    // `handlers::history::serve_history` pass in (the value comes from
    // `WorkspaceConfig::declared_symbols`, which already returns full
    // pair keys). Without this we would produce `BTC-USDT-USDC`.
    let base = raw.rsplit_once('-').map(|(b, _)| b).unwrap_or(raw);
    format!("{}-{}", base, quote.as_str())
}
