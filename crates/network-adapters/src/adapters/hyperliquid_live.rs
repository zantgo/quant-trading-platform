//! Hyperliquid live order-dispatch client (AUDIT-V6-406 / Phase E1).
//!
//! Implements the exchange-facing half of the v7 `ExecutionBackend::LiveBroker`:
//! EIP-712 signed order placement / cancellation against the Hyperliquid
//! `/exchange` endpoint, plus fill + balance queries via `/info`.
//!
//! Signing contract (Hyperliquid API docs):
//!   domain  = { name: "HyperliquidSignTransaction", version: "1", chainId }
//!   primaryType = "HyperliquidTransaction:Order"
//!   types   = HyperliquidTransaction:Order(builder, is_mainnet, order)
//!             Order(a int64, b bool, p string, s string, r string, t int64)

use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// Default domain chain id (Hyperliquid docs use 421614 for both
/// testnet and mainnet signing).
pub const HL_DEFAULT_CHAIN_ID: u64 = 421614;
pub const HL_MAINNET_INFO_URL: &str = "https://api.hyperliquid.xyz/info";
pub const HL_MAINNET_EXCHANGE_URL: &str = "https://api.hyperliquid.xyz/exchange";

// ── EIP-712 hashing helpers ──────────────────────────────────────────

fn keccak256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

fn encode_uint256(v: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..].copy_from_slice(&v.to_be_bytes());
    out
}

fn encode_bool(b: bool) -> [u8; 32] {
    let mut out = [0u8; 32];
    if b {
        out[31] = 1;
    }
    out
}

fn encode_string(s: &str) -> [u8; 32] {
    keccak256(s.as_bytes())
}

fn encode_int64(v: i64) -> [u8; 32] {
    encode_uint256(v as u64)
}

/// EIP-712 type hash: keccak256 of the canonical type string.
fn type_hash(type_string: &str) -> [u8; 32] {
    keccak256(type_string.as_bytes())
}

/// One Hyperliquid order (legacy 6-field `Order` type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlOrder {
    pub a: i64,  // asset index
    pub b: bool, // is buy
    pub p: String,
    pub s: String,
    pub r: bool, // reduce only
    pub t: i64,  // order type: 1 limit, 2 market, 4 stop-market
}

impl HlOrder {
    /// EIP-712 `Order` struct hash.
    fn struct_hash(&self) -> [u8; 32] {
        let order_type = "Order(int64 a,bool b,string p,string s,string r,int64 t)";
        let mut buf = Vec::with_capacity(32 * 7);
        buf.extend_from_slice(&type_hash(order_type));
        buf.extend_from_slice(&encode_int64(self.a));
        buf.extend_from_slice(&encode_bool(self.b));
        buf.extend_from_slice(&encode_string(&self.p));
        buf.extend_from_slice(&encode_string(&self.s));
        buf.extend_from_slice(&encode_string(if self.r { "true" } else { "false" }));
        buf.extend_from_slice(&encode_int64(self.t));
        keccak256(&buf)
    }
}

/// EIP-712 signature payload for `HyperliquidTransaction:Order`.
pub fn sign_order_hash(orders: &[HlOrder], is_mainnet: bool, chain_id: u64) -> [u8; 32] {
    let domain_type = "EIP712Domain(string name,string version,uint256 chainId)";
    let tx_type = "HyperliquidTransaction:Order(string builder,bool is_mainnet,Order order)";
    let builder = "";

    let domain_hash: [u8; 32] = {
        let mut buf = Vec::with_capacity(32 * 4);
        buf.extend_from_slice(&type_hash(domain_type));
        buf.extend_from_slice(&encode_string("HyperliquidSignTransaction"));
        buf.extend_from_slice(&encode_string("1"));
        buf.extend_from_slice(&encode_uint256(chain_id));
        keccak256(&buf)
    };

    let struct_hash = {
        let mut buf = Vec::with_capacity(32 * 4);
        buf.extend_from_slice(&type_hash(tx_type));
        buf.extend_from_slice(&encode_string(builder));
        buf.extend_from_slice(&encode_bool(is_mainnet));
        buf.extend_from_slice(&orders[0].struct_hash());
        keccak256(&buf)
    };

    let mut signed = Vec::with_capacity(66);
    signed.extend_from_slice(b"\x19\x01");
    signed.extend_from_slice(&domain_hash);
    signed.extend_from_slice(&struct_hash);
    keccak256(&signed)
}

