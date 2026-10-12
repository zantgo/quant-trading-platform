use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct KeyQuery {
    pub exchange: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddKeyRequest {
    pub exchange: String,
    pub account_name: String,
    pub api_key: String,
    pub api_secret: String,
    #[serde(default)]
    pub passphrase: String,
    #[serde(default)]
    pub referred_uid: String,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct KeyMetadata {
    pub id: i64,
    pub exchange: String,
    pub account_name: String,
    pub is_active: bool,
    pub referred_uid: String,
    pub last_sync_timestamp: Option<i64>,
}

pub async fn list_keys(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KeyQuery>,
) -> impl IntoResponse {
    let exchange_filter = query.exchange.unwrap_or_default();

    let rows = if exchange_filter.is_empty() {
        sqlx::query_as::<_, (i64, String, String, String, i32, String, Option<i64>)>(
            "SELECT id, exchange, account_name, api_key, is_active, referred_uid, last_sync_timestamp FROM exchange_keys ORDER BY id DESC",
        )
        .fetch_all(&state.pool)
        .await
    } else {
        sqlx::query_as::<_, (i64, String, String, String, i32, String, Option<i64>)>(
            "SELECT id, exchange, account_name, api_key, is_active, referred_uid, last_sync_timestamp FROM exchange_keys WHERE exchange = ? ORDER BY id DESC",
        )
        .bind(&exchange_filter)
        .fetch_all(&state.pool)
        .await
    };

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to query keys: {}", e)
                })),
            )
                .into_response();
        }
    };

    let keys: Vec<KeyMetadata> = rows
        .into_iter()
        .map(
            |(id, exchange, account_name, _api_key, is_active, referred_uid, last_sync)| {
                KeyMetadata {
                    id,
                    exchange,
                    account_name,
                    is_active: is_active != 0,
                    referred_uid,
                    last_sync_timestamp: last_sync,
                }
            },
        )
        .collect();

    Json(serde_json::json!({
        "keys": keys,
        "count": keys.len(),
    }))
    .into_response()
}

pub async fn add_key(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddKeyRequest>,
) -> impl IntoResponse {
    if req.exchange.is_empty()
        || req.account_name.is_empty()
        || req.api_key.is_empty()
        || req.api_secret.is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "exchange, account_name, api_key, and api_secret are required"
            })),
        )
            .into_response();
    }

    let valid_exchanges = ["Hyperliquid", "Bitget", "Bybit"];
    if !valid_exchanges.contains(&req.exchange.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Invalid exchange. Must be Hyperliquid, Bitget or Bybit"
            })),
        )
            .into_response();
    }

    let is_active_int = if req.is_active { 1 } else { 0 };

    // AUDIT-H10: the documented contract (§2.10 / 06-02 §3.5) requires
    // AES-256-GCM at rest. The old code INSERTed the raw secret —
    // contradicting `verify_encryption_or_panic` at daemon boot, which
    // asserts every existing row is encrypted. Refuse to store plaintext
    // when no master key is provisioned.
    let master_key_ok = database_storage::crypto::master_key_available();
    let encrypted_secret = if master_key_ok {
        database_storage::crypto::encrypt_field(&req.api_secret)
    } else {
        Err("EXCHANGE_SECRET_KEY is not set".to_string())
    };
    let encrypted_passphrase = if req.passphrase.is_empty() {
        Ok(String::new())
    } else if master_key_ok {
        database_storage::crypto::encrypt_field(&req.passphrase)
    } else {
        Err("EXCHANGE_SECRET_KEY is not set".to_string())
    };
    let (secret, passphrase) = match (encrypted_secret, encrypted_passphrase) {
        (Ok(s), Ok(p)) => (s, p),
        (Err(e), _) | (_, Err(e)) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": format!(
                        "Cannot store exchange credentials: {} — set EXCHANGE_SECRET_KEY in the environment to enable encrypted storage",
                        e
                    )
                })),
            )
                .into_response();
        }
    };

    let result = sqlx::query(
        "INSERT INTO exchange_keys (exchange, account_name, api_key, api_secret, passphrase, referred_uid, is_active, last_sync_timestamp) VALUES (?, ?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(&req.exchange)
    .bind(&req.account_name)
    .bind(&req.api_key)
    .bind(&secret)
    .bind(&passphrase)
    .bind(&req.referred_uid)
    .bind(is_active_int)
    .execute(&state.pool)
    .await;

    match result {
        Ok(r) => {
            let id = r.last_insert_rowid();
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "id": id,
                    "exchange": req.exchange,
                    "account_name": req.account_name,
                    "message": "Key stored successfully"
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to store key: {}", e)
            })),
        )
            .into_response(),
    }
}

pub async fn delete_key(
    State(state): State<Arc<AppState>>,
    Path(key_id): Path<String>,
) -> impl IntoResponse {
    let id: i64 = match key_id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid key ID" })),
            )
                .into_response();
        }
    };

    let result = sqlx::query("DELETE FROM exchange_keys WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => Json(serde_json::json!({
            "id": id,
            "message": "Key deleted"
        }))
        .into_response(),
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Key not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to delete key: {}", e)
            })),
        )
            .into_response(),
    }
}

// ─── Rotation & backup (AUDIT-V6-077) ────────────────────────────────

/// POST /api/keys/rotate — in-process re-encryption of all stored
/// credentials under a new master key (no daemon restart).
#[derive(Deserialize)]
pub struct RotateKeysRequest {
    pub new_master_secret: String,
}

