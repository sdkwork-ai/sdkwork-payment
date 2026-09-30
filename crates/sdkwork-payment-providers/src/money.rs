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

pub fn minor_to_decimal_string(amount_minor: i64) -> String {
    let sign = if amount_minor < 0 { "-" } else { "" };
    let absolute = amount_minor.abs();
    format!("{sign}{}.{:02}", absolute / 100, absolute % 100)
}

/// ISO 4217 currencies whose smallest unit is not 1/100 (zero-decimal and
/// three-decimal currencies). The payment storage is `NUMERIC(18,2)`, so
/// charging such a currency would silently misplace the decimal point; these
/// currencies are rejected fail-closed at the pay boundary instead.
const NON_TWO_DECIMAL_CURRENCIES: &[&str] = &[
    // Zero-decimal
    "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "PYG", "RWF", "UGX", "UYI", "VUV",
    "VND", "XAF", "XOF", "XPF",
    // Three-decimal
    "BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND",
];

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

/// Rejects currencies whose minor unit is not 1/100, which the `NUMERIC(18,2)`
/// storage cannot represent faithfully.
///
/// # Errors
/// Validation error naming the unsupported currency.
pub fn ensure_currency_supported(code: &str) -> Result<(), CommerceServiceError> {
    let upper = normalized_payment_currency(code)?;
    if NON_TWO_DECIMAL_CURRENCIES.contains(&upper.as_str()) {
        return Err(CommerceServiceError::validation(format!(
            "currency {upper} does not use two decimal places and is not supported"
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
}
