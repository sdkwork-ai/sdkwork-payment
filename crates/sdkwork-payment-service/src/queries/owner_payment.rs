use std::collections::BTreeMap;

use sdkwork_contract_service::{CommerceMoney, CommerceServiceError};

/// Scoped lookup for payable-order validation inside the payment executor.
///
/// Payment reads `commerce_order` rows only as a foreign reference — order lifecycle
/// remains owned by `sdkwork-order`. Callers must not depend on order crates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderPaymentReferenceQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub order_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderPaymentReferenceSnapshot {
    pub expires_at: Option<String>,
    pub order_id: String,
    pub order_sn: String,
    pub order_subject: Option<String>,
    pub status: String,
    pub total_amount: CommerceMoney,
    /// ISO-4217 currency code of the order (e.g. `CNY`, `USD`). `None` when
    /// the order row carries no currency; callers default to `CNY`.
    pub currency_code: Option<String>,
    pub pay_time: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayOwnerOrderCommand {
    pub order_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub payment_method: String,
    pub payment_scene: Option<String>,
    pub payment_attempt_callback_payload: Option<String>,
    pub payment_metadata: serde_json::Value,
    pub tenant_id: String,
    pub idempotency_key: String,
    pub request_no: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayOwnerOrderCommandInput {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub order_id: String,
    pub payment_method: String,
    pub payment_scene: Option<String>,
    pub payment_attempt_callback_payload: Option<String>,
    pub payment_metadata: serde_json::Value,
    pub request_no: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayOwnerOrderOutcome {
    pub amount: CommerceMoney,
    pub order_id: String,
    pub out_trade_no: String,
    pub payment_id: String,
    pub payment_method: String,
    pub status: String,
    pub payment_params: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelOrderPaymentsCommand {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub order_id: String,
}

impl OrderPaymentReferenceQuery {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
        owner_user_id: &str,
        order_id: &str,
    ) -> Result<Self, CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;
        crate::validation::require_non_empty("owner_user_id", owner_user_id)?;
        crate::validation::require_non_empty("order_id", order_id)?;

        Ok(Self {
            tenant_id: tenant_id.trim().to_string(),
            organization_id: optional_text(organization_id),
            owner_user_id: owner_user_id.trim().to_string(),
            order_id: order_id.trim().to_string(),
        })
    }
}

impl PayOwnerOrderCommand {
    pub fn new(input: PayOwnerOrderCommandInput) -> Result<Self, CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &input.tenant_id)?;
        crate::validation::require_non_empty("owner_user_id", &input.owner_user_id)?;
        crate::validation::require_non_empty("order_id", &input.order_id)?;
        crate::validation::require_non_empty("payment_method", &input.payment_method)?;
        crate::validation::require_non_empty("request_no", &input.request_no)?;
        crate::validation::require_non_empty("idempotency_key", &input.idempotency_key)?;
        if !input.payment_metadata.is_object() {
            return Err(CommerceServiceError::validation(
                "payment_metadata must be a JSON object",
            ));
        }

        Ok(Self {
            order_id: input.order_id.trim().to_string(),
            organization_id: optional_text(input.organization_id.as_deref()),
            owner_user_id: input.owner_user_id.trim().to_string(),
            payment_method: input.payment_method.trim().to_ascii_lowercase(),
            payment_scene: input
                .payment_scene
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty()),
            payment_attempt_callback_payload: input.payment_attempt_callback_payload,
            payment_metadata: input.payment_metadata,
            tenant_id: input.tenant_id.trim().to_string(),
            idempotency_key: input.idempotency_key.trim().to_string(),
            request_no: input.request_no.trim().to_string(),
        })
    }
}

impl CancelOrderPaymentsCommand {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
        owner_user_id: &str,
        order_id: &str,
    ) -> Result<Self, CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;
        crate::validation::require_non_empty("owner_user_id", owner_user_id)?;
        crate::validation::require_non_empty("order_id", order_id)?;

        Ok(Self {
            tenant_id: tenant_id.trim().to_string(),
            organization_id: optional_text(organization_id),
            owner_user_id: owner_user_id.trim().to_string(),
            order_id: order_id.trim().to_string(),
        })
    }
}

fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{PayOwnerOrderCommand, PayOwnerOrderCommandInput};

    #[test]
    fn pay_owner_order_input_preserves_validation_and_normalization() {
        let command = PayOwnerOrderCommand::new(PayOwnerOrderCommandInput {
            tenant_id: " tenant-1 ".to_owned(),
            organization_id: Some(" organization-1 ".to_owned()),
            owner_user_id: " user-1 ".to_owned(),
            order_id: " order-1 ".to_owned(),
            payment_method: " WeChat_Pay ".to_owned(),
            payment_scene: Some(" Mini_Program ".to_owned()),
            payment_attempt_callback_payload: Some("{\"source\":\"test\"}".to_owned()),
            payment_metadata: serde_json::json!({"openid":"openid-1"}),
            request_no: " request-1 ".to_owned(),
            idempotency_key: " idempotency-1 ".to_owned(),
        })
        .expect("pay owner order command");

        assert_eq!(command.tenant_id, "tenant-1");
        assert_eq!(command.organization_id.as_deref(), Some("organization-1"));
        assert_eq!(command.owner_user_id, "user-1");
        assert_eq!(command.order_id, "order-1");
        assert_eq!(command.payment_method, "wechat_pay");
        assert_eq!(command.payment_scene.as_deref(), Some("mini_program"));
        assert_eq!(
            command.payment_attempt_callback_payload.as_deref(),
            Some("{\"source\":\"test\"}")
        );
        assert_eq!(command.payment_metadata["openid"], "openid-1");
        assert_eq!(command.request_no, "request-1");
        assert_eq!(command.idempotency_key, "idempotency-1");

        let error = PayOwnerOrderCommand::new(PayOwnerOrderCommandInput {
            tenant_id: " ".to_owned(),
            organization_id: None,
            owner_user_id: "user-1".to_owned(),
            order_id: "order-1".to_owned(),
            payment_method: "wechat_pay".to_owned(),
            payment_scene: None,
            payment_attempt_callback_payload: None,
            payment_metadata: serde_json::json!({}),
            request_no: "request-1".to_owned(),
            idempotency_key: "idempotency-1".to_owned(),
        })
        .expect_err("blank tenant must be rejected");
        assert_eq!(error.code(), "validation");
    }
}
