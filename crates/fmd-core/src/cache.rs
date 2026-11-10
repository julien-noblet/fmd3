use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TtlUnit {
    Minutes,
    Hours,
    Days,
    Weeks,
}

impl Default for TtlUnit {
    fn default() -> Self {
        TtlUnit::Hours
    }
}

impl TtlUnit {
    pub fn to_duration(&self, value: u32) -> Duration {
        match self {
            TtlUnit::Minutes => Duration::minutes(value as i64),
            TtlUnit::Hours => Duration::hours(value as i64),
            TtlUnit::Days => Duration::days(value as i64),
            TtlUnit::Weeks => Duration::weeks(value as i64),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TtlUnit::Minutes => "minutes",
            TtlUnit::Hours => "hours",
            TtlUnit::Days => "days",
            TtlUnit::Weeks => "weeks",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "m" | "min" | "mins" | "minute" | "minutes" => TtlUnit::Minutes,
            "h" | "hr" | "hrs" | "hour" | "hours" => TtlUnit::Hours,
            "d" | "day" | "days" => TtlUnit::Days,
            "w" | "week" | "weeks" => TtlUnit::Weeks,
            _ => TtlUnit::Hours,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleCachePolicy {
    pub module_id: String, // "*" for global fallback, or specific module ID
    pub ttl_value: u32,
    pub ttl_unit: TtlUnit,
    pub min_delay_ms: u64, // anti-ban delay between network requests to this module
    pub updated_at: DateTime<Utc>,
}

impl Default for ModuleCachePolicy {
    fn default() -> Self {
        Self {
            module_id: "*".to_string(),
            ttl_value: 24,
            ttl_unit: TtlUnit::Hours,
            min_delay_ms: 1000, // 1s default rate limiting
            updated_at: Utc::now(),
        }
    }
}

impl ModuleCachePolicy {
    pub fn calculate_expiration(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        from + self.ttl_unit.to_duration(self.ttl_value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MangaCacheItem {
    pub manga_id: String, // composite id: "{module_id}:{url}"
    pub module_id: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub metadata_json: String,
    pub chapters_json: String,
    pub cached_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl MangaCacheItem {
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn remaining_seconds(&self) -> i64 {
        let diff = (self.expires_at - Utc::now()).num_seconds();
        if diff < 0 { 0 } else { diff }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ttl_duration() {
        let now = Utc::now();
        let policy_min = ModuleCachePolicy {
            module_id: "test".into(),
            ttl_value: 30,
            ttl_unit: TtlUnit::Minutes,
            min_delay_ms: 500,
            updated_at: now,
        };
        let exp_min = policy_min.calculate_expiration(now);
        assert_eq!((exp_min - now).num_minutes(), 30);

        let policy_days = ModuleCachePolicy {
            module_id: "test".into(),
            ttl_value: 2,
            ttl_unit: TtlUnit::Days,
            min_delay_ms: 500,
            updated_at: now,
        };
        let exp_days = policy_days.calculate_expiration(now);
        assert_eq!((exp_days - now).num_days(), 2);
    }
}
