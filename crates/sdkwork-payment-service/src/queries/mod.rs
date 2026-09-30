mod intent;
mod owner_payment;
mod refund;

pub use intent::{
    CancelOwnerPaymentIntentCommand, CreateOwnerPaymentAttemptCommand,
    CreateOwnerPaymentAttemptOutcome, CreateOwnerPaymentIntentCommand, PaymentIntentDetailQuery,
    PaymentIntentView,
};
pub use owner_payment::{
    CancelOrderPaymentsCommand, OrderPaymentReferenceQuery, OrderPaymentReferenceSnapshot,
    PayOwnerOrderCommand, PayOwnerOrderCommandInput, PayOwnerOrderOutcome,
};
pub use refund::{
    CreateOwnerRefundCommand, RefundDetailQuery, RefundListPage, RefundListQuery, RefundView,
};

use crate::domain::PaymentRecordItem;

/// Phase 1.3：标准分页结果，store 一次性返回当前页 items + 满足条件的总记录数。
///
/// `total_items` 来自 SQL `COUNT(*) OVER()` 窗口函数（单次往返），
/// handler 据此填充 `data.pageInfo`，禁止在进程内对全量数据做 skip/take
/// （PAGINATION_SPEC §2）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecordListPage {
    pub items: Vec<PaymentRecordItem>,
    pub total_items: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentMethodListQuery {
    pub organization_id: Option<String>,
    pub tenant_id: String,
    pub offset: i64,
    pub limit: i64,
    /// Filters methods that expose the given channel `scene_code` (e.g. `web`, `app`).
    pub scene_code_filter: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentMethodListPage {
    pub items: Vec<PaymentMethodItem>,
    pub total_items: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentMethodItem {
    pub display_name: String,
    pub id: String,
    pub method_key: String,
    pub provider_code: String,
    pub scene_codes: Vec<String>,
    pub sort_order: i64,
}

impl PaymentMethodListQuery {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
    ) -> Result<Self, sdkwork_contract_service::CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;

        Ok(Self {
            organization_id: optional_text(organization_id),
            tenant_id: tenant_id.trim().to_string(),
            offset: 0,
            limit: 20,
            scene_code_filter: None,
        })
    }

    pub fn with_paging(mut self, offset: i64, limit: i64) -> Self {
        self.offset = offset.max(0);
        self.limit = limit.clamp(1, 200);
        self
    }

    pub fn with_scene_code_filter(mut self, scene_code: Option<String>) -> Self {
        self.scene_code_filter = scene_code;
        self
    }
}

/// Map API `clientType` wire values to persistence `scene_code` filters.
pub fn scene_code_filter_from_client_type(client_type: Option<&str>) -> Option<String> {
    let value = client_type?.trim();
    if value.is_empty() {
        return None;
    }
    Some(match value.to_ascii_lowercase().as_str() {
        "pc" | "web" => "web".to_owned(),
        "app" => "app".to_owned(),
        "mini_program" | "miniprogram" => "mini_program".to_owned(),
        "api" => "api".to_owned(),
        other => other.to_owned(),
    })
}

/// Map persistence `scene_code` values to API `productTypes` wire codes.
pub fn wire_product_types_from_scene_codes(scenes: &[String]) -> Vec<(String, String)> {
    use std::collections::BTreeSet;

    let mut codes = BTreeSet::new();
    for scene in scenes {
        match scene.trim().to_ascii_lowercase().as_str() {
            "" => {}
            "web" => {
                codes.insert("pc".to_string());
            }
            "app" => {
                codes.insert("app".to_string());
            }
            "mini_program" => {
                codes.insert("mini_program".to_string());
            }
            "api" => {
                codes.insert("api".to_string());
            }
            other => {
                codes.insert(other.to_string());
            }
        }
    }
    if codes.is_empty() {
        codes.insert("pc".to_string());
    }

    codes
        .into_iter()
        .map(|code| (code.clone(), wire_product_type_display_name(&code)))
        .collect()
}

fn wire_product_type_display_name(code: &str) -> String {
    match code {
        "pc" => "PC".to_string(),
        "app" => "App".to_string(),
        "mini_program" => "Mini Program".to_string(),
        "api" => "API".to_string(),
        other => other.to_string(),
    }
}

