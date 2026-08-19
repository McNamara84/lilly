use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use serde::Serializer;

pub fn serialize_utc_naive<S>(value: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(
        &DateTime::<Utc>::from_naive_utc_and_offset(*value, Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true),
    )
}

#[allow(clippy::ref_option)] // serde's serialize_with API passes the field by reference.
pub fn serialize_optional_utc_naive<S>(
    value: &Option<NaiveDateTime>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(value) => serializer.serialize_some(
            &DateTime::<Utc>::from_naive_utc_and_offset(*value, Utc)
                .to_rfc3339_opts(SecondsFormat::AutoSi, true),
        ),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde::Serialize;

    #[derive(Serialize)]
    struct TimestampFixture {
        #[serde(serialize_with = "super::serialize_utc_naive")]
        timestamp: NaiveDateTime,
        #[serde(serialize_with = "super::serialize_optional_utc_naive")]
        optional_timestamp: Option<NaiveDateTime>,
    }

    use super::NaiveDateTime;

    #[test]
    fn serializes_naive_database_timestamps_as_utc() {
        let timestamp = NaiveDate::from_ymd_opt(2026, 8, 24)
            .unwrap()
            .and_hms_micro_opt(8, 0, 0, 123_000)
            .unwrap();
        let json = serde_json::to_value(TimestampFixture {
            timestamp,
            optional_timestamp: Some(timestamp),
        })
        .unwrap();

        assert_eq!(json["timestamp"], "2026-08-24T08:00:00.123Z");
        assert_eq!(json["optional_timestamp"], "2026-08-24T08:00:00.123Z");
    }

    #[test]
    fn serializes_missing_optional_timestamp_as_null() {
        let timestamp = NaiveDate::from_ymd_opt(2026, 8, 24)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap();
        let json = serde_json::to_value(TimestampFixture {
            timestamp,
            optional_timestamp: None,
        })
        .unwrap();

        assert!(json["optional_timestamp"].is_null());
    }
}
