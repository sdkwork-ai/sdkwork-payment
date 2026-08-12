//! Provider-side payment operations (cancel, refund) invoked from app handlers.

use sdkwork_contract_service::CommerceServiceError;
use serde_json::json;

use crate::adapter::{
    normalize_provider_code, PaymentCancelPaymentIntentRequest, PaymentCreateRefundRequest,
    PaymentQueryRefundRequest,
};
use crate::error::ProviderError;
use crate::money::money_to_minor;
use crate::registry::PaymentProviderRegistry;
use sdkwork_contract_service::CommerceMoney;

pub async fn cancel_provider_payment(
    registry: &PaymentProviderRegistry,
    provider_code: &str,
    out_trade_no: &str,
    provider_transaction_id: Option<&str>,
) -> Result<(), CommerceServiceError> {
    let provider_code = normalize_provider_code(provider_code);
    if provider_code == "sandbox" || provider_code.is_empty() {
        return Ok(());
    }
    let adapter = registry.resolve(&provider_code).ok_or_else(|| {
        CommerceServiceError::provider_unavailable(format!(
            "payment provider {provider_code} is not configured"
        ))
    })?;
    let cancel_reference = match provider_code.as_str() {
        "stripe" => provider_transaction_id
            .filter(|value| value.starts_with("pi_"))
            .unwrap_or(out_trade_no),
        _ => out_trade_no,
    };
    let idempotency_key = provider_operation_idempotency_key(
        "cancel",
        &provider_code,
        &[out_trade_no, cancel_reference],
    );
    match adapter
        .cancel_payment_intent(PaymentCancelPaymentIntentRequest {
            payment_intent_id: Some(cancel_reference.to_owned()),
            reason: None,
            metadata: json!({ "idempotency_key": idempotency_key }),
        })
        .await
    {
        Ok(_) => Ok(()),
        Err(error) if cancel_error_means_trade_is_absent(&provider_code, &error) => Ok(()),
        Err(_) => Err(CommerceServiceError::provider_unavailable(
            "payment provider refund query did not produce a conclusive result",
        )),
    }
}

