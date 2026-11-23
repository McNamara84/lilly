use std::collections::{HashMap, HashSet};

use crate::error::AppError;
use crate::models::collection::{
    normalize_collection_note, normalize_edition_label, validate_status_condition,
};
use crate::models::collection_transfer::{
    CollectionDocument, FORMAT_NAME, FORMAT_VERSION, ParsedRow, TransferEntry,
};

const CSV_FIELDS: [&str; 14] = [
    "format_version",
    "copy_key",
    "series_slug",
    "issue_number",
    "copy_number",
    "status",
    "source_key",
    "source_record_id",
    "condition_grade",
    "edition_label",
    "notes",
    "series_name",
    "title",
    "text_encoding",
];
const REQUIRED_FIELDS: [&str; 6] = [
    "format_version",
    "copy_key",
    "series_slug",
    "issue_number",
    "copy_number",
    "status",
];

fn invalid_file(code: &str) -> AppError {
    AppError::BadRequestWithCode {
        message: "Use a valid version 1 LILLY CSV or JSON collection file".to_string(),
        code: code.to_string(),
    }
}

pub fn parse_document(
    bytes: &[u8],
    format: &str,
    max_bytes: usize,
    max_rows: usize,
) -> Result<Vec<ParsedRow>, AppError> {
    if bytes.len() > max_bytes {
        return Err(AppError::PayloadTooLarge(format!(
            "Import exceeds {max_bytes} bytes"
        )));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid_file("invalid_utf8"))?
        .trim_start_matches('\u{feff}');
    let values = match format {
        "json" => {
            let document: CollectionDocument =
                serde_json::from_str(text).map_err(|_| invalid_file("invalid_document"))?;
            if document.format != FORMAT_NAME || document.format_version != FORMAT_VERSION {
                return Err(invalid_file("unsupported_format_version"));
            }
            document.entries
        }
        "csv" => parse_csv(text)?,
        _ => return Err(invalid_file("unsupported_format")),
    };
    if values.len() > max_rows {
        return Err(AppError::PayloadTooLarge(format!(
            "Import exceeds {max_rows} records"
        )));
    }
    let mut rows: Vec<ParsedRow> = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let parsed = serde_json::from_value::<TransferEntry>(value)
                .map_err(|_| "invalid_fields".to_string())
                .and_then(normalize_entry);
            let (entry, code) = match parsed {
                Ok(entry) => (Some(entry), None),
                Err(code) => (None, Some(code)),
            };
            ParsedRow {
                row_number: u32::try_from(index + 1).unwrap_or(u32::MAX),
                entry,
                code,
            }
        })
        .collect();
    classify_duplicate_keys(&mut rows);
    Ok(rows)
}

fn normalize_entry(mut entry: TransferEntry) -> Result<TransferEntry, String> {
    entry.series_slug = entry.series_slug.to_ascii_lowercase();
    if entry.copy_key.is_empty()
        || entry.copy_key.len() > 128
        || !entry
            .copy_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
    {
        return Err("invalid_copy_key".to_string());
    }
    if entry.series_slug.is_empty()
        || !entry
            .series_slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || entry.series_slug.len() > 100
        || entry.issue_number == 0
        || entry.copy_number == 0
    {
        return Err("invalid_issue_identity".to_string());
    }
    validate_status_condition(&entry.status, entry.condition_grade.as_deref())
        .map_err(|_| "invalid_status_condition".to_string())?;
    entry.notes = normalize_collection_note(entry.notes.as_deref())
        .map_err(|_| "invalid_notes".to_string())?
        .map(str::to_string);
    entry.edition_label = normalize_edition_label(entry.edition_label.as_deref())
        .map_err(|_| "invalid_edition".to_string())?;
    if entry.source_key.is_some() != entry.source_record_id.is_some()
        || entry.source_key.as_deref().is_some_and(str::is_empty)
        || entry.source_record_id.as_deref().is_some_and(str::is_empty)
    {
        return Err("invalid_source_identity".to_string());
    }
    Ok(entry)
}

