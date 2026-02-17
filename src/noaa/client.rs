// NOAA weather API client — used by weather strategy.
//
// Flow: /points/{lat},{lon} -> gridpoint ({wfo}/{x},{y}) -> /gridpoints/{wfo}/{x},{y}/forecast
// Cache gridpoint mapping, re-validate every 24 hours.
// No API key required (api.weather.gov is free).

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{AppError, Result};

/// Cached gridpoint mapping for a lat/lon.
#[derive(Debug, Clone)]
struct GridpointCache {
    wfo: String,
    grid_x: u32,
    grid_y: u32,
    cached_at: DateTime<Utc>,
}

/// A single forecast period from the NOAA API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastPeriod {
    pub name: String,
    pub temperature: i32,
    #[serde(rename = "temperatureUnit")]
    pub temperature_unit: String,
    #[serde(rename = "shortForecast")]
    pub short_forecast: String,
    #[serde(rename = "startTime")]
    pub start_time: String,
    #[serde(rename = "endTime")]
    pub end_time: String,
}

/// NOAA NWS API client with gridpoint caching.
pub struct NoaaClient {
    http: Client,
    base_url: String,
    cache: RwLock<HashMap<String, GridpointCache>>,
    cache_ttl_hours: i64,
}

impl NoaaClient {
    pub fn new(cache_ttl_hours: i64) -> Self {
        Self {
            http: Client::builder()
                .user_agent("polymarket-hft/1.0")
                .build()
                .expect("failed to build http client"),
            base_url: "https://api.weather.gov".to_string(),
            cache: RwLock::new(HashMap::new()),
            cache_ttl_hours,
        }
    }

    /// Create with a custom base URL (for testing with mock server).
    pub fn with_base_url(base_url: &str, cache_ttl_hours: i64) -> Self {
        Self {
            http: Client::builder()
                .user_agent("polymarket-hft/1.0")
                .build()
                .expect("failed to build http client"),
            base_url: base_url.trim_end_matches('/').to_string(),
            cache: RwLock::new(HashMap::new()),
            cache_ttl_hours,
        }
    }

