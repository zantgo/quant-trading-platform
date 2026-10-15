//! v9 Strategy CRUD — the single source of truth for model behavior.
//!
//! The strategy JSON (schema_version + base inheritance + MME L1–L7 / TAE /
//! PME / PAE sections) is stored in the workspace config, editable and
//! exportable as JSON, understood identically by the CLI.

use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use config_models::StrategyConfig;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct StrategyUpsert {
    pub name: String,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Full or partial strategy JSON — merged over the resolved base
    /// (patch inheritance).
    pub strategy: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct StrategyCloneRequest {
    pub new_name: String,
}

fn strategy_summary(s: &StrategyConfig) -> serde_json::Value {
    serde_json::json!({
        "name": s.name,
        "base": s.base,
        "description": s.description,
        "schema_version": s.schema_version,
    })
}

pub async fn list_strategies(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut workspace = state.workspace.config().await;
    workspace.ensure_default_strategy();
    state.workspace.set_config(workspace.clone()).await;
    let list: Vec<serde_json::Value> = workspace.strategies.iter().map(strategy_summary).collect();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "strategies": list })),
    )
}

pub async fn get_strategy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let workspace = state.workspace.config().await;
    match workspace.resolve_strategy(&name) {
        Ok(resolved) => (
            StatusCode::OK,
            Json(serde_json::to_value(&resolved).unwrap()),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e })),
        ),
    }
}

async fn upsert_impl(state: Arc<AppState>, payload: StrategyUpsert) -> impl IntoResponse {
    if payload.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "strategy name must not be empty" })),
        );
    }
    let workspace = state.workspace.config().await;

    // Resolve against the declared base (or the built-in default) so the
    // stored entry is the full resolved strategy — later edits re-resolve.
    let base_json = match &payload.base {
        Some(b) => match workspace.resolve_strategy(b) {
            Ok(r) => Some(serde_json::to_value(&r).unwrap()),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": e })),
                );
            }
        },
        None => None,
    };
    let resolved = match StrategyConfig::resolve(base_json.as_ref(), &payload.strategy) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e })),
            )
        }
    };
    let problems = resolved.validate();
    if problems
        .iter()
        .any(|p| p.contains("schema_version") || p.contains("must not be empty"))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": problems.join("; ") })),
        );
    }

    let mut entry = resolved;
    entry.name = payload.name.clone();
    entry.base = payload.base.clone();
    if let Some(d) = payload.description {
        entry.description = d;
    }

    // AUDIT-M8: persist atomically under the workspace write lock. The old
    // `config()` -> mutate -> `save_workspace` -> `set_config` sequence released
    // the lock between the read and the write, so a concurrent mutator (a
    // ladder edit, an instance add, the boot-spawn retry loop) could persist a
    // snapshot taken before this edit and silently discard it while this
    // handler returned 200.
    let strategy_name = payload.name.clone();
    if let Err(e) = state
        .workspace
        .mutate_and_persist(move |workspace| {
            if let Some(existing) = workspace
                .strategies
                .iter_mut()
                .find(|s| s.name == strategy_name)
            {
                *existing = entry;
            } else {
                workspace.strategies.push(entry);
            }
            workspace.config_version = workspace.config_version.saturating_add(1);
            Some(workspace.clone())
        })
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("persist failed: {e}") })),
        );
    }

    // Strategy edits affect the MME/TAE behavior of bound instances —
    // recharge them (idempotent; failures logged, never fatal).
    let ctx = state.registry_context();
    for key in state.workspace.keys().await {
        if let Err(e) = portfolio_supervisor::registry::recharge_instance(&ctx, &key).await {
            eprintln!("strategy recharge failed for {key}: {e}");
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "name": payload.name,
            "warnings": problems,
        })),
    )
}

pub async fn create_strategy(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StrategyUpsert>,
) -> impl IntoResponse {
    upsert_impl(state, payload).await
}

pub async fn update_strategy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(mut payload): Json<StrategyUpsert>,
) -> impl IntoResponse {
    payload.name = name;
    upsert_impl(state, payload).await
}

pub async fn delete_strategy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if name == "default" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "the default strategy is locked" })),
        );
    }
    let workspace = state.workspace.config().await;
    // Block deletion while another strategy inherits from it.
    if let Some(dependent) = workspace
        .strategies
        .iter()
        .find(|s| s.base.as_deref() == Some(name.as_str()))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("strategy '{}' inherits from '{}' — delete or rebase it first", dependent.name, name)
            })),
        );
    }
    let exists = workspace.strategies.iter().any(|s| s.name == name);
    if !exists {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("strategy '{name}' not found") })),
        );
    }
    // AUDIT-M8: the removal and the write happen under one lock acquisition.
    let target = name.clone();
    let outcome = state
        .workspace
        .mutate_and_persist(move |workspace| {
            let before = workspace.strategies.len();
            workspace.strategies.retain(|s| s.name != target);
            if workspace.strategies.len() == before {
                return None;
            }
            workspace.config_version = workspace.config_version.saturating_add(1);
            Some(workspace.clone())
        })
        .await;
    match outcome {
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("persist failed: {e}") })),
            )
        }
        Ok(persisted) => {
            if persisted.strategies.iter().any(|s| s.name == name) {
                // Another writer restored it between the check and the lock —
                // report the truth rather than claiming a delete that did not
                // happen.
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": format!("strategy '{name}' not found") })),
                );
            }
        }
    }
    (StatusCode::OK, Json(serde_json::json!({ "success": true })))
}

pub async fn clone_strategy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<StrategyCloneRequest>,
) -> impl IntoResponse {
    let workspace = state.workspace.config().await;
    let source = match workspace.resolve_strategy(&name) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e })),
            )
        }
    };
    if workspace
        .strategies
        .iter()
        .any(|s| s.name == payload.new_name)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "error": format!("strategy '{}' already exists", payload.new_name) }),
            ),
        );
    }
    let mut entry = source;
    entry.name = payload.new_name.clone();
    entry.base = None; // fully-resolved clone — no inheritance chain
                       // AUDIT-M8: atomic with respect to concurrent mutators.
    let new_name = payload.new_name.clone();
    if let Err(e) = state
        .workspace
        .mutate_and_persist(move |workspace| {
            // Re-check under the lock: the existence test above ran against a
            // snapshot that another mutator may already have changed.
            if workspace.strategies.iter().any(|s| s.name == new_name) {
                return None;
            }
            workspace.strategies.push(entry);
            workspace.config_version = workspace.config_version.saturating_add(1);
            Some(workspace.clone())
        })
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("persist failed: {e}") })),
        );
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({ "success": true, "name": payload.new_name })),
    )
}