fn classify_duplicate_keys(rows: &mut [ParsedRow]) {
    let mut first: HashMap<String, TransferEntry> = HashMap::new();
    let mut conflicts = HashSet::new();
    for row in rows.iter() {
        if let Some(entry) = &row.entry {
            if let Some(previous) = first.get(&entry.copy_key) {
                if !same_values(previous, entry) {
                    conflicts.insert(entry.copy_key.clone());
                }
            } else {
                first.insert(entry.copy_key.clone(), entry.clone());
            }
        }
    }
    let mut seen = HashSet::new();
    for row in rows {
        if let Some(entry) = &row.entry {
            if conflicts.contains(&entry.copy_key) {
                row.code = Some("duplicate_key_conflict".to_string());
            } else if !seen.insert(entry.copy_key.clone()) {
                row.code = Some("duplicate_file_row".to_string());
            }
        }
    }
}

pub fn same_values(left: &TransferEntry, right: &TransferEntry) -> bool {
    left.series_slug == right.series_slug
        && left.issue_number == right.issue_number
        && left.copy_number == right.copy_number
        && left.status == right.status
        && left.condition_grade == right.condition_grade
        && left.notes == right.notes
        && left.edition_label == right.edition_label
}

// Quotes alone do not prevent spreadsheet formulas. An explicit encoding flag
// makes escaping reversible without changing hand-written plain-text imports.
fn escape_cell(value: &str) -> String {
    if value
        .chars()
        .next()
        .is_some_and(|c| "=+-@'".contains(c) || c.is_ascii_whitespace())
    {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

fn unescape_cell(value: &str, encoded: bool) -> String {
    if encoded
        && let Some(rest) = value.strip_prefix('\'')
        && rest
            .chars()
            .next()
            .is_some_and(|c| "=+-@'".contains(c) || c.is_ascii_whitespace())
    {
        return rest.to_string();
    }
    value.to_string()
}

fn validate_csv_syntax(text: &str) -> Result<(), AppError> {
    let (mut quoted, mut closed, mut start) = (false, false, true);
    let mut bytes = text.bytes().peekable();
    while let Some(byte) = bytes.next() {
        if quoted {
            if byte == b'"' {
                if bytes.peek() == Some(&b'"') {
                    bytes.next();
                } else {
                    quoted = false;
                    closed = true;
                }
            }
        } else if b";\r\n".contains(&byte) {
            start = true;
            closed = false;
        } else if byte == b'"' && start {
            quoted = true;
            start = false;
        } else if closed || byte == b'"' {
            return Err(invalid_file("invalid_csv_structure"));
        } else {
            start = false;
        }
    }
    if quoted {
        return Err(invalid_file("invalid_csv_structure"));
    }
    Ok(())
}

fn parse_csv(text: &str) -> Result<Vec<serde_json::Value>, AppError> {
    validate_csv_syntax(text)?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|_| invalid_file("invalid_csv_header"))?
        .clone();
    let unique: HashSet<&str> = headers.iter().collect();
    if unique.len() != headers.len()
        || headers.iter().any(|h| !CSV_FIELDS.contains(&h))
        || REQUIRED_FIELDS.iter().any(|field| !unique.contains(field))
    {
        return Err(invalid_file("invalid_csv_header"));
    }
    let mut result = Vec::new();
    for row in reader.records() {
        let row = row.map_err(|_| invalid_file("invalid_csv_structure"))?;
        let fields: HashMap<&str, &str> = headers.iter().zip(row.iter()).collect();
        if fields.get("format_version") != Some(&"1") {
            return Err(invalid_file("unsupported_format_version"));
        }
        let encoded = match fields.get("text_encoding").copied().unwrap_or("") {
            "" | "plain" => false,
            "spreadsheet-safe-v1" => true,
            _ => return Err(invalid_file("unsupported_text_encoding")),
        };
        let mut value = serde_json::Map::new();
        for (name, cell) in fields {
            if ["format_version", "text_encoding"].contains(&name) {
                continue;
            }
            let cell = unescape_cell(cell, encoded);
            let field = if ["issue_number", "copy_number"].contains(&name) {
                cell.parse::<u32>()
                    .map_or(serde_json::Value::Null, serde_json::Value::from)
            } else if cell.is_empty() && !REQUIRED_FIELDS.contains(&name) {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(cell)
            };
            value.insert(name.to_string(), field);
        }
        result.push(serde_json::Value::Object(value));
    }
    Ok(result)
}

pub fn encode_document(
    entries: &[TransferEntry],
    format: &str,
    exported_at: &str,
) -> Result<Vec<u8>, AppError> {
    if format == "json" {
        return serde_json::to_vec_pretty(&CollectionDocument {
            format: FORMAT_NAME.to_string(),
            format_version: FORMAT_VERSION,
            exported_at: Some(exported_at.to_string()),
            entries: entries
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()
                .map_err(|e| AppError::InternalError(e.into()))?,
        })
        .map_err(|e| AppError::InternalError(e.into()));
    }
    if format != "csv" {
        return Err(invalid_file("unsupported_format"));
    }
    let mut writer = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_writer(Vec::new());
    writer
        .write_record(CSV_FIELDS)
        .map_err(|e| AppError::InternalError(e.into()))?;
    for entry in entries {
        let value = serde_json::to_value(entry).map_err(|e| AppError::InternalError(e.into()))?;
        let cells: Vec<String> = CSV_FIELDS
            .iter()
            .map(|name| match *name {
                "format_version" => "1".to_string(),
                "text_encoding" => "spreadsheet-safe-v1".to_string(),
                "issue_number" => entry.issue_number.to_string(),
                "copy_number" => entry.copy_number.to_string(),
                _ => escape_cell(
                    value
                        .get(*name)
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(""),
                ),
            })
            .collect();
        writer
            .write_record(cells)
            .map_err(|e| AppError::InternalError(e.into()))?;
    }
    let mut output = b"\xef\xbb\xbf".to_vec();
    output.extend(
        writer
            .into_inner()
            .map_err(|e| AppError::InternalError(e.into()))?,
    );
    Ok(output)
}

pub fn template_entry() -> TransferEntry {
    TransferEntry {
        copy_key: "bestand-0001".to_string(),
        series_slug: "maddrax".to_string(),
        issue_number: 1,
        copy_number: 1,
        status: "owned".to_string(),
        condition_grade: Some("Z2".to_string()),
        edition_label: Some("Erstauflage".to_string()),
        notes: None,
        source_key: None,
        source_record_id: None,
        series_name: None,
        title: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8], format: &str) -> Result<Vec<ParsedRow>, AppError> {
        parse_document(bytes, format, 100_000, 100)
    }

    #[test]
    fn csv_and_json_round_trip_unicode_multiline_quotes_and_formulas() {
        for text in [
            "Grüße 🦇; \"Titel\"\nzweite Zeile",
            "=HYPERLINK(\"evil\")",
            "+1",
            "-2",
            "@SUM(A1)",
            "'original",
            "\t=formula",
            "  =formula",
            "plain",
        ] {
            let mut entry = template_entry();
            entry.notes = Some(text.to_string());
            // Notes use the same normalization as ordinary collection writes.
            let entry = normalize_entry(entry).unwrap();
            for format in ["csv", "json"] {
                let encoded =
                    encode_document(std::slice::from_ref(&entry), format, "2026-11-22T12:00:00Z")
                        .unwrap();
                let rows = parse(&encoded, format).unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].entry.as_ref(), Some(&entry));
                assert!(rows[0].code.is_none());
            }
        }
    }

    #[test]
    fn empty_collection_has_valid_headers_and_json_envelope() {
        for format in ["csv", "json"] {
            let encoded = encode_document(&[], format, "now").unwrap();
            assert!(parse(&encoded, format).unwrap().is_empty());
        }
    }

    #[test]
    fn json_accepts_omitted_and_null_exported_at() {
        let entry = template_entry();
        let mut document = serde_json::json!({
            "format": FORMAT_NAME,
            "format_version": FORMAT_VERSION,
            "entries": [&entry],
        });
        for exported_at in [None, Some(serde_json::Value::Null)] {
            if let Some(exported_at) = exported_at {
                document["exported_at"] = exported_at;
            }
            let rows = parse(document.to_string().as_bytes(), "json").unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].entry.as_ref(), Some(&entry));
            assert!(rows[0].code.is_none());
        }
    }

    #[test]
    fn csv_rejects_records_with_missing_or_extra_columns() {
        let header =
            "format_version;copy_key;series_slug;issue_number;copy_number;status;condition_grade";
        let valid = "1;first;maddrax;1;1;owned;Z2";
        for malformed in [
            "1;second;maddrax;2;1;owned",
            "1;second;maddrax;2;1;owned;Z2;unexpected",
        ] {
            let csv = format!("{header}\n{valid}\n{malformed}\n");
            assert!(
                matches!(
                    parse(csv.as_bytes(), "csv"),
                    Err(AppError::BadRequestWithCode { code, .. }) if code == "invalid_csv_structure"
                ),
                "{malformed}"
            );
        }

        let csv = format!("{header}\n{valid}\n1;second;maddrax;2;invalid;owned;Z2\n");
        let rows = parse(csv.as_bytes(), "csv").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].entry.is_some());
        assert_eq!(rows[1].code.as_deref(), Some("invalid_fields"));
    }

    #[test]
    fn malformed_rows_do_not_hide_valid_records() {
        let entry = serde_json::to_value(template_entry()).unwrap();
        let mut invalid = entry.clone();
        invalid["copy_number"] = serde_json::json!(0);
        let data = serde_json::json!({"format": FORMAT_NAME, "format_version": 1, "entries": [entry, invalid, {"user_id": 2}]});
        let rows = parse(data.to_string().as_bytes(), "json").unwrap();
        assert!(rows[0].entry.is_some());
        assert_eq!(rows[1].code.as_deref(), Some("invalid_issue_identity"));
        assert_eq!(rows[2].code.as_deref(), Some("invalid_fields"));
    }

    #[test]
    fn duplicate_conflicts_mark_every_occurrence_and_identical_rows_skip() {
        let entry = template_entry();
        let bytes = encode_document(&[entry.clone(), entry.clone()], "json", "now").unwrap();
        let rows = parse(&bytes, "json").unwrap();
        assert!(rows[0].code.is_none());
        assert_eq!(rows[1].code.as_deref(), Some("duplicate_file_row"));
        let mut changed = entry.clone();
        changed.notes = Some("different".to_string());
        let bytes = encode_document(&[entry, changed], "json", "now").unwrap();
        assert!(
            parse(&bytes, "json")
                .unwrap()
                .iter()
                .all(|r| r.code.as_deref() == Some("duplicate_key_conflict"))
        );
    }

    #[test]
    fn strict_header_structure_versions_and_limits_are_enforced() {
        for text in [
            "",
            "copy_key;copy_key\na;b",
            "format_version;user_id\n1;2",
            "\"unterminated",
            "a\"bad;header",
            "\"closed\"bad;header",
        ] {
            assert!(parse(text.as_bytes(), "csv").is_err(), "{text}");
        }
        for bytes in [
            b"not-json".as_slice(),
            b"{\"format\":\"lilly.collection\",\"format_version\":2,\"entries\":[]}",
            &[0xff],
        ] {
            assert!(parse(bytes, "json").is_err());
        }
        let bytes = encode_document(&[template_entry()], "json", "now").unwrap();
        assert!(parse_document(&bytes, "json", bytes.len() - 1, 1).is_err());
        assert!(parse_document(&bytes, "json", bytes.len(), 0).is_err());
        assert!(parse_document(&bytes, "json", bytes.len(), 1).is_ok());
        assert!(parse(&bytes, "xml").is_err());
        assert!(encode_document(&[], "xml", "now").is_err());
    }

    #[test]
    fn validation_uses_real_collection_rules_and_preserves_optional_wanted_condition() {
        let base = template_entry();
        for invalid in ["", "a/b", "a b", "ä", &"a".repeat(129)] {
            let mut entry = base.clone();
            entry.copy_key = invalid.to_string();
            assert_eq!(normalize_entry(entry).unwrap_err(), "invalid_copy_key");
        }
        for status in ["sale", "missing", "OWNED"] {
            let mut entry = base.clone();
            entry.status = status.to_string();
            assert_eq!(
                normalize_entry(entry).unwrap_err(),
                "invalid_status_condition"
            );
        }
        let mut entry = base.clone();
        entry.status = "wanted".to_string();
        entry.condition_grade = None;
        assert!(normalize_entry(entry).is_ok());
        let mut entry = base.clone();
        entry.source_key = Some("wiki".to_string());
        assert_eq!(
            normalize_entry(entry).unwrap_err(),
            "invalid_source_identity"
        );
        let mut entry = base.clone();
        entry.notes = Some("x".repeat(10_001));
        assert_eq!(normalize_entry(entry).unwrap_err(), "invalid_notes");
        let mut entry = base;
        entry.edition_label = Some("x".repeat(121));
        assert_eq!(normalize_entry(entry).unwrap_err(), "invalid_edition");
    }
}