pub async fn rotate_keys(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RotateKeysRequest>,
) -> impl IntoResponse {
    if req.new_master_secret.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "new_master_secret required" })),
        )
            .into_response();
    }

    // 1. Decrypt every stored secret with the CURRENT master key.
    let rows: Vec<(i64, String, String)> =
        match sqlx::query_as("SELECT id, api_secret, COALESCE(passphrase, '') FROM exchange_keys")
            .fetch_all(&state.pool)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": format!("Read failed: {}", e) })),
                )
                    .into_response();
            }
        };

    let old_key = match database_storage::crypto::get_master_key() {
        Some(k) => k,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "EXCHANGE_SECRET_KEY is not set" })),
            )
                .into_response();
        }
    };

    let mut decrypted: Vec<(i64, String, String)> = Vec::with_capacity(rows.len());
    for (id, secret, passphrase) in rows {
        let secret_plain = match database_storage::crypto::decrypt_with_key(&secret, &old_key) {
            Ok(s) => s,
            Err(e) => {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": format!("Row {} cannot be decrypted with the current key: {}", id, e)
                    })),
                )
                    .into_response();
            }
        };
        let pass_plain = if passphrase.is_empty() {
            String::new()
        } else {
            match database_storage::crypto::decrypt_with_key(&passphrase, &old_key) {
                Ok(s) => s,
                Err(e) => {
                    return (
                        StatusCode::CONFLICT,
                        Json(serde_json::json!({
                            "error": format!("Row {} passphrase cannot be decrypted: {}", id, e)
                        })),
                    )
                        .into_response();
                }
            }
        };
        decrypted.push((id, secret_plain, pass_plain));
    }

    // 2. Swap the in-process master key.
    let new_key = match database_storage::crypto::rotate_master_key(&req.new_master_secret) {
        Some(k) => k,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to install the new master key" })),
            )
                .into_response();
        }
    };

    // 3. Re-encrypt everything under the new key.
    for (id, secret_plain, pass_plain) in &decrypted {
        let new_secret = database_storage::crypto::encrypt_with_key(secret_plain, &new_key);
        let new_pass = if pass_plain.is_empty() {
            String::new()
        } else {
            database_storage::crypto::encrypt_with_key(pass_plain, &new_key).unwrap_or_default()
        };
        let new_secret = match new_secret {
            Ok(s) => s,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": format!("Re-encrypt failed: {}", e) })),
                )
                    .into_response();
            }
        };
        if let Err(e) =
            sqlx::query("UPDATE exchange_keys SET api_secret = ?, passphrase = ? WHERE id = ?")
                .bind(&new_secret)
                .bind(&new_pass)
                .bind(id)
                .execute(&state.pool)
                .await
        {
            eprintln!("DB persist failed: {e}");
        }
    }

    Json(serde_json::json!({
        "success": true,
        "rotated": decrypted.len(),
        "message": "All stored credentials re-encrypted under the new master key"
    }))
    .into_response()
}

/// GET /api/keys/backup?passphrase= — encrypted-backup export (legacy, query-param).
/// Prefer POST /api/keys/backup with JSON body `{"passphrase": "..."}` to avoid
/// logging the secret in access logs. GET is retained for backward compat.
#[derive(Deserialize)]
pub struct BackupKeyQuery {
    pub passphrase: String,
}

#[derive(Deserialize)]
pub struct BackupKeyRequest {
    pub passphrase: String,
}

async fn backup_keys_internal(
    state: Arc<AppState>,
    passphrase: String,
) -> axum::response::Response {
    if passphrase.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "passphrase required" })),
        )
            .into_response();
    }

    type KeyRow = (i64, String, String, String, String, i32, Option<i64>);
    let rows: Vec<KeyRow> = match sqlx::query_as(
        "SELECT id, exchange, account_name, api_key, api_secret, is_active, last_sync_timestamp FROM exchange_keys ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Read failed: {}", e) })),
            )
                .into_response();
        }
    };

    let backup_key = database_storage::crypto::backup_key_from_passphrase(&passphrase);
    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(id, exchange, account_name, api_key, api_secret, is_active, last_sync)| {
                // Decrypt with the master key, then re-encrypt with the backup key.
                let plain =
                    database_storage::crypto::decrypt_field(&api_secret).unwrap_or_default();
                let secret_enc = database_storage::crypto::encrypt_with_key(&plain, &backup_key)
                    .unwrap_or_default();
                serde_json::json!({
                    "id": id,
                    "exchange": exchange,
                    "account_name": account_name,
                    "api_key": api_key,
                    "api_secret_encrypted": secret_enc,
                    "is_active": is_active,
                    "last_sync_timestamp": last_sync,
                })
            },
        )
        .collect();

    Json(serde_json::json!({
        "items": items,
        "note": "api_secret_encrypted is AES-256-GCM encrypted with the passphrase-derived key; restore with the same passphrase"
    }))
    .into_response()
}

pub async fn backup_keys(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BackupKeyQuery>,
) -> impl IntoResponse {
    eprintln!("WARN: GET /api/keys/backup via query param is deprecated — use POST with JSON body to avoid logging the passphrase");
    backup_keys_internal(state, query.passphrase).await
}

pub async fn backup_keys_post(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BackupKeyRequest>,
) -> impl IntoResponse {
    backup_keys_internal(state, body.passphrase).await
}
