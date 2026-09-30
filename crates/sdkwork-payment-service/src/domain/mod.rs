use sdkwork_contract_service::{CommerceMoney, CommerceServiceError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecordItem {
    pub id: String,
    pub order_id: String,
    pub order_no: String,
    pub method: String,
    pub amount: CommerceMoney,
    pub date: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentIntentDraft {
    pub amount: CommerceMoney,
    pub idempotency_key: String,
    pub order_id: String,
    pub payment_method: String,
    pub provider_code: String,
    pub tenant_id: String,
}

/// Payment lifecycle wire states, exactly matching the attempt and intent
/// CHECK constraints in `database/ddl/baseline/postgres/0001_payment_baseline.sql`.
///
/// Refund completeness is intentionally NOT a payment state: refunds are
/// tracked on `commerce_refund` rows (reserve while `submitted`/`processing`),
/// and a fully or partially refunded payment keeps its terminal capture state
/// `succeeded` — the Stripe model. A refund can never mark a paid payment
/// `failed`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentStatus {
    Created,
    Pending,
    Succeeded,
    Failed,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentTransition {
    from: PaymentStatus,
    to: PaymentStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefundStatus {
    Requested,
    Processing,
    Succeeded,
    Failed,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundTransition {
    from: RefundStatus,
    to: RefundStatus,
}

impl PaymentRecordItem {
    pub fn new(
        id: &str,
        order_id: &str,
        order_no: &str,
        method: &str,
        amount: CommerceMoney,
        date: &str,
        status: &str,
    ) -> Result<Self, CommerceServiceError> {
        crate::validation::require_non_empty("id", id)?;
        crate::validation::require_non_empty("order_id", order_id)?;
        crate::validation::require_non_empty("order_no", order_no)?;
        crate::validation::require_non_empty("method", method)?;
        crate::validation::require_non_empty("date", date)?;
        crate::validation::require_non_empty("status", status)?;

        Ok(Self {
            id: id.trim().to_string(),
            order_id: order_id.trim().to_string(),
            order_no: order_no.trim().to_string(),
            method: method.trim().to_string(),
            amount,
            date: date.trim().to_string(),
            status: status.trim().to_string(),
        })
    }
}

impl PaymentIntentDraft {
    pub fn new(
        tenant_id: &str,
        order_id: &str,
        payment_method: &str,
        provider_code: &str,
        amount: CommerceMoney,
        idempotency_key: &str,
    ) -> Result<Self, CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;
        crate::validation::require_non_empty("order_id", order_id)?;
        crate::validation::require_non_empty("payment_method", payment_method)?;
        crate::validation::require_non_empty("provider_code", provider_code)?;
        crate::validation::require_non_empty("idempotency_key", idempotency_key)?;

        Ok(Self {
            amount,
            idempotency_key: idempotency_key.trim().to_string(),
            order_id: order_id.trim().to_string(),
            payment_method: payment_method.trim().to_ascii_lowercase(),
            provider_code: provider_code.trim().to_ascii_lowercase(),
            tenant_id: tenant_id.trim().to_string(),
        })
    }
}

impl PaymentStatus {
    pub fn from_wire(value: &str) -> Result<Self, CommerceServiceError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "created" => Ok(Self::Created),
            "pending" | "processing" => Ok(Self::Pending),
            "succeeded" | "success" | "paid" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "canceled" | "cancelled" | "closed" => Ok(Self::Closed),
            "refunding" | "refunded" => Err(CommerceServiceError::validation(
                "refund completeness is tracked on refund rows, not payment status",
            )),
            other => Err(CommerceServiceError::validation(format!(
                "unknown payment status: {other}"
            ))),
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Pending => "pending",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Closed => "canceled",
        }
    }
}

impl RefundStatus {
    pub fn from_wire(value: &str) -> Result<Self, CommerceServiceError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "submitted" | "requested" => Ok(Self::Requested),
            "processing" => Ok(Self::Processing),
            "succeeded" | "success" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "closed" | "canceled" | "cancelled" => Ok(Self::Closed),
            other => Err(CommerceServiceError::validation(format!(
                "unknown refund status: {other}"
            ))),
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Requested => "submitted",
            Self::Processing => "processing",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Closed => "closed",
        }
    }
}

/// Validate a payment status change using persistence wire values (`pending`, `canceled`, …).
pub fn validate_payment_wire_transition(from: &str, to: &str) -> Result<(), CommerceServiceError> {
    let from_status = PaymentStatus::from_wire(from)?;
    let to_status = PaymentStatus::from_wire(to)?;
    if from_status == to_status {
        return Ok(());
    }
    PaymentTransition::new(from_status, to_status).validate()
}

/// Validate a refund status change. Pass `from: None` when creating a new refund row.
pub fn validate_refund_wire_transition(
    from: Option<&str>,
    to: &str,
) -> Result<(), CommerceServiceError> {
    let to_status = RefundStatus::from_wire(to)?;
    let Some(from) = from.map(str::trim).filter(|value| !value.is_empty()) else {
        // Creation anchors at the submitted state: a refund row can never be
        // created directly in a terminal or in-flight state.
        if matches!(to_status, RefundStatus::Requested) {
            return Ok(());
        }
        return Err(CommerceServiceError::invalid_state(
            "refund creation must start from the submitted state",
        ));
    };
    let from_status = RefundStatus::from_wire(from)?;
    if from_status == to_status {
        return Ok(());
    }
    RefundTransition::new(from_status, to_status).validate()
}

impl PaymentTransition {
    pub fn new(from: PaymentStatus, to: PaymentStatus) -> Self {
        Self { from, to }
    }

    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        match (&self.from, &self.to) {
            (PaymentStatus::Created, PaymentStatus::Pending)
            // The SQL close guard accepts a `created` intent whose checkout
            // never started; the wire machine matches it.
            | (PaymentStatus::Created, PaymentStatus::Closed)
            | (PaymentStatus::Pending, PaymentStatus::Succeeded)
            | (PaymentStatus::Pending, PaymentStatus::Failed)
            | (PaymentStatus::Pending, PaymentStatus::Closed) => Ok(()),
            _ => Err(CommerceServiceError::invalid_state(
                "invalid payment status transition",
            )),
        }
    }
}

impl RefundTransition {
    pub fn new(from: RefundStatus, to: RefundStatus) -> Self {
        Self { from, to }
    }

    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        match (&self.from, &self.to) {
            (RefundStatus::Requested, RefundStatus::Processing)
            // Alipay `trade.refund` settles synchronously: a fresh refund can
            // land directly in its terminal state from one PSP response.
            | (RefundStatus::Requested, RefundStatus::Succeeded)
            | (RefundStatus::Requested, RefundStatus::Failed)
            | (RefundStatus::Requested, RefundStatus::Closed)
            | (RefundStatus::Processing, RefundStatus::Succeeded)
            | (RefundStatus::Processing, RefundStatus::Failed)
            | (RefundStatus::Processing, RefundStatus::Closed)
            // A PSP status query can reveal that a previously failed refund
            // actually settled; a failed refund can also be abandoned.
            | (RefundStatus::Failed, RefundStatus::Processing)
            | (RefundStatus::Failed, RefundStatus::Succeeded)
            | (RefundStatus::Failed, RefundStatus::Closed) => Ok(()),
            _ => Err(CommerceServiceError::invalid_state(
                "invalid refund status transition",
            )),
        }
    }
}