fn cancel_error_means_trade_is_absent(provider_code: &str, error: &ProviderError) -> bool {
    let ProviderError::Transport { message, .. } = error else {
        return false;
    };
    let message = message.to_ascii_uppercase();
    match provider_code {
        "wechat_pay" => {
            (message.contains("HTTP 404") && message.contains("ORDER_NOT_EXIST"))
                || (message.contains("HTTP 400") && message.contains("ORDER_CLOSED"))
        }
        "alipay" => message.contains("ACQ.TRADE_NOT_EXIST"),
        "stripe" => {
            (message.contains("HTTP 404")
                && (message.contains("RESOURCE_MISSING")
                    || message.contains("NO SUCH PAYMENT_INTENT")))
                || (message.contains("PAYMENT_INTENT_UNEXPECTED_STATE")
                    && message.contains("STATUS OF CANCELED"))
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRefundSubmissionState {
    Processing,
    Succeeded,
    Failed,
}

pub async fn create_provider_refund(
    registry: &PaymentProviderRegistry,
    provider_code: &str,
    out_trade_no: &str,
    provider_transaction_id: Option<&str>,
    refund_no: &str,
    refund_amount: &CommerceMoney,
    total_amount: &CommerceMoney,
    reason: Option<String>,
) -> Result<ProviderRefundSubmissionState, CommerceServiceError> {
    let provider_code = normalize_provider_code(provider_code);
    if provider_code == "sandbox" || provider_code.is_empty() {
        return Ok(ProviderRefundSubmissionState::Succeeded);
    }
    let adapter = registry.resolve(&provider_code).ok_or_else(|| {
        CommerceServiceError::provider_unavailable(format!(
            "payment provider {provider_code} is not configured"
        ))
    })?;
    let amount_minor = money_to_minor(refund_amount)?;
    let total_amount_minor = money_to_minor(total_amount)?;
    let payment_reference =
        provider_refund_reference(&provider_code, out_trade_no, provider_transaction_id)?;
    let idempotency_key =
        provider_operation_idempotency_key("refund", &provider_code, &[out_trade_no, refund_no]);
    let outcome = adapter
        .create_refund(PaymentCreateRefundRequest {
            payment_intent_id: Some(payment_reference.to_owned()),
            refund_no: Some(refund_no.to_owned()),
            amount_minor: Some(amount_minor),
            reason,
            metadata: json!({
                "idempotency_key": idempotency_key,
                "total_amount_minor": total_amount_minor,
            }),
        })
        .await?;
    Ok(provider_refund_submission_state(
        &provider_code,
        outcome.raw_status.as_deref(),
    ))
}

/// Normalized provider payment-intent query outcome used by the compensation
/// worker and reconciliation paths.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProviderPaymentQueryState {
    Pending,
    Succeeded,
    Failed,
    Canceled,
}

/// Queries the PSP for the current payment-intent state. Returns `None` when
/// the trade does not exist at the provider (order never reached the PSP or
/// was closed before submission).
pub async fn query_provider_payment_intent(
    registry: &PaymentProviderRegistry,
    provider_code: &str,
    out_trade_no: &str,
    provider_transaction_id: Option<&str>,
) -> Result<Option<ProviderPaymentQueryState>, CommerceServiceError> {
    let provider_code = normalize_provider_code(provider_code);
    if provider_code == "sandbox" || provider_code.is_empty() {
        return Ok(Some(ProviderPaymentQueryState::Succeeded));
    }
    let adapter = registry.resolve(&provider_code).ok_or_else(|| {
        CommerceServiceError::provider_unavailable(format!(
            "payment provider {provider_code} is not configured"
        ))
    })?;
    let query_reference = match provider_code.as_str() {
        // Stripe requires the native `pi_` resource id; the merchant order no
        // is only a fallback for pre-submission diagnostics.
        "stripe" => provider_transaction_id
            .filter(|value| value.starts_with("pi_"))
            .unwrap_or(out_trade_no),
        _ => out_trade_no,
    };
    let result = adapter
        .query_payment_intent(crate::adapter::PaymentQueryPaymentIntentRequest {
            payment_intent_id: Some(query_reference.to_owned()),
            metadata: json!({ "out_trade_no": out_trade_no }),
        })
        .await;
    match result {
        Ok(outcome) => Ok(Some(provider_payment_query_state(
            &provider_code,
            outcome.raw_status.as_deref(),
        ))),
        Err(error) if payment_query_error_means_not_found(&provider_code, &error) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn provider_payment_query_state(
    provider_code: &str,
    raw_status: Option<&str>,
) -> ProviderPaymentQueryState {
    let status = raw_status
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match provider_code {
        "stripe" => match status.as_str() {
            "succeeded" => ProviderPaymentQueryState::Succeeded,
            "canceled" | "cancelled" => ProviderPaymentQueryState::Canceled,
            "payment_failed" => ProviderPaymentQueryState::Failed,
            _ => ProviderPaymentQueryState::Pending,
        },
        "wechat_pay" => match status.as_str() {
            "success" => ProviderPaymentQueryState::Succeeded,
            "closed" | "revoked" | "payerror" => ProviderPaymentQueryState::Canceled,
            _ => ProviderPaymentQueryState::Pending,
        },
        "alipay" => match status.as_str() {
            "trade_success" | "trade_finished" => ProviderPaymentQueryState::Succeeded,
            "trade_closed" => ProviderPaymentQueryState::Canceled,
            _ => ProviderPaymentQueryState::Pending,
        },
        _ => ProviderPaymentQueryState::Pending,
    }
}

fn payment_query_error_means_not_found(provider_code: &str, error: &ProviderError) -> bool {
    let message = match error {
        ProviderError::InvalidResponse { message, .. }
        | ProviderError::Transport { message, .. } => message.to_ascii_uppercase(),
        _ => return false,
    };
    match provider_code {
        "stripe" => message.contains("NO SUCH PAYMENT_INTENT"),
        "wechat_pay" => {
            message.contains("HTTP 404")
                && (message.contains("ORDER_NOT_EXIST") || message.contains("RESOURCE_NOT_EXISTS"))
        }
        "alipay" => message.contains("ACQ.TRADE_NOT_EXIST"),
        _ => false,
    }
}

pub async fn query_provider_refund(
    registry: &PaymentProviderRegistry,
    provider_code: &str,
    out_trade_no: &str,
    provider_transaction_id: Option<&str>,
    refund_no: &str,
) -> Result<Option<ProviderRefundSubmissionState>, CommerceServiceError> {
    let provider_code = normalize_provider_code(provider_code);
    if provider_code == "sandbox" || provider_code.is_empty() {
        return Ok(Some(ProviderRefundSubmissionState::Succeeded));
    }
    let adapter = registry.resolve(&provider_code).ok_or_else(|| {
        CommerceServiceError::provider_unavailable(format!(
            "payment provider {provider_code} is not configured"
        ))
    })?;
    let refund_id = provider_transaction_id
        .filter(|value| provider_code == "stripe" && value.starts_with("re_"))
        .map(str::to_owned);
    let result = adapter
        .query_refund(PaymentQueryRefundRequest {
            refund_id,
            refund_no: Some(refund_no.to_owned()),
            metadata: json!({
                "out_trade_no": out_trade_no,
                "payment_intent_id": provider_transaction_id,
            }),
        })
        .await;
    match result {
        Ok(outcome) => Ok(Some(provider_refund_submission_state(
            &provider_code,
            outcome.raw_status.as_deref(),
        ))),
        Err(error) if refund_query_error_means_not_found(&provider_code, &error) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn provider_refund_submission_state(
    provider_code: &str,
    raw_status: Option<&str>,
) -> ProviderRefundSubmissionState {
    let status = raw_status
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match provider_code {
        "stripe" => match status.as_str() {
            "succeeded" => ProviderRefundSubmissionState::Succeeded,
            "failed" | "canceled" | "cancelled" => ProviderRefundSubmissionState::Failed,
            _ => ProviderRefundSubmissionState::Processing,
        },
        "wechat_pay" => match status.as_str() {
            "success" => ProviderRefundSubmissionState::Succeeded,
            "closed" | "abnormal" => ProviderRefundSubmissionState::Failed,
            _ => ProviderRefundSubmissionState::Processing,
        },
        "alipay" => match status.as_str() {
            "success" | "refund_success" | "trade_success" | "trade_finished" => {
                ProviderRefundSubmissionState::Succeeded
            }
            "failed" | "failure" | "trade_closed" => ProviderRefundSubmissionState::Failed,
            _ => ProviderRefundSubmissionState::Processing,
        },
        _ => ProviderRefundSubmissionState::Processing,
    }
}

fn refund_query_error_means_not_found(provider_code: &str, error: &ProviderError) -> bool {
    let message = match error {
        ProviderError::InvalidResponse { message, .. }
        | ProviderError::Transport { message, .. } => message.to_ascii_uppercase(),
        _ => return false,
    };
    match provider_code {
        "stripe" => message.contains("STRIPE REFUND WAS NOT FOUND"),
        "wechat_pay" => {
            message.contains("HTTP 404")
                && (message.contains("RESOURCE_NOT_EXISTS")
                    || message.contains("REFUND_NOT_EXIST")
                    || message.contains("REFUND_NOT_FOUND"))
        }
        "alipay" => {
            message.contains("ACQ.TRADE_NOT_EXIST")
                || message.contains("ACQ.REFUND_NOT_EXIST")
                || message.contains("ACQ.REFUND_RECORD_NOT_EXIST")
        }
        _ => false,
    }
}

fn provider_refund_reference<'a>(
    provider_code: &str,
    out_trade_no: &'a str,
    provider_transaction_id: Option<&'a str>,
) -> Result<&'a str, CommerceServiceError> {
    if provider_code == "stripe" {
        return provider_transaction_id
            .filter(|value| value.starts_with("pi_"))
            .ok_or_else(|| {
                CommerceServiceError::conflict(
                    "Stripe refund requires the original provider transaction id",
                )
            });
    }
    Ok(out_trade_no)
}

fn provider_operation_idempotency_key(
    operation: &str,
    provider_code: &str,
    identity_parts: &[&str],
) -> String {
    let mut fingerprint = format!(
        "payment-provider-operation:v1|{}:{}|{}:{}",
        operation.len(),
        operation,
        provider_code.len(),
        provider_code,
    );
    for part in identity_parts {
        fingerprint.push('|');
        fingerprint.push_str(&part.len().to_string());
        fingerprint.push(':');
        fingerprint.push_str(part);
    }
    format!(
        "sdkwork-{operation}-{}",
        sdkwork_utils_rust::crypto::sha256_hash(fingerprint.as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::PaymentAdapterOperation;

    #[test]
    fn provider_operation_keys_are_stable_and_operation_scoped() {
        let cancel = provider_operation_idempotency_key("cancel", "stripe", &["trade-1", "pi_1"]);
        assert_eq!(
            cancel,
            provider_operation_idempotency_key("cancel", "stripe", &["trade-1", "pi_1"])
        );
        assert_ne!(
            cancel,
            provider_operation_idempotency_key("refund", "stripe", &["trade-1", "pi_1"])
        );
        assert!(cancel.is_ascii());
        assert!(cancel.len() <= 255);
    }

    #[test]
    fn refund_reference_uses_stripe_native_id_and_other_provider_trade_number() {
        assert_eq!(
            provider_refund_reference("stripe", "trade-1", Some("pi_123"))
                .expect("Stripe native reference"),
            "pi_123"
        );
        assert!(provider_refund_reference("stripe", "trade-1", None).is_err());
        assert_eq!(
            provider_refund_reference("wechat_pay", "trade-1", Some("wx-1"))
                .expect("WeChat merchant reference"),
            "trade-1"
        );
    }

    #[test]
    fn refund_submission_status_is_provider_specific_and_fails_safe() {
        assert_eq!(
            provider_refund_submission_state("stripe", Some("succeeded")),
            ProviderRefundSubmissionState::Succeeded
        );
        assert_eq!(
            provider_refund_submission_state("wechat_pay", Some("PROCESSING")),
            ProviderRefundSubmissionState::Processing
        );
        assert_eq!(
            provider_refund_submission_state("wechat_pay", Some("ABNORMAL")),
            ProviderRefundSubmissionState::Failed
        );
        assert_eq!(
            provider_refund_submission_state("alipay", Some("Success")),
            ProviderRefundSubmissionState::Succeeded
        );
        assert_eq!(
            provider_refund_submission_state("stripe", Some("unknown-new-status")),
            ProviderRefundSubmissionState::Processing
        );
    }

    #[test]
    fn provider_trade_not_found_is_an_idempotent_cancel_result() {
        for (provider_code, message) in [
            ("wechat_pay", r#"HTTP 404: {"code":"ORDER_NOT_EXIST"}"#),
            (
                "alipay",
                "Alipay alipay.trade.close failed (40004/ACQ.TRADE_NOT_EXIST): trade missing",
            ),
            (
                "stripe",
                r#"HTTP 404: {"code":"resource_missing","message":"No such payment_intent"}"#,
            ),
            (
                "stripe",
                r#"HTTP 400: {"code":"payment_intent_unexpected_state","message":"You cannot cancel this PaymentIntent because it has a status of canceled."}"#,
            ),
            (
                "wechat_pay",
                r#"HTTP 400: {"code":"ORDER_CLOSED","message":"The order has been closed"}"#,
            ),
        ] {
            assert!(cancel_error_means_trade_is_absent(
                provider_code,
                &ProviderError::transport(provider_code, message)
            ));
        }
    }

    #[test]
    fn refund_query_not_found_is_provider_specific() {
        for (provider_code, error) in [
            (
                "stripe",
                ProviderError::invalid_response(
                    PaymentAdapterOperation::QueryRefund,
                    "Stripe refund was not found for the supplied refund_no",
                ),
            ),
            (
                "wechat_pay",
                ProviderError::transport(
                    "wechat_pay",
                    r#"HTTP 404: {"code":"RESOURCE_NOT_EXISTS"}"#,
                ),
            ),
            (
                "alipay",
                ProviderError::transport(
                    "alipay",
                    "Alipay query failed (40004/ACQ.REFUND_NOT_EXIST)",
                ),
            ),
        ] {
            assert!(refund_query_error_means_not_found(provider_code, &error));
        }
        assert!(!refund_query_error_means_not_found(
            "stripe",
            &ProviderError::transport("stripe", "HTTP 401: invalid API key")
        ));
    }

    #[test]
    fn provider_cancel_does_not_hide_unknown_failures() {
        assert!(!cancel_error_means_trade_is_absent(
            "wechat_pay",
            &ProviderError::transport("wechat_pay", "HTTP 401: SIGN_ERROR")
        ));
        assert!(!cancel_error_means_trade_is_absent(
            "wechat_pay",
            &ProviderError::transport("wechat_pay", "HTTP 500: ORDER_NOT_EXIST")
        ));
        assert!(!cancel_error_means_trade_is_absent(
            "alipay",
            &ProviderError::ProviderUnavailable {
                provider_code: "alipay".to_owned(),
                message: "credentials unavailable".to_owned(),
            }
        ));
    }
}
