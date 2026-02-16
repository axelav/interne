use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum Interval {
    #[serde(rename = "hours")]
    #[sqlx(rename = "hours")]
    Hours,
    #[serde(rename = "days")]
    #[sqlx(rename = "days")]
    Days,
    #[serde(rename = "weeks")]
    #[sqlx(rename = "weeks")]
    Weeks,
    #[serde(rename = "months")]
    #[sqlx(rename = "months")]
    Months,
    #[serde(rename = "years")]
    #[sqlx(rename = "years")]
    Years,
}

impl std::fmt::Display for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Interval::Hours => write!(f, "hours"),
            Interval::Days => write!(f, "days"),
            Interval::Weeks => write!(f, "weeks"),
            Interval::Months => write!(f, "months"),
            Interval::Years => write!(f, "years"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Entry {
    pub id: String,
    pub user_id: String,
    pub collection_id: Option<String>,
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub duration: i64,
    pub interval: Interval,
    pub dismissed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_serde_roundtrip() {
        let variants = vec![
            (Interval::Hours, "\"hours\""),
            (Interval::Days, "\"days\""),
            (Interval::Weeks, "\"weeks\""),
            (Interval::Months, "\"months\""),
            (Interval::Years, "\"years\""),
        ];
        for (variant, expected_json) in variants {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected_json);
            let deserialized: Interval = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, variant);
        }
    }

    #[test]
    fn interval_display() {
        assert_eq!(Interval::Hours.to_string(), "hours");
        assert_eq!(Interval::Days.to_string(), "days");
        assert_eq!(Interval::Weeks.to_string(), "weeks");
        assert_eq!(Interval::Months.to_string(), "months");
        assert_eq!(Interval::Years.to_string(), "years");
    }
}
