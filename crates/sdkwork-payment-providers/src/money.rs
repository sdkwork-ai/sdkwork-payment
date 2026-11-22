use sdkwork_contract_service::{CommerceMoney, CommerceServiceError};

pub fn money_to_minor(amount: &CommerceMoney) -> Result<i64, CommerceServiceError> {
    let value = amount.as_str().trim();
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return Err(CommerceServiceError::validation(
            "money amount must be a non-negative integer smallest-unit amount",
        ));
    }
    value
        .parse()
        .map_err(|_| CommerceServiceError::validation("money amount overflow"))
}

/// Validates a positive smallest-unit amount (a payable amount of zero cannot
/// produce a PSP charge and would corrupt reservation math downstream).
pub fn require_positive_minor(
    amount: &CommerceMoney,
    field: &str,
) -> Result<i64, CommerceServiceError> {
    let minor = money_to_minor(amount)?;
    if minor <= 0 {
        return Err(CommerceServiceError::validation(format!(
            "{field} must be greater than zero"
        )));
    }
    Ok(minor)
}

/// ISO 4217 exponents that are not the default 2, as the single source of truth
/// for how many decimal places a currency's minor unit has.
///
/// Formatting and validation both read this table, so a currency cannot be
/// formatted with one exponent and validated against another.
const CURRENCY_EXPONENTS: &[(&str, u32)] = &[
    // Zero-decimal
    ("BIF", 0), ("CLP", 0), ("DJF", 0), ("GNF", 0), ("ISK", 0), ("JPY", 0),
    ("KMF", 0), ("KRW", 0), ("PYG", 0), ("RWF", 0), ("UGX", 0), ("UYI", 0),
    ("VUV", 0), ("VND", 0), ("XAF", 0), ("XOF", 0), ("XPF", 0),
    // Three-decimal
    ("BHD", 3), ("IQD", 3), ("JOD", 3), ("KWD", 3), ("LYD", 3), ("OMR", 3), ("TND", 3),
];

/// Default ISO 4217 exponent for currencies not in [`CURRENCY_EXPONENTS`].
const DEFAULT_CURRENCY_EXPONENT: u32 = 2;

/// Maximum exponent this code base formats, bounded by the storage scale.
pub const MAX_CURRENCY_EXPONENT: u32 = 3;

/// Returns the ISO 4217 exponent of a currency (2 for the common case).
#[must_use]
pub fn currency_exponent(code: &str) -> u32 {
    let upper = code.trim().to_ascii_uppercase();
    CURRENCY_EXPONENTS
        .iter()
        .find(|(candidate, _)| *candidate == upper)
        .map(|(_, exponent)| *exponent)
        .unwrap_or(DEFAULT_CURRENCY_EXPONENT)
}

pub fn minor_to_decimal_string(amount_minor: i64) -> String {
    minor_to_decimal_string_for(amount_minor, DEFAULT_CURRENCY_EXPONENT)
}

/// Formats a minor-unit amount using a currency exponent.
///
/// The integer form is exact: no float is involved, so a zero- or three-decimal
/// currency is formatted at its own scale instead of being divided by 100.
#[must_use]
pub fn minor_to_decimal_string_for(amount_minor: i64, exponent: u32) -> String {
    let sign = if amount_minor < 0 { "-" } else { "" };
    let absolute = amount_minor.unsigned_abs();
    let exponent = exponent.min(MAX_CURRENCY_EXPONENT);
    let divisor = 10_u64.pow(exponent);
    let whole = absolute / divisor;
    if exponent == 0 {
        return format!("{sign}{whole}");
    }
    format!("{sign}{whole}.{:0width$}", absolute % divisor, width = exponent as usize)
}

/// Normalizes an order currency code to upper-case ISO-4217 alpha-3 form.
///
/// # Errors
/// Validation error when the code is not exactly three ASCII letters.
pub fn normalized_payment_currency(code: &str) -> Result<String, CommerceServiceError> {
    let trimmed = code.trim();
    if trimmed.len() != 3 || !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(CommerceServiceError::validation(
            "currency code must be an ISO-4217 alpha-3 value",
        ));
    }
    Ok(trimmed.to_ascii_uppercase())
}

