//! Sandbox webhook adapter for local development.
//!
//! The sandbox payment provider performs no external PSP HTTP calls; its
//! create/cancel/refund operations are no-ops in `operations.rs`. The only
//! adapter surface that can be reached is webhook ingestion (order settlement
//! routes call `registry.resolve(provider_code)` and then verify/normalize).
//! This adapter accepts every sandbox webhook body without signature
//! verification and normalizes the payment status from the body, so local
//! development can simulate a PSP payment-success callback end to end
//! (order settlement → Token Bank ledger credit).

use serde_json::{json, Value};

use crate::adapter::{
    PaymentAdapterFuture, PaymentAdapterOperation, PaymentCancelPaymentIntentRequest,
    PaymentCreateIntentRequest, PaymentCreateRefundRequest, PaymentNormalizeWebhookRequest,
    PaymentNormalizedWebhookEvent, PaymentProviderAdapter, PaymentProviderCapabilities,
    PaymentProviderOperationOutcome, PaymentQueryPaymentIntentRequest, PaymentQueryRefundRequest,
    PaymentVerifyWebhookRequest, PaymentWebhookVerificationOutcome,
};
use crate::error::{ProviderError, ProviderResult};

const SANDBOX_WEBHOOK_OPERATIONS: &[PaymentAdapterOperation] = &[
    PaymentAdapterOperation::VerifyWebhook,
    PaymentAdapterOperation::NormalizeWebhook,
];

static SANDBOX_CAPABILITIES: PaymentProviderCapabilities = PaymentProviderCapabilities {
    provider_code: "sandbox",
    operations: SANDBOX_WEBHOOK_OPERATIONS,
};

/// Sandbox provider webhook adapter: accepts unsigned bodies and maps the
/// body's `status`/`paymentStatus` to a normalized payment-success event.
#[derive(Debug, Clone, Default)]
pub struct SandboxWebhookPaymentProviderAdapter;

impl SandboxWebhookPaymentProviderAdapter {
    pub fn new() -> Self {
        Self
    }
}

fn unsupported(operation: PaymentAdapterOperation) -> ProviderError {
    ProviderError::invalid_request(
        operation,
        "sandbox provider does not implement this operation; use the webhook surface instead",
    )
}

impl PaymentProviderAdapter for SandboxWebhookPaymentProviderAdapter {
    fn capabilities(&self) -> &'static PaymentProviderCapabilities {
        &SANDBOX_CAPABILITIES
    }

    fn create_payment_intent<'a>(
        &'a self,
        _request: PaymentCreateIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move { Err(unsupported(PaymentAdapterOperation::CreatePaymentIntent)) })
    }

    fn query_payment_intent<'a>(
        &'a self,
        _request: PaymentQueryPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move { Err(unsupported(PaymentAdapterOperation::QueryPaymentIntent)) })
    }

    fn cancel_payment_intent<'a>(
        &'a self,
        _request: PaymentCancelPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move { Err(unsupported(PaymentAdapterOperation::CancelPaymentIntent)) })
    }

    fn create_refund<'a>(
        &'a self,
        _request: PaymentCreateRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move { Err(unsupported(PaymentAdapterOperation::CreateRefund)) })
    }

    fn query_refund<'a>(
        &'a self,
        _request: PaymentQueryRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move { Err(unsupported(PaymentAdapterOperation::QueryRefund)) })
    }

    fn verify_webhook<'a>(
        &'a self,
        _request: PaymentVerifyWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentWebhookVerificationOutcome> {
        Box::pin(async move {
            Ok(PaymentWebhookVerificationOutcome {
                verified: true,
                provider_event_id: None,
            })
        })
    }

    fn normalize_webhook<'a>(
        &'a self,
        request: PaymentNormalizeWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentNormalizedWebhookEvent> {
        Box::pin(async move {
            let payload: Value = if request.body.is_empty() {
                json!({})
            } else {
                serde_json::from_slice(&request.body).map_err(|error| {
                    ProviderError::invalid_response(
                        PaymentAdapterOperation::NormalizeWebhook,
                        format!("sandbox webhook body must be valid JSON: {error}"),
                    )
                })?
            };
            let out_trade_no = payload
                .get("outTradeNo")
                .or_else(|| payload.get("out_trade_no"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let payment_status = payload
                .get("paymentStatus")
                .or_else(|| payload.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("succeeded")
                .to_owned();
            let event_id = payload
                .get("eventId")
                .or_else(|| payload.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            Ok(PaymentNormalizedWebhookEvent {
                provider_code: "sandbox".to_owned(),
                event_type: payload
                    .get("eventType")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                provider_event_id: event_id,
                out_trade_no,
                payment_status: Some(payment_status),
                payload,
            })
        })
    }
}
