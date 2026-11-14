//! Creation-time query bounds for payment record list surfaces.
//!
//! Finance consoles query payment records and refunds by creation window. The
//! SDKWork list standard names those bounds `created_after` (inclusive) and
//! `created_before` (exclusive) and types them as date-time strings.
//!
//! The raw query values must be validated here, before they reach SQL: binding
//! an unparsable string into a `timestamptz` comparison fails inside the storage
//! layer and surfaces as an opaque `500` instead of telling the operator that a
//! date filter is wrong. Normalizing to canonical RFC 3339 UTC also removes the
//! ambiguity of client offsets (`+08:00`) and of zone-less input.

use chrono::{DateTime, SecondsFormat, Utc};

/// Normalized creation-time window for one list query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreatedAtBounds {
    /// Inclusive lower bound (`created_after`), canonical RFC 3339 UTC.
    pub from_inclusive: Option<String>,
    /// Exclusive upper bound (`created_before`), canonical RFC 3339 UTC.
    pub before_exclusive: Option<String>,
}

/// Parses and normalizes `created_after` / `created_before` query values.
///
/// Returns a client-facing validation message (mapped to HTTP 400) when a bound
/// is malformed or when the window is inverted or empty.
pub fn parse_created_at_bounds(
    created_after: Option<&str>,
    created_before: Option<&str>,
) -> Result<CreatedAtBounds, String> {
    let from = parse_bound(created_after, "created_after")?;
    let before = parse_bound(created_before, "created_before")?;
    if let (Some(from), Some(before)) = (from, before) {
        if from >= before {
            return Err("created_after must be earlier than created_before".to_owned());
        }
        return Ok(CreatedAtBounds {
            from_inclusive: Some(format_bound(from)),
            before_exclusive: Some(format_bound(before)),
        });
    }
    Ok(CreatedAtBounds {
        from_inclusive: from.map(format_bound),
        before_exclusive: before.map(format_bound),
    })
}

fn parse_bound(raw: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>, String> {
    let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| Some(parsed.with_timezone(&Utc)))
        .map_err(|_| {
            format!(
                "{field} must be an RFC 3339 date-time, for example 2026-01-31T00:00:00Z"
            )
        })
}

fn format_bound(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// Trims an optional list-filter value and drops blank values.
///
/// An empty filter must behave exactly like an absent filter; otherwise a
/// console that submits an untouched text input narrows the result set to rows
/// whose column equals the empty string.
pub fn normalize_filter_value(raw: Option<String>) -> Option<String> {
    raw.map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{parse_created_at_bounds, CreatedAtBounds};

    #[test]
    fn absent_bounds_produce_an_unbounded_window() {
        let bounds = parse_created_at_bounds(None, None).expect("unbounded window is valid");
        assert_eq!(bounds, CreatedAtBounds::default());
    }

    #[test]
    fn blank_bounds_are_treated_as_absent() {
        let bounds = parse_created_at_bounds(Some("   "), Some("")).expect("blank means absent");
        assert_eq!(bounds, CreatedAtBounds::default());
    }

    #[test]
    fn offsets_are_normalized_to_utc() {
        let bounds = parse_created_at_bounds(Some("2026-03-01T08:00:00+08:00"), None)
            .expect("offset bound is valid");
        assert_eq!(bounds.from_inclusive.as_deref(), Some("2026-03-01T00:00:00Z"));
        assert_eq!(bounds.before_exclusive, None);
    }

    #[test]
    fn forward_window_keeps_both_bounds() {
        let bounds = parse_created_at_bounds(
            Some("2026-03-01T00:00:00Z"),
            Some("2026-03-02T00:00:00Z"),
        )
        .expect("forward window is valid");
        assert_eq!(bounds.from_inclusive.as_deref(), Some("2026-03-01T00:00:00Z"));
        assert_eq!(
            bounds.before_exclusive.as_deref(),
            Some("2026-03-02T00:00:00Z")
        );
    }

    #[test]
    fn malformed_bound_names_the_offending_parameter() {
        let error = parse_created_at_bounds(Some("yesterday"), None)
            .expect_err("free text is not a date-time");
        assert!(error.contains("created_after"), "{error}");
        assert!(error.contains("RFC 3339"), "{error}");
    }

    #[test]
    fn inverted_window_is_rejected() {
        let error = parse_created_at_bounds(
            Some("2026-03-02T00:00:00Z"),
            Some("2026-03-01T00:00:00Z"),
        )
        .expect_err("inverted window is not a valid query");
        assert!(error.contains("earlier"), "{error}");
    }

    #[test]
    fn empty_window_is_rejected() {
        let error = parse_created_at_bounds(
            Some("2026-03-01T00:00:00Z"),
            Some("2026-03-01T00:00:00Z"),
        )
        .expect_err("an empty window can never match a record");
        assert!(error.contains("earlier"), "{error}");
    }
}
