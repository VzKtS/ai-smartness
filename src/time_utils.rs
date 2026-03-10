use chrono::{DateTime, Utc};

/// Retourne le timestamp courant en UTC
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// Formate un timestamp ISO 8601 pour SQLite
pub fn to_sqlite(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

/// Parse un timestamp ISO 8601 depuis SQLite
pub fn from_sqlite(s: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    s.parse::<DateTime<Utc>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let dt = now();
        let s = to_sqlite(&dt);
        let parsed = from_sqlite(&s).unwrap();
        assert_eq!(dt.timestamp(), parsed.timestamp());
    }
}
