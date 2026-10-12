//! Bybit V5 live order-dispatch client.
//!
//! Implements the exchange-facing half of the Bybit `ExecutionBackend`:
//! HMAC-SHA256 signed REST calls against the V5 linear-perpetual endpoints.
//! Fills are REST-polled, matching the Bitget pattern.
//!
//! ## The signing preimage is NOT Bitget's
//!
//! This is the single most dangerous thing to port by copy-paste, because a
//! wrong preimage produces requests that **compile, send, and are rejected at
//! runtime** rather than failing to build:
//!
//! ```text
//! Bitget:  hex(HMAC-SHA256(timestamp + method + requestPath + body, secret))
//!          headers: ACCESS-KEY / ACCESS-SIGN / ACCESS-TIMESTAMP / ACCESS-PASSPHRASE
//!
//! Bybit:   hex(HMAC-SHA256(timestamp + apiKey + recvWindow + queryString_or_body, secret))
//!          headers: X-BAPI-API-KEY / X-BAPI-SIGN / X-BAPI-TIMESTAMP / X-BAPI-RECV-WINDOW
//! ```
//!
//! Note that Bybit's preimage embeds the **API key** and the **recvWindow** —
//! neither appears in Bitget's — and carries no method or path. GET requests
//! sign the query string; POST requests sign the JSON body.
//!
//! `X-BAPI-SIGN` must be the **uppercase** hex digest (Bybit is stricter than
//! Bitget here).
//!
//! ## Rate limits
//!
//! `POST /v5/order/create` and `/v5/order/cancel` are capped at 10/s for
//! linear, so the client throttles at 10 req/s exactly like the Bitget client
//! (whose venue default is ~20/s). The platform-wide limit is 600 HTTP
//! requests per 5 s per IP.

use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const BYBIT_BASE_URL: &str = "https://api.bybit.com";

type HmacSha256 = Hmac<Sha256>;

/// Default recvWindow (ms) attached to every signed request.
pub const BYBIT_RECV_WINDOW_MS: u64 = 5000;

/// Bybit V5 signature: uppercase-hex HMAC-SHA256 over
/// `timestamp + api_key + recv_window + query_or_body`.
///
/// **Not** Bitget's `timestamp + method + path + body`.
pub fn bybit_sign(
    timestamp_ms: u64,
    api_key: &str,
    recv_window_ms: u64,
    payload: &str,
    api_secret: &str,
) -> String {
    let prehash = format!("{}{}{}{}", timestamp_ms, api_key, recv_window_ms, payload);
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).expect("hmac key");
    mac.update(prehash.as_bytes());
    let digest = mac.finalize().into_bytes();
    digest
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>()
}

/// Strip the pair-key separator: `BTC-USDT` → `BTCUSDT`. The USDC family
/// (`BTCPERP`) is out of scope — Bybit is wired for USDT settlement only.
pub fn bybit_symbol_from_internal(symbol: &str) -> String {
    symbol.replace('-', "").to_uppercase()
}

/// True when the body reports Bybit's success sentinel. Bybit always returns
/// HTTP 200 and signals failure in the envelope (`retCode != 0`), so status
/// alone is never sufficient.
pub fn check_result(body: &serde_json::Value) -> Result<(), String> {
    let code = body.get("retCode").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code == 0 {
        return Ok(());
    }
    let msg = body
        .get("retMsg")
        .and_then(|m| m.as_str())
        .unwrap_or("<no retMsg>");
    Err(format!("Bybit API error: retCode={} retMsg={}", code, msg))
}

pub struct BybitLiveClient {
    pub api_key: String,
    pub api_secret: String,
    http: reqwest::Client,
    last_request_ms: tokio::sync::Mutex<u64>,
}

impl BybitLiveClient {
    pub fn new(api_key: String, api_secret: String) -> Self {
        Self {
            api_key,
            api_secret,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            last_request_ms: tokio::sync::Mutex::new(0),
        }
    }