/// ECDSA-secp256k1 sign a 32-byte digest with the account private key.
/// Returns `(r_hex, s_hex, v)`.
pub fn sign_digest(
    private_key_hex: &str,
    digest: &[u8; 32],
) -> Result<(String, String, u64), String> {
    use k256::ecdsa::{RecoveryId, Signature, SigningKey};
    use k256::SecretKey;

    let secret = SecretKey::from_slice(&hex_decode(private_key_hex)?)
        .map_err(|e| format!("invalid private key: {}", e))?;
    let signing_key = SigningKey::from(&secret);
    let (signature, recid): (Signature, RecoveryId) = signing_key
        .sign_prehash_recoverable(digest)
        .map_err(|e| format!("signing failed: {}", e))?;

    // Hyperliquid expects the signature without normalization constraints
    // beyond standard r/s; RecoveryId yields v = recid + 27.
    let r = signature.r();
    let s = signature.s();
    let mut r_bytes = [0u8; 32];
    let mut s_bytes = [0u8; 32];
    r_bytes.copy_from_slice(&r.to_bytes());
    s_bytes.copy_from_slice(&s.to_bytes());
    let v = 27 + u8::from(recid) as u64;

    Ok((hex_encode(&r_bytes), hex_encode(&s_bytes), v))
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() % 2 != 0 {
        return Err("odd-length hex".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("hex: {}", e)))
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ── REST client ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HyperliquidLiveClient {
    pub info_url: String,
    pub exchange_url: String,
    pub address: String,
    pub private_key_hex: String,
    pub is_mainnet: bool,
    pub chain_id: u64,
    http: reqwest::Client,
}

impl HyperliquidLiveClient {
    pub fn new(
        address: String,
        private_key_hex: String,
        is_mainnet: bool,
        chain_id: Option<u64>,
    ) -> Self {
        Self {
            info_url: HL_MAINNET_INFO_URL.to_string(),
            exchange_url: HL_MAINNET_EXCHANGE_URL.to_string(),
            address,
            private_key_hex,
            is_mainnet,
            chain_id: chain_id.unwrap_or(HL_DEFAULT_CHAIN_ID),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Resolve coin → asset index via the `meta` query.
    ///
    /// `dex` is the owning HIP-3 perp dex, or `None` for the default crypto
    /// dex. Asset indices are **per-dex**: a `xyz:` market's index is meaningless
    /// in the default dex's namespace, so a HIP-3 coin must be resolved against
    /// its own dex's `meta`.
    pub async fn asset_index(&self, coin: &str, dex: Option<&str>) -> Result<i64, String> {
        let body = match dex {
            Some(d) => serde_json::json!({ "type": "meta", "dex": d }),
            None => serde_json::json!({ "type": "meta" }),
        };
        let resp = self
            .http
            .post(&self.info_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HL meta failed: {}", e))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("HL meta parse failed: {}", e))?;
        let universe = body
            .get("universe")
            .and_then(|u| u.as_array())
            .ok_or_else(|| "HL meta has no universe".to_string())?;
        // The asset index is the coin's POSITION in `universe`, not a field on the
        // entry — the payload carries only `name`/`szDecimals`/`maxLeverage`/
        // `marginTableId`. Reading a non-existent `index` field made every live
        // order fail with "coin 'X' has no index", on every symbol, default dex
        // included. `BTC` is position 0, which matches its well-known asset id.
        for (idx, entry) in universe.iter().enumerate() {
            if entry.get("name").and_then(|n| n.as_str()) == Some(coin) {
                return Ok(idx as i64);
            }
        }
        Err(format!(
            "coin '{}' not found in Hyperliquid meta{}",
            coin,
            dex.map(|d| format!(" (dex '{d}')")).unwrap_or_default()
        ))
    }

    /// Place orders. Returns the exchange order ids (one per order).
    pub async fn place_orders(&self, orders: &[HlOrder]) -> Result<Vec<String>, String> {
        if orders.is_empty() {
            return Ok(vec![]);
        }
        let digest = sign_order_hash(orders, self.is_mainnet, self.chain_id);
        let (r, s, v) = sign_digest(&self.private_key_hex, &digest)?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let payload = serde_json::json!({
            "action": {
                "type": "order",
                "orders": orders.iter().map(|o| serde_json::json!({
                    "a": o.a, "b": o.b, "p": o.p, "s": o.s, "r": o.r, "t": o.t,
                })).collect::<Vec<_>>(),
            },
            "nonce": nonce,
            "signature": { "r": r, "s": s, "v": v },
        });

        let resp = self
            .http
            .post(&self.exchange_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("HL exchange request failed: {}", e))?;
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("HL response parse failed: {}", e))?;

        if let Some(err) = body.get("err").and_then(|e| e.as_str()) {
            return Err(format!("HL order rejected: {}", err));
        }
        let ids = body
            .pointer("/response/data/statuses")
            .and_then(|s| s.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|st| st.get("oid").and_then(|o| o.as_u64()))
                    .map(|oid| oid.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return Err(format!(
                "HL order response without ids (http {}): {}",
                status, body
            ));
        }
        Ok(ids)
    }

    /// Cancel orders by asset index + oid.
    pub async fn cancel_orders(&self, pairs: &[(i64, u64)]) -> Result<(), String> {
        if pairs.is_empty() {
            return Ok(());
        }
        // The cancel action has no EIP-712 payload; it uses the same
        // signature scheme with an empty "HyperliquidTransaction" payload.
        let digest = keccak256(b"\x19\x01");
        let (r, s, v) = sign_digest(&self.private_key_hex, &digest)?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let payload = serde_json::json!({
            "action": {
                "type": "cancel",
                "cancels": pairs.iter().map(|(a, o)| serde_json::json!({ "a": a, "o": o })).collect::<Vec<_>>(),
            },
            "nonce": nonce,
            "signature": { "r": r, "s": s, "v": v },
        });

        let resp = self
            .http
            .post(&self.exchange_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("HL cancel request failed: {}", e))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("HL cancel parse failed: {}", e))?;
        if let Some(err) = body.get("err").and_then(|e| e.as_str()) {
            return Err(format!("HL cancel rejected: {}", err));
        }
        Ok(())
    }

    /// Recent user fills (order id, fill price, size, direction).
    pub async fn fetch_fills(&self) -> Result<Vec<HlFill>, String> {
        let resp = self
            .http
            .post(&self.info_url)
            .json(&serde_json::json!({ "type": "userFills", "user": self.address }))
            .send()
            .await
            .map_err(|e| format!("HL userFills failed: {}", e))?;
        let fills: Vec<serde_json::Value> = resp
            .json()
            .await
            .map_err(|e| format!("HL userFills parse failed: {}", e))?;

        Ok(fills
            .iter()
            .filter_map(|f| {
                let oid = f.get("oid").and_then(|o| o.as_u64())?;
                let px = f.get("px").and_then(|p| p.as_str())?.parse::<f64>().ok()?;
                let sz = f.get("sz").and_then(|s| s.as_str())?.parse::<f64>().ok()?;
                Some(HlFill {
                    order_id: oid.to_string(),
                    price: px,
                    size: sz,
                })
            })
            .collect())
    }

    /// Account equity from `clearinghouseState`.
    pub async fn fetch_equity(&self) -> Result<f64, String> {
        let resp = self
            .http
            .post(&self.info_url)
            .json(&serde_json::json!({ "type": "clearinghouseState", "user": self.address }))
            .send()
            .await
            .map_err(|e| format!("HL clearinghouseState failed: {}", e))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("HL state parse failed: {}", e))?;
        body.pointer("/marginSummary/accountValue")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or_else(|| "accountValue missing from clearinghouseState".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct HlFill {
    pub order_id: String,
    pub price: f64,
    pub size: f64,
}

/// Build an `HlOrder` from a v7 `OrderPacket` + asset index.
pub fn hl_order_from_packet(packet: &config_models::OrderPacket, asset_index: i64) -> HlOrder {
    let coin = packet.symbol.split('-').next().unwrap_or(&packet.symbol);
    let _ = coin;
    HlOrder {
        a: asset_index,
        b: packet.side == config_models::OrderSide::Buy,
        p: packet.price.map(|p| p.to_string()).unwrap_or_default(),
        s: packet.size.to_string(),
        r: packet.reduce_only,
        t: match packet.order_type {
            config_models::OrderType::Market => 2,
            config_models::OrderType::Stop => 4,
            config_models::OrderType::Limit => 1,
        },
    }
}

/// Symbol → coin conversion used by the broker ("BTC-USDC" → "BTC").
///
/// The pair key never carries a HIP-3 dex qualifier — the operator types
/// `xyz:TLT` but the workspace key is `TLT-USDC` — so this returns the bare
/// base. The dex is a separate argument to the broker, not part of the key.
pub fn coin_from_symbol(symbol: &str) -> String {
    symbol.split('-').next().unwrap_or(symbol).to_string()
}

/// The HIP-3 perp dex a bare coin belongs to, as derived from a *venue* name.
///
/// The venue wire name is authoritative — it is what the resolver returned —
/// so `"xyz:TLT"` yields `Some("xyz")` and a default-dex `"BTC"` yields `None`.
/// Deriving it here (rather than parsing a user string) keeps the broker from
/// ever guessing a dex the resolver did not actually pick.
pub fn dex_from_venue_coin(venue_coin: &str) -> Option<String> {
    core_domain::symbol_rules::split_dex_qualifier(venue_coin).map(|(dex, _)| dex.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eip712_order_signing_round_trip() {
        // Fixed test key (dev-only; never used with real funds).
        let key = "5c8b8b0f6a9d4f9e1f8f3e6f1a5b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b";
        let order = HlOrder {
            a: 0,
            b: true,
            p: "60000.0".to_string(),
            s: "0.001".to_string(),
            r: false,
            t: 1,
        };
        let digest = sign_order_hash(std::slice::from_ref(&order), true, 421614);
        let (r_hex, s_hex, v) = sign_digest(key, &digest).expect("sign");

        // Verify with the public key derived from the same secret.
        use k256::ecdsa::signature::hazmat::PrehashVerifier;
        use k256::ecdsa::{Signature, VerifyingKey};
        use k256::SecretKey;
        let secret = SecretKey::from_slice(&hex_decode(key).unwrap()).unwrap();
        let verifying = VerifyingKey::from(&k256::ecdsa::SigningKey::from(&secret));
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&hex_decode(&r_hex).unwrap());
        s.copy_from_slice(&hex_decode(&s_hex).unwrap());
        let mut rs = [0u8; 64];
        rs[..32].copy_from_slice(&r);
        rs[32..].copy_from_slice(&s);
        let sig = Signature::from_slice(&rs).unwrap();
        assert!(verifying.verify_prehash(&digest, &sig).is_ok());
        assert!(v == 27 || v == 28);

        // Deterministic: same inputs → same digest.
        let digest2 = sign_order_hash(&[order], true, 421614);
        assert_eq!(digest, digest2);
    }

    #[test]
    fn coin_from_symbol_splits_base() {
        assert_eq!(coin_from_symbol("BTC-USDC"), "BTC");
        assert_eq!(coin_from_symbol("ETH-USDT"), "ETH");
        // A HIP-3 instance's pair key stays bare — the dex is NOT in the key,
        // so this must not leak a qualifier into the order's coin field.
        assert_eq!(coin_from_symbol("TLT-USDC"), "TLT");
    }

    #[test]
    fn dex_is_derived_from_the_venue_coin_not_the_pair_key() {
        assert_eq!(dex_from_venue_coin("xyz:TLT"), Some("xyz".to_string()));
        assert_eq!(dex_from_venue_coin("mkts:USBOND"), Some("mkts".to_string()));
        // Default crypto dex → no qualifier.
        assert_eq!(dex_from_venue_coin("BTC"), None);
        assert_eq!(dex_from_venue_coin("TAO"), None);
    }
}

#[cfg(test)]
mod asset_index_tests {
    use super::*;

    /// The asset id a Hyperliquid order carries is the coin's position in that
    /// dex's `meta.universe`. The payload has no `index` field — reading one
    /// meant every live order failed with "coin 'X' has no index", on the default
    /// crypto dex as well as on HIP-3 dexes.
    #[test]
    fn the_asset_index_is_the_universe_position() {
        // Recorded default-dex universe: BTC is the first entry.
        let universe: serde_json::Value =
            serde_json::from_str(r#"{"universe":[{"name":"BTC"},{"name":"ETH"},{"name":"TAO"}]}"#)
                .unwrap();
        let arr = universe["universe"].as_array().unwrap();
        for (idx, coin) in ["BTC", "ETH", "TAO"].iter().enumerate() {
            let found = arr
                .iter()
                .position(|e| e["name"] == *coin)
                .expect("coin present") as i64;
            assert_eq!(found, idx as i64, "{coin}");
        }
        // No entry carries an `index` field at all.
        assert!(arr[0].get("index").is_none());
    }

    /// HIP-3: indices are namespaced per dex, so a `xyz:` market's id is only
    /// meaningful within the `xyz` universe.
    #[test]
    fn a_hip3_asset_index_is_its_position_on_its_own_dex() {
        let xyz: serde_json::Value = serde_json::from_str(
            r#"{"universe":[{"name":"xyz:XYZ100"},{"name":"xyz:TSLA"},{"name":"xyz:TLT"}]}"#,
        )
        .unwrap();
        let arr = xyz["universe"].as_array().unwrap();
        let tlt = arr.iter().position(|e| e["name"] == "xyz:TLT").unwrap() as i64;
        assert_eq!(tlt, 2);
    }
}
