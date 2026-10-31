use chrono::NaiveDate;
use lilly_importer_core::AdapterError;

/// Distinguish an explicitly unknown publication date from malformed source data.
pub(crate) fn parse(
    value: Option<&str>,
    parser: fn(&str) -> Option<NaiveDate>,
) -> Result<Option<NaiveDate>, AdapterError> {
    let value = value.unwrap_or_default().trim();
    if matches!(
        value.to_lowercase().as_str(),
        "" | "?" | "-" | "–" | "—" | "unbekannt"
    ) || value.chars().all(|character| character == '?')
    {
        return Ok(None);
    }
    parser(value)
        .map(Some)
        .ok_or_else(|| AdapterError::Parse(format!("Invalid publication date: {value:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numeric(value: &str) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(value, "%d.%m.%Y").ok()
    }

    #[test]
    fn missing_and_explicitly_unknown_dates_remain_optional() {
        for value in [
            None,
            Some(""),
            Some(" ? "),
            Some("??????"),
            Some("-"),
            Some("–"),
            Some("—"),
            Some("Unbekannt"),
        ] {
            assert_eq!(parse(value, numeric).unwrap(), None);
        }
    }

    #[test]
    fn validates_present_dates_including_leap_days() {
        assert_eq!(
            parse(Some(" 29.02.2024 "), numeric).unwrap(),
            NaiveDate::from_ymd_opt(2024, 2, 29)
        );
        for value in [
            "29.02.1982", // Observed on the live source for John Sinclair 191.
            "29.02.2025",
            "31.04.2026",
            "2026",
            "invalid",
            "??.02.2026",
            "01.01.2026 trailing",
        ] {
            let error = parse(Some(value), numeric).unwrap_err();
            assert!(error.to_string().contains("Invalid publication date"));
            assert!(error.to_string().contains(value));
        }
    }
}