    /// Get the gridpoint (WFO, grid_x, grid_y) for a lat/lon, using cache.
    async fn get_gridpoint(
        &self,
        lat: f64,
        lon: f64,
        now: DateTime<Utc>,
    ) -> Result<(String, u32, u32)> {
        let cache_key = format!("{:.4},{:.4}", lat, lon);

        // Check cache
        {
            let cache = self.cache.read();
            if let Some(entry) = cache.get(&cache_key) {
                let age = now - entry.cached_at;
                if age < Duration::hours(self.cache_ttl_hours) {
                    return Ok((entry.wfo.clone(), entry.grid_x, entry.grid_y));
                }
            }
        }

        // Fetch from API
        let url = format!("{}/points/{},{}", self.base_url, lat, lon);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("noaa points request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(AppError::Http(format!(
                "noaa points returned status {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Http(format!("failed to parse noaa points: {}", e)))?;

        let props = &body["properties"];
        let wfo = props["gridId"]
            .as_str()
            .ok_or_else(|| AppError::Http("missing gridId".to_string()))?
            .to_string();
        let grid_x = props["gridX"]
            .as_u64()
            .ok_or_else(|| AppError::Http("missing gridX".to_string()))? as u32;
        let grid_y = props["gridY"]
            .as_u64()
            .ok_or_else(|| AppError::Http("missing gridY".to_string()))? as u32;

        // Update cache
        {
            let mut cache = self.cache.write();
            cache.insert(
                cache_key,
                GridpointCache {
                    wfo: wfo.clone(),
                    grid_x,
                    grid_y,
                    cached_at: now,
                },
            );
        }

        Ok((wfo, grid_x, grid_y))
    }

    /// Fetch the forecast for a lat/lon.
    pub async fn get_forecast(
        &self,
        lat: f64,
        lon: f64,
        now: DateTime<Utc>,
    ) -> Result<Vec<ForecastPeriod>> {
        let (wfo, x, y) = self.get_gridpoint(lat, lon, now).await?;
        let url = format!("{}/gridpoints/{}/{},{}/forecast", self.base_url, wfo, x, y);

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("noaa forecast request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(AppError::Http(format!(
                "noaa forecast returned status {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Http(format!("failed to parse noaa forecast: {}", e)))?;

        let periods_json = body["properties"]["periods"]
            .as_array()
            .ok_or_else(|| AppError::Http("missing forecast periods".to_string()))?;

        let periods: Vec<ForecastPeriod> = periods_json
            .iter()
            .filter_map(|p| serde_json::from_value(p.clone()).ok())
            .collect();

        Ok(periods)
    }

    /// Check if a gridpoint is cached for a given lat/lon.
    pub fn is_cached(&self, lat: f64, lon: f64) -> bool {
        let key = format!("{:.4},{:.4}", lat, lon);
        self.cache.read().contains_key(&key)
    }

    /// Number of cached gridpoints.
    pub fn cache_size(&self) -> usize {
        self.cache.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn base_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 2, 16, 12, 0, 0).unwrap()
    }

    fn points_response_json() -> &'static str {
        r#"{
            "properties": {
                "gridId": "OKX",
                "gridX": 33,
                "gridY": 37
            }
        }"#
    }

    fn forecast_response_json() -> &'static str {
        r#"{
            "properties": {
                "periods": [
                    {
                        "name": "Tonight",
                        "temperature": 28,
                        "temperatureUnit": "F",
                        "shortForecast": "Partly Cloudy",
                        "startTime": "2026-02-16T18:00:00-05:00",
                        "endTime": "2026-02-17T06:00:00-05:00"
                    },
                    {
                        "name": "Tuesday",
                        "temperature": 42,
                        "temperatureUnit": "F",
                        "shortForecast": "Sunny",
                        "startTime": "2026-02-17T06:00:00-05:00",
                        "endTime": "2026-02-17T18:00:00-05:00"
                    }
                ]
            }
        }"#
    }

    #[tokio::test]
    async fn test_gridpoint_cache_and_revalidation() {
        let mut server = mockito::Server::new_async().await;

        let points_mock = server
            .mock("GET", "/points/40.7128,-74.006")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(points_response_json())
            .expect(2) // Should be called twice: initial + revalidation
            .create_async()
            .await;

        let client = NoaaClient::with_base_url(&server.url(), 24);

        // First call: fetches from API
        let (wfo, x, y) = client.get_gridpoint(40.7128, -74.006, base_time()).await.unwrap();
        assert_eq!(wfo, "OKX");
        assert_eq!(x, 33);
        assert_eq!(y, 37);
        assert!(client.is_cached(40.7128, -74.006));

        // Second call within TTL: uses cache (no HTTP call)
        let (wfo2, x2, y2) = client
            .get_gridpoint(40.7128, -74.006, base_time() + Duration::hours(1))
            .await
            .unwrap();
        assert_eq!(wfo2, "OKX");
        assert_eq!(x2, 33);

        // Third call after TTL: re-fetches from API
        let (wfo3, _, _) = client
            .get_gridpoint(40.7128, -74.006, base_time() + Duration::hours(25))
            .await
            .unwrap();
        assert_eq!(wfo3, "OKX");

        points_mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_get_forecast() {
        let mut server = mockito::Server::new_async().await;

        let _points_mock = server
            .mock("GET", "/points/40.7128,-74.006")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(points_response_json())
            .create_async()
            .await;

        let _forecast_mock = server
            .mock("GET", "/gridpoints/OKX/33,37/forecast")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(forecast_response_json())
            .create_async()
            .await;

        let client = NoaaClient::with_base_url(&server.url(), 24);
        let periods = client.get_forecast(40.7128, -74.006, base_time()).await.unwrap();

        assert_eq!(periods.len(), 2);
        assert_eq!(periods[0].name, "Tonight");
        assert_eq!(periods[0].temperature, 28);
        assert_eq!(periods[1].name, "Tuesday");
        assert_eq!(periods[1].temperature, 42);
    }

    #[test]
    fn test_forecast_period_deserialization() {
        let json = r#"{
            "name": "Tonight",
            "temperature": 28,
            "temperatureUnit": "F",
            "shortForecast": "Partly Cloudy",
            "startTime": "2026-02-16T18:00:00-05:00",
            "endTime": "2026-02-17T06:00:00-05:00"
        }"#;

        let period: ForecastPeriod = serde_json::from_str(json).unwrap();
        assert_eq!(period.name, "Tonight");
        assert_eq!(period.temperature, 28);
        assert_eq!(period.temperature_unit, "F");
    }
}