    /// Venue cap for `/v5/order/create` and `/v5/order/cancel` on linear is
    /// 10/s, so the throttle floor is 100 ms.
    async fn throttle(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let mut last = self.last_request_ms.lock().await;
        let elapsed = now.saturating_sub(*last);
        if elapsed < 100 {
            drop(last);
            tokio::time::sleep(std::time::Duration::from_millis(100 - elapsed)).await;
            last = self.last_request_ms.lock().await;
        }
        *last = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Signed GET. `path` MUST already carry its query string — that query
    /// string is what gets signed (Bybit, not the path alone).
    async fn signed_get(&self, path: &str) -> Result<serde_json::Value, String> {
        self.throttle().await;
        let ts = Self::now_ms();
        let recv = BYBIT_RECV_WINDOW_MS.to_string();
        // A GET signs the query string, which is everything after the '?'.
        let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
        let sign = bybit_sign(
            ts,
            &self.api_key,
            BYBIT_RECV_WINDOW_MS,
            query,
            &self.api_secret,
        );
        let resp = self
            .http
            .get(format!("{}{}", BYBIT_BASE_URL, path))
            .header("X-BAPI-API-KEY", &self.api_key)
            .header("X-BAPI-SIGN", sign)
            .header("X-BAPI-TIMESTAMP", ts.to_string())
            .header("X-BAPI-RECV-WINDOW", recv)
            .send()
            .await
            .map_err(|e| format!("Bybit GET failed: {}", e))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Bybit GET parse failed: {}", e))?;
        check_result(&body)?;
        Ok(body)
    }

    /// Signed POST. The JSON body is what gets signed.
    async fn signed_post(
        &self,
        path: &str,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.throttle().await;
        let ts = Self::now_ms();
        let body_str = payload.to_string();
        let sign = bybit_sign(
            ts,
            &self.api_key,
            BYBIT_RECV_WINDOW_MS,
            &body_str,
            &self.api_secret,
        );
        let resp = self
            .http
            .post(format!("{}{}", BYBIT_BASE_URL, path))
            .header("X-BAPI-API-KEY", &self.api_key)
            .header("X-BAPI-SIGN", sign)
            .header("X-BAPI-TIMESTAMP", ts.to_string())
            .header("X-BAPI-RECV-WINDOW", BYBIT_RECV_WINDOW_MS.to_string())
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .map_err(|e| format!("Bybit POST failed: {}", e))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Bybit POST parse failed: {}", e))?;
        check_result(&body)?;
        Ok(body)
    }

    /// Place a limit/market order. Returns the exchange order id.
    #[allow(clippy::too_many_arguments)]
    pub async fn place_order(
        &self,
        symbol: &str,
        side: bool,       // true = buy
        order_type: &str, // "Limit" | "Market"
        price: &str,
        size: &str,
        reduce_only: bool,
        order_link_id: &str,
    ) -> Result<String, String> {
        let mut payload = serde_json::json!({
            "category": "linear",
            "symbol": symbol,
            "side": if side { "Buy" } else { "Sell" },
            "orderType": order_type,
            "qty": size,
            "reduceOnly": reduce_only,
            "orderLinkId": order_link_id,
            "timeInForce": "IOC",
        });
        if order_type == "Limit" {
            payload["price"] = serde_json::Value::String(price.to_string());
            payload["timeInForce"] = serde_json::Value::String("GTC".to_string());
        }
        let body = self.signed_post("/v5/order/create", &payload).await?;
        body.get("result")
            .and_then(|r| r.get("orderId"))
            .and_then(|o| o.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("Bybit place-order missing orderId: {}", body))
    }

    /// Set the position's take-profit / stop-loss bracket. Bybit models TP/SL
    /// as position attributes rather than a separate trigger order, so this
    /// maps to `/v5/position/trading-stop` instead of Bitget's
    /// `place-tpsl-order`.
    #[allow(clippy::too_many_arguments)]
    pub async fn set_trading_stop(
        &self,
        symbol: &str,
        take_profit: Option<&str>,
        stop_loss: Option<&str>,
        tp_size: Option<&str>,
        sl_size: Option<&str>,
    ) -> Result<(), String> {
        let mut payload = serde_json::json!({
            "category": "linear",
            "symbol": symbol,
            "positionIdx": 0,
            "slTriggerBy": "MarkPrice",
            "tpTriggerBy": "MarkPrice",
        });
        if let Some(tp) = take_profit {
            payload["takeProfit"] = serde_json::Value::String(tp.to_string());
            if let Some(sz) = tp_size {
                payload["takeProfitSize"] = serde_json::Value::String(sz.to_string());
            }
        }
        if let Some(sl) = stop_loss {
            payload["stopLoss"] = serde_json::Value::String(sl.to_string());
            if let Some(sz) = sl_size {
                payload["stopLossSize"] = serde_json::Value::String(sz.to_string());
            }
        }
        self.signed_post("/v5/position/trading-stop", &payload)
            .await?;
        Ok(())
    }

    /// Cancel an order by symbol + order id.
    pub async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<(), String> {
        let payload = serde_json::json!({
            "category": "linear",
            "symbol": symbol,
            "orderId": order_id,
        });
        self.signed_post("/v5/order/cancel", &payload).await?;
        Ok(())
    }

    /// Recent account fills: (orderId, exec price, exec qty).
    pub async fn fetch_fills(&self) -> Result<Vec<(String, f64, f64)>, String> {
        let path = format!(
            "/v5/execution/list?category=linear&limit=50&startTime=0&endTime={}",
            Self::now_ms()
        );
        let body = self.signed_get(&path).await?;
        let rows = body
            .get("result")
            .and_then(|r| r.get("list"))
            .and_then(|l| l.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(rows
            .iter()
            .filter_map(|f| {
                let oid = f.get("orderId").and_then(|o| o.as_str())?.to_string();
                let px = f.get("execPrice")?.as_str()?.parse::<f64>().ok()?;
                let sz = f.get("execQty")?.as_str()?.parse::<f64>().ok()?;
                Some((oid, px, sz))
            })
            .collect())
    }

    /// Account equity in USDT (the settlement currency for the USDT family).
    pub async fn fetch_equity(&self) -> Result<f64, String> {
        let path = "/v5/account/wallet-balance?accountType=UNIFIED&coin=USDT";
        let body = self.signed_get(path).await?;
        body.get("result")
            .and_then(|r| r.get("list"))
            .and_then(|l| l.as_array())
            .and_then(|arr| arr.first())
            .and_then(|a| a.get("totalEquity"))
            .and_then(|e| e.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or_else(|| format!("Bybit wallet-balance missing totalEquity: {}", body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_preimage_differs_from_bitgets() {
        // Bybit's preimage is timestamp + apiKey + recvWindow + payload, with
        // NO method and NO path. This test pins that shape so a future
        // "simplification" to Bitget's form fails loudly instead of producing
        // runtime 10003/10004 auth errors.
        let sign = bybit_sign(1_700_000_000_000, "MYKEY", 5000, "{\"a\":1}", "MYSECRET");
        // Deterministic + uppercase hex.
        assert_eq!(
            sign,
            bybit_sign(1_700_000_000_000, "MYKEY", 5000, "{\"a\":1}", "MYSECRET")
        );
        assert_eq!(sign.len(), 64);
        assert!(sign
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_lowercase()));

        // Every preimage component must change the signature — if, say, the
        // api key were omitted the signature would be identical for two
        // different keys, which is the actual failure mode.
        assert_ne!(
            sign,
            bybit_sign(1_700_000_000_001, "MYKEY", 5000, "{\"a\":1}", "MYSECRET")
        );
        assert_ne!(
            sign,
            bybit_sign(1_700_000_000_000, "OTHERKEY", 5000, "{\"a\":1}", "MYSECRET")
        );
        assert_ne!(
            sign,
            bybit_sign(1_700_000_000_000, "MYKEY", 9000, "{\"a\":1}", "MYSECRET")
        );
        assert_ne!(
            sign,
            bybit_sign(1_700_000_000_000, "MYKEY", 5000, "{\"a\":2}", "MYSECRET")
        );
        assert_ne!(
            sign,
            bybit_sign(1_700_000_000_000, "MYKEY", 5000, "{\"a\":1}", "OTHERSECRET")
        );
    }

    #[test]
    fn pair_key_to_venue_symbol() {
        assert_eq!(bybit_symbol_from_internal("BTC-USDT"), "BTCUSDT");
        assert_eq!(bybit_symbol_from_internal("AAPL-USDT"), "AAPLUSDT");
        assert_eq!(bybit_symbol_from_internal("XAU-USDT"), "XAUUSDT");
        assert_eq!(bybit_symbol_from_internal("EURUSD-USDT"), "EURUSDUSDT");
    }

    #[test]
    fn envelope_error_is_reported_even_on_http_200() {
        // Bybit always returns HTTP 200 and signals failure in the envelope,
        // so a status-code check alone would treat every rejection as success.
        let ok = serde_json::json!({"retCode": 0, "retMsg": "OK", "result": {}});
        assert!(check_result(&ok).is_ok());
        let bad =
            serde_json::json!({"retCode": 10003, "retMsg": "API key is invalid.", "result": {}});
        let err = check_result(&bad).expect_err("must reject");
        assert!(err.contains("10003"));
        assert!(err.contains("API key is invalid"));
    }
}
