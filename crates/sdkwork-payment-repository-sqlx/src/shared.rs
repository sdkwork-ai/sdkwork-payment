//! Shared utility functions for the commerce payment repository-sqlx crate.
//!
//! These helpers are used across both PostgreSQL and SQLite repository
//! implementations. Keeping them in a single module eliminates duplication
//! and ensures consistent behavior.
use chrono::{DateTime, SecondsFormat, Utc};
use sdkwork_contract_service::{CommerceMoney, CommerceServiceError};
use sdkwork_payment_service::{
    validate_payment_wire_transition, validate_refund_wire_transition,
    CreateOwnerPaymentAttemptCommand, CreateOwnerPaymentAttemptOutcome,
    CreateOwnerPaymentIntentCommand, CreateOwnerRefundCommand, PayOwnerOrderCommand,
    PayOwnerOrderOutcome, PaymentIntentView, RefundView,
};
use sqlx::postgres::PgRow;
use sqlx::Row;
pub(crate) fn payment_attempt_is_terminal_success(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "succeeded" | "success" | "paid"
    )
}
pub(crate) fn required_persisted_paid_at(paid_at: &str) -> Result<String, CommerceServiceError> {
    let paid_at = paid_at.trim();
    if paid_at.is_empty() {
        return Err(CommerceServiceError::storage(
            "succeeded payment attempt is missing persisted paid_at",
        ));
    }
    DateTime::parse_from_rfc3339(paid_at)
        .map(|_| paid_at.to_owned())
        .map_err(|_| {
            CommerceServiceError::storage(
                "succeeded payment attempt has a non-RFC3339 persisted paid_at",
            )
        })
}
pub(crate) fn ensure_confirmation_intent_update(
    rows_affected: u64,
    persisted_status: Option<&str>,
) -> Result<(), CommerceServiceError> {
    match rows_affected {
        1 => Ok(()),
        0 => {
            let Some(status) = persisted_status else {
                return Err(CommerceServiceError::storage(
                    "owner payment intent disappeared during confirmation",
                ));
            };
            if payment_attempt_is_terminal_success(status) {
                return Ok(());
            }
            ensure_payment_status_transition(status, "succeeded")?;
            Err(CommerceServiceError::storage(
                "owner payment intent was not updated despite a transitionable status",
            ))
        }
        count => Err(CommerceServiceError::storage(format!(
            "owner payment intent confirmation updated {count} rows; expected at most one"
        ))),
    }
}
pub(crate) fn resolve_confirmation_attempt_replayed(
    rows_affected: u64,
    persisted_status: Option<&str>,
) -> Result<bool, CommerceServiceError> {
    let Some(status) = persisted_status else {
        return Err(CommerceServiceError::storage(
            "owner payment attempt disappeared during confirmation",
        ));
    };
    match rows_affected {
        1 if payment_attempt_is_terminal_success(status) => Ok(false),
        0 if payment_attempt_is_terminal_success(status) => Ok(true),
        0 => {
            ensure_payment_status_transition(status, "succeeded")?;
            Err(CommerceServiceError::storage(
                "owner payment attempt was not updated despite a transitionable status",
            ))
        }
        1 => Err(CommerceServiceError::storage(
            "owner payment attempt update did not persist succeeded status",
        )),
        count => Err(CommerceServiceError::storage(format!(
            "owner payment attempt confirmation updated {count} rows; expected at most one"
        ))),
    }
}
pub(crate) fn ensure_payment_status_transition(
    from: &str,
    to: &str,
) -> Result<(), CommerceServiceError> {
    validate_payment_wire_transition(from, to)
}
pub(crate) fn ensure_refund_status_transition(
    from: Option<&str>,
    to: &str,
) -> Result<(), CommerceServiceError> {
    validate_refund_wire_transition(from, to)
}
pub(crate) fn ensure_payment_intent_idempotency_replay_matches(
    command: &CreateOwnerPaymentIntentCommand,
    existing: &PaymentIntentView,
) -> Result<(), CommerceServiceError> {
    if existing.order_id != command.order_id
        || !existing
            .payment_method
            .eq_ignore_ascii_case(&command.payment_method)
    {
        return Err(idempotency_parameter_conflict("payment intent"));
    }
    Ok(())
}
pub(crate) fn ensure_payment_attempt_idempotency_replay_matches(
    command: &CreateOwnerPaymentAttemptCommand,
    existing: &CreateOwnerPaymentAttemptOutcome,
) -> Result<(), CommerceServiceError> {
    if existing.payment_intent_id != command.payment_intent_id {
        return Err(idempotency_parameter_conflict("payment attempt"));
    }
    Ok(())
}
pub(crate) fn ensure_owner_payment_idempotency_replay_matches(
    command: &PayOwnerOrderCommand,
    existing: &PayOwnerOrderOutcome,
    callback_payload: &str,
) -> Result<(), CommerceServiceError> {
    if existing.order_id != command.order_id
        || !existing
            .payment_method
            .eq_ignore_ascii_case(&command.payment_method)
    {
        return Err(idempotency_parameter_conflict("order payment"));
    }
    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(callback_payload) {
        if let Some(object) = payload.as_object() {
            if object
                .get("paymentMetadata")
                .is_some_and(|metadata| metadata != &command.payment_metadata)
            {
                return Err(idempotency_parameter_conflict("order payment"));
            }
            if let Some(persisted_scene) = object.get("paymentScene") {
                let requested_scene = command
                    .payment_scene
                    .as_deref()
                    .map(|value| serde_json::Value::String(value.to_owned()))
                    .unwrap_or(serde_json::Value::Null);
                if persisted_scene != &requested_scene {
                    return Err(idempotency_parameter_conflict("order payment"));
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn owner_payment_reuse_matches(
    command: &PayOwnerOrderCommand,
    callback_payload: &str,
) -> bool {
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(callback_payload) else {
        return false;
    };
    let Some(object) = payload.as_object() else {
        return false;
    };
    let requested_scene = command
        .payment_scene
        .as_deref()
        .map(|value| serde_json::Value::String(value.to_owned()))
        .unwrap_or(serde_json::Value::Null);
    object.get("paymentScene") == Some(&requested_scene)
        && object.get("paymentMetadata") == Some(&command.payment_metadata)
}
fn snapshot_provider_account(
    object: &mut serde_json::Map<String, serde_json::Value>,
    provider_account_id: Option<&str>,
) {
    if let Some(provider_account_id) = provider_account_id {
        object.insert(
            "providerAccountId".to_owned(),
            serde_json::Value::String(provider_account_id.to_owned()),
        );
    }
}
pub(crate) fn payment_attempt_callback_payload(provider_account_id: Option<&str>) -> String {
    let mut object = serde_json::Map::new();
    snapshot_provider_account(&mut object, provider_account_id);
    serde_json::Value::Object(object).to_string()
}
pub(crate) fn owner_payment_callback_payload(
    command: &PayOwnerOrderCommand,
    provider_account_id: Option<&str>,
) -> String {
    let raw = command
        .payment_attempt_callback_payload
        .as_deref()
        .unwrap_or("{}");
    let Ok(mut payload) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_owned();
    };
    let Some(object) = payload.as_object_mut() else {
        return raw.to_owned();
    };
    object.insert(
        "paymentMetadata".to_owned(),
        command.payment_metadata.clone(),
    );
    object.insert(
        "paymentScene".to_owned(),
        command
            .payment_scene
            .as_deref()
            .map(|value| serde_json::Value::String(value.to_owned()))
            .unwrap_or(serde_json::Value::Null),
    );
    snapshot_provider_account(object, provider_account_id);
    payload.to_string()
}
pub(crate) fn ensure_refund_idempotency_replay_matches(
    command: &CreateOwnerRefundCommand,
    existing: &RefundView,
) -> Result<(), CommerceServiceError> {
    let amount_matches = match command.amount.as_deref() {
        Some(amount) => {
            money_to_minor_units(amount)? == money_to_minor_units(existing.amount.as_str())?
        }
        None => true,
    };
    let attempt_matches = command
        .payment_attempt_id
        .as_deref()
        .map(|attempt_id| attempt_id == existing.payment_attempt_id)
        .unwrap_or(true);
    let reason_matches = normalized_optional_text(command.reason_code.as_deref())
        == normalized_optional_text(existing.reason_code.as_deref());
    if existing.order_id != command.order_id
        || !existing
            .currency_code
            .eq_ignore_ascii_case(&command.currency_code)
        || !amount_matches
        || !attempt_matches
        || !reason_matches
    {
        return Err(idempotency_parameter_conflict("refund"));
    }
    Ok(())
}
pub(crate) fn ensure_refund_requester_idempotency_replay_matches(
    command: &CreateOwnerRefundCommand,
    requested_by_type: &str,
    requested_by: &str,
) -> Result<(), CommerceServiceError> {
    if !requested_by_type.eq_ignore_ascii_case(&command.requested_by_type)
        || requested_by != command.requested_by
    {
        return Err(idempotency_parameter_conflict("refund"));
    }
    Ok(())
}
fn normalized_optional_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
fn idempotency_parameter_conflict(resource: &str) -> CommerceServiceError {
    CommerceServiceError::conflict(format!(
        "{resource} idempotency key was already used with different parameters"
    ))
}
/// Wrap a storage-layer error with a descriptive context message.
///
/// Accepts any `Display` type so it works uniformly with `sqlx::Error`,
/// `std::io::Error`, and other error types.
pub(crate) fn store_error(message: &str, error: impl std::fmt::Display) -> CommerceServiceError {
    CommerceServiceError::storage(format!("{message}: {error}"))
}
/// Produce a deterministic, filesystem-safe storage identifier from path parts.
///
/// Each part is sanitized: non-alphanumeric characters (except `-`, `_`, `.`)
/// are replaced with `-`, and parts are joined with `-`.
pub(crate) fn stable_storage_id(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| {
            part.chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                        character
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("-")
}
pub(crate) fn provider_out_trade_no(
    tenant_id: &str,
    order_id: &str,
    idempotency_key: &str,
) -> String {
    let fingerprint = format!(
        "payment-provider-trade:v1|{}:{}|{}:{}|{}:{}",
        tenant_id.len(),
        tenant_id,
        order_id.len(),
        order_id,
        idempotency_key.len(),
        idempotency_key,
    );
    let digest = sdkwork_utils_rust::crypto::sha256_hash(fingerprint.as_bytes());
    format!("SW{}", &digest[..30])
}
/// Return the current UTC timestamp in the wire/storage RFC3339 format.
pub(crate) fn current_timestamp_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
/// Parse a money string into integer smallest currency units.
///
/// `CommerceMoney` is stored and exchanged as a non-negative integer string in
/// the smallest currency unit. For CNY/USD this means cents; for provider APIs
/// this value can be passed directly as the minor-unit amount.
///
/// # Errors
///
/// Returns a `validation` error if the value is empty, non-numeric, negative,
/// or overflows `i64`.
pub(crate) fn money_to_minor_units(value: &str) -> Result<i64, CommerceServiceError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommerceServiceError::validation(
            "money amount must not be empty",
        ));
    }
    if !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Err(CommerceServiceError::validation(
            "money amount must be a non-negative integer smallest-unit amount",
        ));
    }
    trimmed
        .parse::<i64>()
        .map_err(|_| CommerceServiceError::validation("money amount overflows i64 minor units"))
}
/// Resolve the refund amount string from the command or default to the paid
/// amount (the succeeded payment attempt amount).
pub(crate) fn resolve_refund_amount(
    command: &CreateOwnerRefundCommand,
    paid_amount: &CommerceMoney,
) -> Result<String, CommerceServiceError> {
    Ok(command
        .amount
        .clone()
        .unwrap_or_else(|| paid_amount.as_str().to_owned()))
}
/// Validate that the refund amount is positive and does not exceed the paid
/// amount the refund is anchored to.
pub(crate) fn validate_refund_bounds(
    refund_minor: i64,
    paid_minor: i64,
) -> Result<(), CommerceServiceError> {
    if refund_minor <= 0 {
        return Err(CommerceServiceError::validation(
            "refund amount must be greater than zero",
        ));
    }
    if refund_minor > paid_minor {
        return Err(CommerceServiceError::conflict(
            "refund amount exceeds original payment amount",
        ));
    }
    Ok(())
}
pub(crate) fn string_cell<R: StringCellRow>(row: &R, column: &str) -> String {
    row.string_cell(column)
}
pub(crate) trait StringCellRow {
    fn string_cell(&self, column: &str) -> String;
}
impl StringCellRow for PgRow {
    fn string_cell(&self, column: &str) -> String {
        self.try_get::<Option<String>, _>(column)
            .ok()
            .flatten()
            .unwrap_or_default()
    }
}
#[cfg(test)]
mod tests {
    use super::{
        ensure_confirmation_intent_update, ensure_owner_payment_idempotency_replay_matches,
        ensure_refund_idempotency_replay_matches, owner_payment_callback_payload,
        owner_payment_reuse_matches, payment_attempt_callback_payload, provider_out_trade_no,
        required_persisted_paid_at, resolve_confirmation_attempt_replayed,
    };
    use sdkwork_contract_service::CommerceMoney;
    use sdkwork_payment_service::{
        CreateOwnerRefundCommand, PayOwnerOrderCommand, PayOwnerOrderCommandInput,
        PayOwnerOrderOutcome, RefundView,
    };
    #[test]
    fn confirmation_replay_requires_and_preserves_persisted_paid_at() {
        assert_eq!(
            required_persisted_paid_at("2026-07-12T01:02:03Z").expect("persisted paid_at"),
            "2026-07-12T01:02:03Z"
        );
        assert!(required_persisted_paid_at(" ").is_err());
    }
    #[test]
    fn confirmation_update_counts_distinguish_first_write_from_replay() {
        assert!(!resolve_confirmation_attempt_replayed(1, Some("succeeded"))
            .expect("first confirmation"));
        assert!(
            resolve_confirmation_attempt_replayed(0, Some("succeeded")).expect("concurrent replay")
        );
        assert!(resolve_confirmation_attempt_replayed(0, Some("pending")).is_err());
        assert!(resolve_confirmation_attempt_replayed(2, Some("succeeded")).is_err());
        ensure_confirmation_intent_update(1, None).expect("updated intent");
        ensure_confirmation_intent_update(0, Some("succeeded")).expect("intent replay");
        assert!(ensure_confirmation_intent_update(0, Some("pending")).is_err());
        assert!(ensure_confirmation_intent_update(0, None).is_err());
    }
    #[test]
    fn refund_idempotency_replay_rejects_changed_financial_parameters() {
        let command = CreateOwnerRefundCommand::new_with_currency(
            "tenant-1",
            None,
            "user-1",
            "order-1",
            Some("attempt-1"),
            Some("500"),
            Some("CNY"),
            Some("customer_request"),
            "request-2",
            "idempotency-1",
        )
        .expect("refund command");
        let existing = RefundView {
            amount: CommerceMoney::new("400").expect("amount"),
            currency_code: "CNY".to_owned(),
            order_id: "order-1".to_owned(),
            payment_attempt_id: "attempt-1".to_owned(),
            reason_code: Some("customer_request".to_owned()),
            refund_id: "refund-1".to_owned(),
            refund_no: "RF-1".to_owned(),
            status: "submitted".to_owned(),
        };
        let error = ensure_refund_idempotency_replay_matches(&command, &existing)
            .expect_err("changed amount must conflict");
        assert_eq!(error.code(), "conflict");
    }
    #[test]
    fn owner_payment_callback_payload_preserves_domain_data_and_snapshots_provider_input() {
        let command = PayOwnerOrderCommand::new(PayOwnerOrderCommandInput {
            tenant_id: "tenant-1".to_owned(),
            organization_id: Some("organization-1".to_owned()),
            owner_user_id: "user-1".to_owned(),
            order_id: "order-1".to_owned(),
            payment_method: "wechat_jsapi".to_owned(),
            payment_scene: Some("mini_program".to_owned()),
            payment_attempt_callback_payload: Some(serde_json::json!({"points": 100}).to_string()),
            payment_metadata: serde_json::json!({"openid": "payer-1"}),
            request_no: "request-1".to_owned(),
            idempotency_key: "idempotency-1".to_owned(),
        })
        .expect("owner payment command");
        let callback_payload = owner_payment_callback_payload(&command, Some("provider-account-1"));
        let payload: serde_json::Value =
            serde_json::from_str(&callback_payload).expect("canonical callback payload");
        assert_eq!(payload["points"], 100);
        assert_eq!(payload["paymentMetadata"]["openid"], "payer-1");
        assert_eq!(payload["paymentScene"], "mini_program");
        assert_eq!(payload["providerAccountId"], "provider-account-1");
        let changed_command = PayOwnerOrderCommand::new(PayOwnerOrderCommandInput {
            tenant_id: "tenant-1".to_owned(),
            organization_id: Some("organization-1".to_owned()),
            owner_user_id: "user-1".to_owned(),
            order_id: "order-1".to_owned(),
            payment_method: "wechat_jsapi".to_owned(),
            payment_scene: Some("mini_program".to_owned()),
            payment_attempt_callback_payload: Some(callback_payload.clone()),
            payment_metadata: serde_json::json!({"openid": "payer-2"}),
            request_no: "request-2".to_owned(),
            idempotency_key: "idempotency-1".to_owned(),
        })
        .expect("changed owner payment command");
        let existing = PayOwnerOrderOutcome {
            amount: CommerceMoney::new("100").expect("amount"),
            order_id: "order-1".to_owned(),
            out_trade_no: "trade-1".to_owned(),
            payment_id: "attempt-1".to_owned(),
            payment_method: "wechat_jsapi".to_owned(),
            status: "pending".to_owned(),
            payment_params: Default::default(),
        };
        let error = ensure_owner_payment_idempotency_replay_matches(
            &changed_command,
            &existing,
            &callback_payload,
        )
        .expect_err("changed payer metadata must conflict");
        assert_eq!(error.code(), "conflict");
        assert!(owner_payment_reuse_matches(&command, &callback_payload));
        assert!(!owner_payment_reuse_matches(
            &changed_command,
            &callback_payload
        ));
        let changed_scene = PayOwnerOrderCommand::new(PayOwnerOrderCommandInput {
            tenant_id: "tenant-1".to_owned(),
            organization_id: Some("organization-1".to_owned()),
            owner_user_id: "user-1".to_owned(),
            order_id: "order-1".to_owned(),
            payment_method: "wechat_jsapi".to_owned(),
            payment_scene: Some("mobile_cashier_h5".to_owned()),
            payment_attempt_callback_payload: None,
            payment_metadata: serde_json::json!({"openid": "payer-1"}),
            request_no: "request-3".to_owned(),
            idempotency_key: "idempotency-2".to_owned(),
        })
        .expect("changed-scene owner payment command");
        assert!(!owner_payment_reuse_matches(
            &changed_scene,
            &callback_payload
        ));
        assert!(!owner_payment_reuse_matches(&command, "{}"));
        assert!(!owner_payment_reuse_matches(&command, "not-json"));
    }
    #[test]
    fn two_step_attempt_callback_payload_snapshots_provider_account() {
        let payload: serde_json::Value = serde_json::from_str(&payment_attempt_callback_payload(
            Some("provider-account-2"),
        ))
        .expect("payment attempt callback payload");
        assert_eq!(payload["providerAccountId"], "provider-account-2");
        assert_eq!(payment_attempt_callback_payload(None), "{}");
    }
    #[test]
    fn provider_trade_number_is_fixed_width_ascii_and_unambiguous() {
        let trade = provider_out_trade_no("租户", "订单/一", "重复点击:支付");
        assert_eq!(trade.len(), 32);
        assert!(trade.starts_with("SW"));
        assert!(trade
            .chars()
            .all(|character| character.is_ascii_alphanumeric()));
        assert_eq!(
            trade,
            provider_out_trade_no("租户", "订单/一", "重复点击:支付")
        );
        assert_ne!(
            provider_out_trade_no("a:bc", "d", "e"),
            provider_out_trade_no("a", "bc:d", "e")
        );
    }
}