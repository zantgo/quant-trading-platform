// HOT PATH — real-time exchange WebSocket adapters.
// These modules process live market data on the critical path.
// Operations must be non-blocking and bounded-channel-aware.
pub mod bitget;
pub mod bitget_derivatives;
pub mod bitget_historical_fetch;
pub mod bitget_live;
pub mod bitget_rest;
pub mod historical_fetch;
pub mod hl_derivatives_poller;
pub mod http;
pub mod hyperliquid;
pub mod hyperliquid_historical_fetch;
pub mod hyperliquid_live;
pub mod hyperliquid_rest;
pub mod reconstruction;
pub mod resilience;