/// Rejects currencies whose exponent is not 2.
///
/// Payment storage is `NUMERIC(18,2)` and the settlement path parses two-decimal
/// amounts, so a zero- or three-decimal currency would silently misplace the
/// decimal point (JPY 10.00 charged as ¥1000). The check is derived from
/// [`currency_exponent`] so lifting it later only requires migrating storage,
/// not hunting a second currency list.
///
/// # Errors
/// Validation error naming the unsupported currency.
pub fn ensure_currency_supported(code: &str) -> Result<(), CommerceServiceError> {
    let upper = normalized_payment_currency(code)?;
    let exponent = currency_exponent(&upper);
    if exponent != DEFAULT_CURRENCY_EXPONENT {
        return Err(CommerceServiceError::validation(format!(
            "currency {upper} uses {exponent} decimal places and is not supported"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_minor_rejects_zero_and_non_numeric() {
        // Amounts travel as digit-only smallest-unit strings ("100" = 1.00).
        let positive = CommerceMoney::new("100").expect("digits amount");
        assert!(require_positive_minor(&positive, "amount").is_ok());
        let zero = CommerceMoney::new("0").expect("zero amount");
        assert!(require_positive_minor(&zero, "amount").is_err());
        // CommerceMoney itself rejects negatives at construction; the
        // positivity guard is the second line of defense.
        assert!(CommerceMoney::new("-5").is_err());
        // Decimal-form amounts are constructible but not minor-unit strings.
        let decimal = CommerceMoney::new("1.5").expect("decimal amount");
        assert!(require_positive_minor(&decimal, "amount").is_err());
    }

    #[test]
    fn non_two_decimal_currencies_are_rejected() {
        assert!(ensure_currency_supported("CNY").is_ok());
        assert!(ensure_currency_supported("usd").is_ok());
        assert!(ensure_currency_supported("JPY").is_err());
        assert!(ensure_currency_supported("kwd").is_err());
        assert!(ensure_currency_supported("EU").is_err());
        assert!(ensure_currency_supported("EURO").is_err());
        assert!(ensure_currency_supported("").is_err());
    }

    #[test]
    fn the_exponent_table_is_the_single_source_of_truth() {
        // Formatting and validation must agree on the exponent.
        assert_eq!(currency_exponent("CNY"), 2);
        assert_eq!(currency_exponent("usd"), 2);
        assert_eq!(currency_exponent("jpy"), 0);
        assert_eq!(currency_exponent("KRW"), 0);
        assert_eq!(currency_exponent("kwd"), 3);
        // Unknown-but-well-formed codes fall back to the ISO default.
        assert_eq!(currency_exponent("XYZ"), 2);
        for code in ["JPY", "KRW", "BHD", "KWD"] {
            assert_ne!(
                currency_exponent(code),
                2,
                "{code} must not be treated as two-decimal"
            );
            assert!(ensure_currency_supported(code).is_err());
        }
    }

    #[test]
    fn minor_units_format_at_the_currency_scale_exactly() {
        assert_eq!(minor_to_decimal_string(1234), "12.34");
        assert_eq!(minor_to_decimal_string_for(1234, 2), "12.34");
        // Zero-decimal: the integer IS the amount (no 100x division).
        assert_eq!(minor_to_decimal_string_for(1234, 0), "1234");
        // Three-decimal.
        assert_eq!(minor_to_decimal_string_for(1234, 3), "1.234");
        assert_eq!(minor_to_decimal_string_for(5, 3), "0.005");
        assert_eq!(minor_to_decimal_string_for(0, 2), "0.00");
        assert_eq!(minor_to_decimal_string_for(7, 0), "7");
        // Negative amounts survive formatting (the pay boundary still rejects
        // them before this point).
        assert_eq!(minor_to_decimal_string_for(-1234, 2), "-12.34");
    }
}