pub fn parse_scene_codes_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|scene| !scene.is_empty())
        .map(str::to_owned)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecordListQuery {
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub tenant_id: String,
    /// Phase 1.3：分页 offset，由 handler 通过 `OffsetListPageParams::parse` 解析后下推到 SQL。
    pub offset: i64,
    /// Phase 1.3：分页 limit，由 handler 通过 `OffsetListPageParams::parse` 解析后下推到 SQL。
    pub limit: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecordDetailQuery {
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub payment_id: String,
    pub tenant_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecordOrderListQuery {
    pub order_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub tenant_id: String,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecordOrderListPage {
    pub items: Vec<PaymentRecordItem>,
    pub total_items: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecordOutTradeNoQuery {
    pub organization_id: Option<String>,
    pub out_trade_no: String,
    pub owner_user_id: String,
    pub tenant_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecordStatisticsQuery {
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub tenant_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecordStatistics {
    pub total_count: i64,
    pub pending_count: i64,
    pub succeeded_count: i64,
    pub failed_count: i64,
    pub timeout_count: i64,
    pub closed_count: i64,
}

impl PaymentRecordListQuery {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
        owner_user_id: &str,
    ) -> Result<Self, sdkwork_contract_service::CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;
        crate::validation::require_non_empty("owner_user_id", owner_user_id)?;

        Ok(Self {
            organization_id: optional_text(organization_id),
            owner_user_id: owner_user_id.trim().to_string(),
            tenant_id: tenant_id.trim().to_string(),
            offset: 0,
            limit: 20,
        })
    }

    /// Phase 1.3：注入标准分页参数（offset/limit），由 handler 在解析 URL 参数后调用。
    pub fn with_paging(mut self, offset: i64, limit: i64) -> Self {
        self.offset = offset.max(0);
        self.limit = limit.clamp(1, 200);
        self
    }
}

impl PaymentRecordDetailQuery {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
        owner_user_id: &str,
        payment_id: &str,
    ) -> Result<Self, sdkwork_contract_service::CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;
        crate::validation::require_non_empty("owner_user_id", owner_user_id)?;
        crate::validation::require_non_empty("payment_id", payment_id)?;

        Ok(Self {
            organization_id: optional_text(organization_id),
            owner_user_id: owner_user_id.trim().to_string(),
            payment_id: payment_id.trim().to_string(),
            tenant_id: tenant_id.trim().to_string(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosePaymentRecordCommand {
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub payment_id: String,
    pub tenant_id: String,
}

impl ClosePaymentRecordCommand {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
        owner_user_id: &str,
        payment_id: &str,
    ) -> Result<Self, sdkwork_contract_service::CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;
        crate::validation::require_non_empty("owner_user_id", owner_user_id)?;
        crate::validation::require_non_empty("payment_id", payment_id)?;

        Ok(Self {
            organization_id: optional_text(organization_id),
            owner_user_id: owner_user_id.trim().to_string(),
            payment_id: payment_id.trim().to_string(),
            tenant_id: tenant_id.trim().to_string(),
        })
    }
}

impl PaymentRecordOrderListQuery {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
        owner_user_id: &str,
        order_id: &str,
    ) -> Result<Self, sdkwork_contract_service::CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;
        crate::validation::require_non_empty("owner_user_id", owner_user_id)?;
        crate::validation::require_non_empty("order_id", order_id)?;

        Ok(Self {
            order_id: order_id.trim().to_string(),
            organization_id: optional_text(organization_id),
            owner_user_id: owner_user_id.trim().to_string(),
            tenant_id: tenant_id.trim().to_string(),
            offset: 0,
            limit: 20,
        })
    }

    pub fn with_paging(mut self, offset: i64, limit: i64) -> Self {
        self.offset = offset.max(0);
        self.limit = limit.clamp(1, 200);
        self
    }
}

impl PaymentRecordOutTradeNoQuery {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
        owner_user_id: &str,
        out_trade_no: &str,
    ) -> Result<Self, sdkwork_contract_service::CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;
        crate::validation::require_non_empty("owner_user_id", owner_user_id)?;
        crate::validation::require_non_empty("out_trade_no", out_trade_no)?;

        Ok(Self {
            organization_id: optional_text(organization_id),
            out_trade_no: out_trade_no.trim().to_string(),
            owner_user_id: owner_user_id.trim().to_string(),
            tenant_id: tenant_id.trim().to_string(),
        })
    }
}

impl PaymentRecordStatisticsQuery {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
        owner_user_id: &str,
    ) -> Result<Self, sdkwork_contract_service::CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", tenant_id)?;
        crate::validation::require_non_empty("owner_user_id", owner_user_id)?;

        Ok(Self {
            organization_id: optional_text(organization_id),
            owner_user_id: owner_user_id.trim().to_string(),
            tenant_id: tenant_id.trim().to_string(),
        })
    }
}

fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
