use config_models::LifecycleState;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct AutomationConditions {
    pub start_at_price_above: Option<f64>,
    pub start_at_price_below: Option<f64>,
    pub start_at_time: Option<String>,
    pub pause_at_price_below: Option<f64>,
    pub pause_at_price_above: Option<f64>,
    pub pause_at_time: Option<String>,
    pub pause_after_duration_secs: Option<u64>,
    pub stop_at_price_above: Option<f64>,
    pub stop_at_price_below: Option<f64>,
    pub stop_after_duration_secs: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AutomationState {
    pub conditions: AutomationConditions,
    pub start_fired: bool,
    pub pause_fired: bool,
    pub stop_fired: bool,
}

impl AutomationState {
    pub fn new(conditions: AutomationConditions) -> Self {
        Self {
            conditions,
            start_fired: false,
            pause_fired: false,
            stop_fired: false,
        }
    }

    pub fn re_arm(&mut self) {
        self.start_fired = false;
        self.pause_fired = false;
        self.stop_fired = false;
    }
}

#[derive(Debug, Clone)]
pub struct LifecycleEvent {
    pub instance_id: String,
    pub from_state: Option<LifecycleState>,
    pub to_state: LifecycleState,
    pub actor: String,
    pub reason: Option<String>,
    pub timestamp_ms: u64,
}

pub struct LifecycleManager {
    pub state: LifecycleState,
    pub automation: Option<AutomationState>,
    pub entered_state_at_ms: u64,
    pub events: Vec<LifecycleEvent>,
    pub instance_id: String,
    pub pool: Option<Arc<SqlitePool>>,
}

impl LifecycleManager {
    pub fn new(automation: Option<AutomationState>) -> Self {
        Self::new_for_mode(automation, None)
    }

    /// v10.1 boot policy — the initial state follows the instance's
    /// execution mode:
    ///   - `Observe` → RUNNING (ghost radar always evaluates; the mode
    ///     itself forbids dispatch).
    ///   - `Paper`/`Live` → PAUSED (close-only: the instance runs but the
    ///     TAE does not open new setups until the operator starts it —
    ///     "never start trading unless explicitly activated").
    ///   - automation with start-at triggers → STOPPED (waits for the
    ///     trigger; unchanged legacy rule).
    ///
    /// `mode = None` keeps the legacy RUNNING default (tests, ad-hoc).
    pub fn new_for_mode(
        automation: Option<AutomationState>,
        mode: Option<config_models::ExecutionMode>,
    ) -> Self {
        let now = current_time_ms();
        let initial_state = match &automation {
            Some(auto)
                if auto.conditions.start_at_time.is_some()
                    || auto.conditions.start_at_price_above.is_some()
                    || auto.conditions.start_at_price_below.is_some() =>
            {
                LifecycleState::Stopped
            }
            _ => match mode {
                Some(config_models::ExecutionMode::Observe) => LifecycleState::Running,
                Some(_) => LifecycleState::LifecyclePaused,
                None => LifecycleState::Running,
            },
        };

        Self {
            state: initial_state,
            automation,
            entered_state_at_ms: now,
            events: vec![],
            instance_id: String::new(),
            pool: None,
        }
    }

    pub fn set_db(&mut self, instance_id: String, pool: Arc<SqlitePool>) {
        self.instance_id = instance_id;
        self.pool = Some(pool);
    }

    /// Current lifecycle state (clone — the enum is `Copy`-free by design).
    pub fn current(&self) -> LifecycleState {
        self.state
    }

    pub async fn persist_lifecycle(&self) {
        if let Some(ref pool) = self.pool {
            let now = current_time_ms();
            let state_str = match self.state {
                LifecycleState::Running => "RUNNING",
                LifecycleState::LifecyclePaused => "PAUSED",
                LifecycleState::Stopping => "STOPPING",
                LifecycleState::Stopped => "STOPPED",
            };
            if let Err(e) = sqlx::query(
                "INSERT OR REPLACE INTO instance_lifecycle (instance_id, lifecycle_state, entered_state_at_ms, updated_at_ms) VALUES (?, ?, ?, ?)"
            )
            .bind(&self.instance_id)
            .bind(state_str)
            .bind(self.entered_state_at_ms as i64)
            .bind(now as i64)
            .execute(pool.as_ref())
            .await
            {
                eprintln!("persist lifecycle failed for {}: {e}", self.instance_id);
            }
        }
    }

    pub async fn persist_event(&self, event: &LifecycleEvent) {
        if let Some(ref pool) = self.pool {
            let from_str = event.from_state.map(|s| match s {
                LifecycleState::Running => "RUNNING",
                LifecycleState::LifecyclePaused => "PAUSED",
                LifecycleState::Stopping => "STOPPING",
                LifecycleState::Stopped => "STOPPED",
            });
            let to_str = match event.to_state {
                LifecycleState::Running => "RUNNING",
                LifecycleState::LifecyclePaused => "PAUSED",
                LifecycleState::Stopping => "STOPPING",
                LifecycleState::Stopped => "STOPPED",
            };
            if let Err(e) = sqlx::query(
                "INSERT INTO instance_lifecycle_events (instance_id, from_state, to_state, actor, reason_json, timestamp_ms) VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(&self.instance_id)
            .bind(from_str)
            .bind(to_str)
            .bind(&event.actor)
            .bind(&event.reason)
            .bind(event.timestamp_ms as i64)
            .execute(pool.as_ref())
            .await
            {
                eprintln!("persist lifecycle event failed for {}: {e}", self.instance_id);
            }
        }
    }

    pub async fn start(&mut self, actor: &str, reason: Option<String>) -> Result<(), String> {
        self.can_start()?;
        let event = self.make_event(self.state, LifecycleState::Running, actor, reason);
        self.state = LifecycleState::Running;
        self.entered_state_at_ms = current_time_ms();
        self.events.push(event.clone());
        self.persist_event(&event).await;
        self.persist_lifecycle().await;
        Ok(())
    }

    pub async fn pause(&mut self, actor: &str, reason: Option<String>) -> Result<(), String> {
        self.can_pause()?;
        let event = self.make_event(self.state, LifecycleState::LifecyclePaused, actor, reason);
        self.state = LifecycleState::LifecyclePaused;
        self.events.push(event.clone());
        self.persist_event(&event).await;
        self.persist_lifecycle().await;
        Ok(())
    }

    pub async fn stop(&mut self, actor: &str, reason: Option<String>) -> Result<(), String> {
        self.can_stop()?;
        let event = self.make_event(self.state, LifecycleState::Stopping, actor, reason);
        self.state = LifecycleState::Stopping;
        self.events.push(event.clone());
        self.persist_event(&event).await;
        self.persist_lifecycle().await;
        Ok(())
    }

    pub async fn complete_stop(&mut self) -> Result<(), String> {
        if self.state != LifecycleState::Stopping {
            return Err("Cannot complete stop: not in STOPPING state".into());
        }
        let event = self.make_event(
            self.state,
            LifecycleState::Stopped,
            "system",
            Some("Flatten complete: 0 positions, 0 open orders".into()),
        );
        self.state = LifecycleState::Stopped;
        self.events.push(event.clone());
        self.persist_event(&event).await;
        self.persist_lifecycle().await;
        Ok(())
    }

    fn make_event(
        &self,
        from: LifecycleState,
        to: LifecycleState,
        actor: &str,
        reason: Option<String>,
    ) -> LifecycleEvent {
        LifecycleEvent {
            instance_id: self.instance_id.clone(),
            from_state: Some(from),
            to_state: to,
            actor: actor.to_string(),
            reason,
            timestamp_ms: current_time_ms(),
        }
    }

    pub fn can_start(&self) -> Result<(), String> {
        match self.state {
            LifecycleState::LifecyclePaused | LifecycleState::Stopped => Ok(()),
            LifecycleState::Running => Err("Instance is already RUNNING".into()),
            LifecycleState::Stopping => Err("Cannot start while STOPPING; wait for STOPPED".into()),
        }
    }

    pub fn can_pause(&self) -> Result<(), String> {
        match self.state {
            LifecycleState::Running => Ok(()),
            _ => Err(format!("Cannot pause from state {}", self.state.as_str())),
        }
    }

    pub fn can_stop(&self) -> Result<(), String> {
        match self.state {
            LifecycleState::Running | LifecycleState::LifecyclePaused => Ok(()),
            _ => Err(format!("Cannot stop from state {}", self.state.as_str())),
        }
    }

    pub fn can_delete(&self) -> Result<(), String> {
        if self.state == LifecycleState::Stopped {
            Ok(())
        } else {
            Err("Instance must be STOPPED before deletion".into())
        }
    }

    pub fn evaluate_automation(&mut self, current_price: Option<f64>) -> Vec<AutomationAction> {
        let auto = match &mut self.automation {
            Some(a) => a,
            None => return vec![],
        };

        let now_ms = current_time_ms();
        let now_secs = now_ms / 1000;
        let mut actions = Vec::new();

        let at_time_fired = |time_str: &Option<String>| -> bool {
            if let Some(ref ts) = time_str {
                if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) {
                    let deadline = parsed.timestamp() as u64;
                    return now_secs >= deadline;
                }
            }
            false
        };

        match self.state {
            LifecycleState::Stopped if !auto.start_fired => {
                if let Some(price) = current_price {
                    if let Some(above) = auto.conditions.start_at_price_above {
                        if price >= above {
                            auto.start_fired = true;
                            actions.push(AutomationAction::Start);
                        }
                    }
                    if !auto.start_fired {
                        if let Some(below) = auto.conditions.start_at_price_below {
                            if price <= below {
                                auto.start_fired = true;
                                actions.push(AutomationAction::Start);
                            }
                        }
                    }
                }
                if !auto.start_fired && at_time_fired(&auto.conditions.start_at_time) {
                    auto.start_fired = true;
                    actions.push(AutomationAction::Start);
                }
            }
            LifecycleState::Running => {
                let mut has_stop = false;
                let mut has_pause = false;

                // Check time-based conditions first (OR with price-based)
                if !auto.stop_fired {
                    if let Some(duration_secs) = auto.conditions.stop_after_duration_secs {
                        let elapsed = (now_ms - self.entered_state_at_ms) / 1000;
                        if elapsed >= duration_secs {
                            auto.stop_fired = true;
                            has_stop = true;
                        }
                    }
                }
                if !auto.pause_fired {
                    if let Some(duration_secs) = auto.conditions.pause_after_duration_secs {
                        let elapsed = (now_ms - self.entered_state_at_ms) / 1000;
                        if elapsed >= duration_secs {
                            auto.pause_fired = true;
                            has_pause = true;
                        }
                    }
                }

                if !auto.pause_fired && at_time_fired(&auto.conditions.pause_at_time) {
                    auto.pause_fired = true;
                    has_pause = true;
                }

                if let Some(price) = current_price {
                    if !auto.stop_fired && !has_stop {
                        if let Some(above) = auto.conditions.stop_at_price_above {
                            if price >= above {
                                auto.stop_fired = true;
                                has_stop = true;
                            }
                        }
                        if let Some(below) = auto.conditions.stop_at_price_below {
                            if price <= below {
                                auto.stop_fired = true;
                                has_stop = true;
                            }
                        }
                    }
                    if !auto.pause_fired && !has_pause {
                        if let Some(below) = auto.conditions.pause_at_price_below {
                            if price <= below {
                                auto.pause_fired = true;
                                has_pause = true;
                            }
                        }
                        if let Some(above) = auto.conditions.pause_at_price_above {
                            if price >= above {
                                auto.pause_fired = true;
                                has_pause = true;
                            }
                        }
                    }
                }

                // Same-tick collision: stop > pause (per IL-12)
                if has_stop {
                    actions.push(AutomationAction::Stop);
                }
                if has_pause {
                    actions.push(AutomationAction::Pause);
                }
            }
            LifecycleState::LifecyclePaused if !auto.start_fired => {
                if let Some(price) = current_price {
                    if let Some(above) = auto.conditions.start_at_price_above {
                        if price >= above {
                            auto.start_fired = true;
                            actions.push(AutomationAction::Start);
                        }
                    }
                }
                if !auto.start_fired && at_time_fired(&auto.conditions.start_at_time) {
                    auto.start_fired = true;
                    actions.push(AutomationAction::Start);
                }
            }
            _ => {}
        }

        actions
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AutomationAction {
    Start,
    Pause,
    Stop,
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_policy_is_mode_aware() {
        // v10.1: paper/live boot PAUSED (TAE not activated), observe boots
        // RUNNING (ghost radar), legacy None stays RUNNING.
        let paper = LifecycleManager::new_for_mode(None, Some(config_models::ExecutionMode::Paper));
        assert_eq!(paper.state, LifecycleState::LifecyclePaused);

        let live = LifecycleManager::new_for_mode(None, Some(config_models::ExecutionMode::Live));
        assert_eq!(live.state, LifecycleState::LifecyclePaused);

        let observe =
            LifecycleManager::new_for_mode(None, Some(config_models::ExecutionMode::Observe));
        assert_eq!(observe.state, LifecycleState::Running);

        let legacy = LifecycleManager::new(None);
        assert_eq!(legacy.state, LifecycleState::Running);
    }

    #[test]
    fn automation_start_triggers_boot_stopped_regardless_of_mode() {
        let cond = AutomationConditions {
            start_at_time: Some("2026-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };
        let mgr = LifecycleManager::new_for_mode(
            Some(AutomationState::new(cond)),
            Some(config_models::ExecutionMode::Paper),
        );
        assert_eq!(mgr.state, LifecycleState::Stopped);
    }

    #[test]
    fn paused_can_start_and_stop() {
        let mgr = LifecycleManager::new_for_mode(None, Some(config_models::ExecutionMode::Paper));
        assert!(mgr.can_start().is_ok());
        assert!(mgr.can_stop().is_ok());
        assert!(mgr.can_pause().is_err(), "cannot pause from PAUSED");
    }
}
