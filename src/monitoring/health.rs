// Health check endpoint for the HFT engine.
//
// GET /health returns JSON with status, uptime, and connected services.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub connected: bool,
    pub last_heartbeat: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub uptime_secs: u64,
    pub started_at: DateTime<Utc>,
    pub services: Vec<ServiceStatus>,
    pub active_strategies: Vec<String>,
}

/// Shared health state, updated by various subsystems.
pub struct HealthChecker {
    started_at: DateTime<Utc>,
    services: Arc<RwLock<Vec<ServiceStatus>>>,
    active_strategies: Arc<RwLock<Vec<String>>>,
}

impl HealthChecker {
    pub fn new(started_at: DateTime<Utc>) -> Self {
        Self {
            started_at,
            services: Arc::new(RwLock::new(Vec::new())),
            active_strategies: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a service for health tracking.
    pub fn register_service(&self, name: &str) {
        let mut services = self.services.write();
        if !services.iter().any(|s| s.name == name) {
            services.push(ServiceStatus {
                name: name.to_string(),
                connected: false,
                last_heartbeat: None,
            });
        }
    }

    /// Update a service's connection status.
    pub fn update_service(&self, name: &str, connected: bool, heartbeat: Option<DateTime<Utc>>) {
        let mut services = self.services.write();
        if let Some(svc) = services.iter_mut().find(|s| s.name == name) {
            svc.connected = connected;
            if heartbeat.is_some() {
                svc.last_heartbeat = heartbeat;
            }
        }
    }

    /// Set the list of currently active strategies.
    pub fn set_active_strategies(&self, strategies: Vec<String>) {
        *self.active_strategies.write() = strategies;
    }

    /// Build the health response at the given point in time.
    pub fn check(&self, now: DateTime<Utc>) -> HealthResponse {
        let services = self.services.read().clone();
        let active_strategies = self.active_strategies.read().clone();

        let uptime = (now - self.started_at).num_seconds().max(0) as u64;

        let status = if services.is_empty() || services.iter().all(|s| s.connected) {
            HealthStatus::Healthy
        } else if services.iter().any(|s| s.connected) {
            HealthStatus::Degraded
        } else {
            HealthStatus::Unhealthy
        };

        HealthResponse {
            status,
            uptime_secs: uptime,
            started_at: self.started_at,
            services,
            active_strategies,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn base_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 2, 16, 12, 0, 0).unwrap()
    }

    #[test]
    fn test_health_returns_healthy_when_all_connected() {
        let checker = HealthChecker::new(base_time());
        checker.register_service("clob_ws");
        checker.register_service("data_api");
        checker.update_service("clob_ws", true, Some(base_time()));
        checker.update_service("data_api", true, Some(base_time()));

        let now = base_time() + chrono::Duration::seconds(60);
        let resp = checker.check(now);

        assert_eq!(resp.status, HealthStatus::Healthy);
        assert_eq!(resp.uptime_secs, 60);
        assert_eq!(resp.services.len(), 2);

        // Verify JSON serialization works
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"healthy\""));
    }

    #[test]
    fn test_health_returns_degraded_when_partial() {
        let checker = HealthChecker::new(base_time());
        checker.register_service("clob_ws");
        checker.register_service("data_api");
        checker.update_service("clob_ws", true, Some(base_time()));
        // data_api stays disconnected

        let resp = checker.check(base_time());
        assert_eq!(resp.status, HealthStatus::Degraded);
    }

    #[test]
    fn test_health_returns_unhealthy_when_none_connected() {
        let checker = HealthChecker::new(base_time());
        checker.register_service("clob_ws");
        checker.register_service("data_api");
        // Both disconnected

        let resp = checker.check(base_time());
        assert_eq!(resp.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_health_healthy_when_no_services() {
        let checker = HealthChecker::new(base_time());
        let resp = checker.check(base_time());
        assert_eq!(resp.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_active_strategies_tracked() {
        let checker = HealthChecker::new(base_time());
        checker.set_active_strategies(vec![
            "spread_farming".to_string(),
            "copy_trade".to_string(),
        ]);

        let resp = checker.check(base_time());
        assert_eq!(resp.active_strategies.len(), 2);
        assert!(resp.active_strategies.contains(&"spread_farming".to_string()));
    }

    #[test]
    fn test_health_response_json_format() {
        let checker = HealthChecker::new(base_time());
        checker.register_service("clob_ws");
        checker.update_service("clob_ws", true, Some(base_time()));
        checker.set_active_strategies(vec!["lp".to_string()]);

        let resp = checker.check(base_time() + chrono::Duration::seconds(120));
        let json = serde_json::to_value(&resp).unwrap();

        assert_eq!(json["status"], "healthy");
        assert_eq!(json["uptime_secs"], 120);
        assert!(json["started_at"].is_string());
        assert!(json["services"].is_array());
        assert!(json["active_strategies"].is_array());
    }
}
