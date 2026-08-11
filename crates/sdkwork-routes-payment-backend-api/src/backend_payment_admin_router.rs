use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_utils_rust::OffsetListPageParams;
use sdkwork_web_core::WebRequestContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{postgres::PgRow, PgPool, Row};
use crate::api_response::{
    conflict, map_service_error, not_found, success_command_accepted, success_created_item,
    success_item, success_list, success_no_content, unauthorized, validation,
};
use crate::command_headers::{
    validate_write_payload, AppWriteCommandHeaders, WriteCommandHeaderError,
};
use crate::subject::backend_runtime_subject_from_extension;
pub type CommerceBackendPaymentAdminFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CommerceServiceError>> + Send + 'a>>;
use sdkwork_payment_repository_sqlx::WEBHOOK_STORED_REPLAY_MAX_RETRIES;
/// C15 修复：webhook 重放结果，用于 handler 区分 404/409/200。
#[derive(Debug, Clone, Serialize)]
pub enum WebhookReplayResult {
    Queued(serde_json::Value),
    NotFound,
    LimitExceeded { current_retries: i64 },
}
pub trait CommerceBackendPaymentAdminStore: Send + Sync {
    fn list_payment_methods<'a>(
        &'a self,
        query: BackendPaymentMethodListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendPaymentMethodListPage>;
    fn upsert_payment_method<'a>(
        &'a self,
        command: UpsertBackendPaymentMethodCommand,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendPaymentMethodView>;
    fn list_provider_accounts<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage>;
    fn upsert_provider_account<'a>(
        &'a self,
        payload: BackendProviderAccountPayload,
    ) -> CommerceBackendPaymentAdminFuture<'a, serde_json::Value>;
    fn provider_account_ready_for_activation<'a>(
        &'a self,
        scope: BackendTenantScope,
        provider_account_id: String,
    ) -> CommerceBackendPaymentAdminFuture<'a, bool>;
    /// Soft-deletes a provider account (marks deleted_at). Returns
    /// `Conflict` while non-deleted channels or sub-merchants still reference
    /// the account, and `NotFound` when the account does not exist or was
    /// already deleted.
    fn delete_provider_account<'a>(
        &'a self,
        scope: BackendTenantScope,
        provider_account_id: String,
    ) -> CommerceBackendPaymentAdminFuture<'a, ()>;
    fn list_channels<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage>;
    fn upsert_channel<'a>(
        &'a self,
        payload: BackendPaymentChannelPayload,
    ) -> CommerceBackendPaymentAdminFuture<'a, serde_json::Value>;
    fn list_route_rules<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage>;
    fn upsert_route_rule<'a>(
        &'a self,
        payload: BackendRouteRulePayload,
    ) -> CommerceBackendPaymentAdminFuture<'a, serde_json::Value>;
    fn delete_route_rule<'a>(
        &'a self,
        scope: BackendTenantScope,
        route_rule_id: String,
    ) -> CommerceBackendPaymentAdminFuture<'a, ()>;
    fn list_attempts<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage>;
    fn list_webhook_events<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage>;
    fn replay_webhook_event<'a>(
        &'a self,
        scope: BackendTenantScope,
        event_id: String,
    ) -> CommerceBackendPaymentAdminFuture<'a, WebhookReplayResult>;
    fn list_reconciliation_runs<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage>;
    fn create_reconciliation_run<'a>(
        &'a self,
        payload: BackendReconciliationRunPayload,
    ) -> CommerceBackendPaymentAdminFuture<'a, serde_json::Value>;
}
#[derive(Clone)]
struct BackendPaymentAdminState {
    store: Arc<dyn CommerceBackendPaymentAdminStore>,
}
#[derive(Debug, Clone)]
pub struct BackendTenantScope {
    pub tenant_id: String,
    pub organization_id: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackendPaymentMethodListParams {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default, rename = "page_size")]
    page_size: Option<i64>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendListQueryParams {
    #[serde(default)]
    page: Option<i64>,
    #[serde(default, rename = "page_size")]
    page_size: Option<i64>,
}
#[derive(Debug, Clone)]
pub struct BackendPaymentMethodListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub status: Option<String>,
    pub offset: i64,
    pub limit: i64,
}
#[derive(Debug, Clone)]
pub struct BackendTenantListQuery {
    pub scope: BackendTenantScope,
    pub offset: i64,
    pub limit: i64,
}
/// Phase 1.3：标准分页结果，store 一次性返回当前页 items + 满足条件的总记录数。
#[derive(Debug, Clone, Serialize)]
pub struct BackendListPage<T> {
    pub items: Vec<T>,
    pub total_items: i64,
}
pub type BackendPaymentMethodListPage = BackendListPage<BackendPaymentMethodView>;
pub type BackendJsonListPage = BackendListPage<serde_json::Value>;
#[derive(Debug, Clone)]
pub struct BackendPaymentMethodView {
    pub id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub method_key: String,
    pub display_name: String,
    pub provider_code: String,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone)]
pub struct UpsertBackendPaymentMethodCommand {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub method_key: String,
    pub display_name: String,
    pub provider_code: String,
    pub status: String,
    pub sort_order: i64,
    pub request_no: String,
    pub idempotency_key: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpsertPaymentMethodBody {
    method_key: Option<String>,
    display_name: Option<String>,
    provider_code: Option<String>,
    status: Option<String>,
    sort_order: Option<i64>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpsertProviderAccountBody {
    account_no: Option<String>,
    provider_code: Option<String>,
    merchant_id: Option<String>,
    account_name: Option<String>,
    environment: Option<String>,
    country_code: Option<String>,
    settlement_currency: Option<String>,
    secret_ref: Option<String>,
    webhook_secret_ref: Option<String>,
    certificate_ref: Option<String>,
    primary_secret: Option<String>,
    webhook_secret: Option<String>,
    certificate: Option<String>,
    account_mode: Option<String>,
    partner_provider_account_id: Option<String>,
    capabilities: Option<Value>,
    metadata: Option<Value>,
    status: Option<String>,
}
#[derive(Clone)]
pub struct BackendProviderAccountPayload {
    pub id: Option<String>,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub account_no: String,
    pub provider_code: Option<String>,
    pub merchant_id: Option<String>,
    pub account_name: Option<String>,
    pub environment: Option<String>,
    pub country_code: Option<String>,
    pub settlement_currency: Option<String>,
    pub secret_ref: Option<String>,
    pub webhook_secret_ref: Option<String>,
    pub certificate_ref: Option<String>,
    pub credential_write: sdkwork_payment_repository_sqlx::ProviderCredentialWrite,
    pub account_mode: Option<String>,
    pub partner_provider_account_id: Option<String>,
    pub capabilities: Option<Value>,
    pub metadata: Option<Value>,
    pub status: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpsertChannelBody {
    channel_no: Option<String>,
    provider_account_id: Option<String>,
    method_id: Option<String>,
    scene_code: Option<String>,
    currency_code: Option<String>,
    country_code: Option<String>,
    status: Option<String>,
    priority: Option<i64>,
}
#[derive(Debug, Clone)]
pub struct BackendPaymentChannelPayload {
    pub id: Option<String>,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub channel_no: String,
    pub provider_account_id: String,
    pub method_id: String,
    pub scene_code: String,
    pub currency_code: String,
    pub country_code: String,
    pub status: String,
    pub priority: i64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpsertRouteRuleBody {
    rule_no: Option<String>,
    priority: Option<i64>,
    purchase_type: Option<String>,
    country_code: Option<String>,
    currency_code: Option<String>,
    client_platform: Option<String>,
    amount_min: Option<String>,
    amount_max: Option<String>,
    user_segment: Option<String>,
    risk_level: Option<String>,
    channel_id: Option<String>,
    status: Option<String>,
    starts_at: Option<String>,
    ends_at: Option<String>,
}
#[derive(Debug, Clone)]
pub struct BackendRouteRulePayload {
    pub id: Option<String>,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub rule_no: String,
    pub priority: i64,
    pub purchase_type: Option<String>,
    pub country_code: Option<String>,
    pub currency_code: Option<String>,
    pub client_platform: Option<String>,
    pub amount_min: Option<String>,
    pub amount_max: Option<String>,
    pub user_segment: Option<String>,
    pub risk_level: Option<String>,
    pub channel_id: String,
    pub status: String,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateReconciliationRunBody {
    provider_code: Option<String>,
    account_id: Option<String>,
    provider_account_id: Option<String>,
    statement_date: Option<String>,
    reconciliation_type: Option<String>,
    period_start: Option<String>,
    period_end: Option<String>,
    currency_code: Option<String>,
}
#[derive(Debug, Clone)]
pub struct BackendReconciliationRunPayload {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub provider_code: String,
    pub provider_account_id: String,
    pub reconciliation_type: String,
    pub period_start: String,
    pub period_end: String,
    pub currency_code: String,
    pub request_no: String,
    pub idempotency_key: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackendPaymentMethodResponse {
    id: String,
    method_key: String,
    display_name: String,
    provider_code: String,
    status: String,
    sort_order: i64,
}
#[derive(Clone)]
struct PostgresBackendPaymentAdminStore {
    pool: PgPool,
}
impl PostgresBackendPaymentAdminStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
impl CommerceBackendPaymentAdminStore for PostgresBackendPaymentAdminStore {
    fn list_payment_methods<'a>(
        &'a self,
        query: BackendPaymentMethodListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendPaymentMethodListPage> {
        Box::pin(async move {
            let rows = sqlx::query(
                r#"
                SELECT id, tenant_id, organization_id, method_key, display_name, provider_code,
                       status, sort_order, created_at, updated_at,
                       COUNT(*) OVER() AS total_count
                FROM commerce_payment_method
                WHERE tenant_id = CAST($1 AS TEXT) AND (organization_id = CAST($2 AS TEXT) OR (organization_id IS NULL AND $2::text IS NULL) OR (organization_id = '0' AND $2::text IS NULL))
                  AND ($3::text IS NULL OR LOWER(COALESCE(status, '')) = LOWER($3::text))
                ORDER BY sort_order ASC, created_at ASC
                LIMIT $4 OFFSET $5
                "#,
            )
            .bind(&query.tenant_id)
            .bind(query.organization_id.as_deref())
            .bind(query.status.as_deref())
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| {
                CommerceServiceError::storage(format!("failed to list payment methods: {error}"))
            })?;
            let total_items = pg_total_count(&rows);
            let items = rows.iter().map(map_method_row_pg).collect();
            Ok(BackendPaymentMethodListPage { items, total_items })
        })
    }
    fn upsert_payment_method<'a>(
        &'a self,
        command: UpsertBackendPaymentMethodCommand,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendPaymentMethodView> {
        Box::pin(async move {
            let now = current_timestamp_string();
            // Platform rows persist the sentinel organization scope (`"0"`) so
            // tenant (personal) sessions never write NULL into the NOT NULL
            // `organization_id` column.
            let organization_id = command
                .organization_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("0");
            let id = stable_storage_id(&[
                "payment-method",
                &command.tenant_id,
                organization_id,
                &command.method_key,
            ]);
            let row = sqlx::query(
                r#"
                INSERT INTO commerce_payment_method
                    (id, tenant_id, organization_id, method_key, display_name, provider_code,
                     status, sort_order, request_no, idempotency_key, created_at, updated_at)
                VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, $6, $7, $8, $9, $10, CAST($11 AS TIMESTAMPTZ), CAST($11 AS TIMESTAMPTZ))
                ON CONFLICT (tenant_id, (COALESCE(organization_id, '')), method_key) WHERE deleted_at IS NULL DO UPDATE SET
                    display_name = EXCLUDED.display_name,
                    provider_code = EXCLUDED.provider_code,
                    status = EXCLUDED.status,
                    sort_order = EXCLUDED.sort_order,
                    updated_at = EXCLUDED.updated_at
                RETURNING id, tenant_id, organization_id, method_key, display_name, provider_code,
                          status, sort_order, created_at, updated_at
                "#,
            )
            .bind(&id)
            .bind(&command.tenant_id)
            .bind(organization_id)
            .bind(&command.method_key)
            .bind(&command.display_name)
            .bind(&command.provider_code)
            .bind(&command.status)
            .bind(command.sort_order)
            .bind(&command.request_no)
            .bind(&command.idempotency_key)
            .bind(&now)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| {
                CommerceServiceError::storage(format!("failed to upsert payment method: {error}"))
            })?;
            Ok(map_method_row_pg(&row))
        })
    }
    fn list_provider_accounts<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage> {
        Box::pin(async move {
            let scope = query.scope;
            let rows = sqlx::query(
                r#"
                SELECT id, account_no, provider_code, merchant_id, account_name,
                       account_name_i18n, account_mode,
                       partner_provider_account_id, environment, country_code,
                       settlement_currency, secret_ref, webhook_secret_ref, certificate_ref,
                       capabilities, metadata, status,
                       CAST(last_tested_at AS TEXT) AS last_tested_at, last_test_status,
                       CAST(certificate_expires_at AS TEXT) AS certificate_expires_at,
                       CAST(created_at AS TEXT) AS created_at, CAST(updated_at AS TEXT) AS updated_at,
                       EXISTS (SELECT 1 FROM commerce_payment_provider_credential credential WHERE credential.provider_account_id = commerce_payment_provider_account.id AND credential.tenant_id = commerce_payment_provider_account.tenant_id AND credential.credential_kind = 'primary_secret' AND credential.status = 'active' AND credential.deleted_at IS NULL) AS has_primary_secret,
                       EXISTS (SELECT 1 FROM commerce_payment_provider_credential credential WHERE credential.provider_account_id = commerce_payment_provider_account.id AND credential.tenant_id = commerce_payment_provider_account.tenant_id AND credential.credential_kind = 'webhook_secret' AND credential.status = 'active' AND credential.deleted_at IS NULL) AS has_webhook_secret,
                       EXISTS (SELECT 1 FROM commerce_payment_provider_credential credential WHERE credential.provider_account_id = commerce_payment_provider_account.id AND credential.tenant_id = commerce_payment_provider_account.tenant_id AND credential.credential_kind = 'certificate' AND credential.status = 'active' AND credential.deleted_at IS NULL) AS has_certificate,
                       COUNT(*) OVER() AS total_count
                FROM commerce_payment_provider_account
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND (organization_id = CAST($2 AS TEXT) OR (organization_id IS NULL AND $2::text IS NULL) OR (organization_id = '0' AND $2::text IS NULL))
                  AND deleted_at IS NULL
                ORDER BY created_at DESC, id DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| CommerceServiceError::storage(format!("failed to list provider accounts: {error}")))?;
            let total_items = pg_total_count(&rows);
            let items = rows
                .iter()
                .map(|row| {
                    serde_json::json!({
                        "id": pg_string(row, "id"),
                        "accountNo": pg_string(row, "account_no"),
                        "providerCode": pg_string(row, "provider_code"),
                        "merchantId": pg_string(row, "merchant_id"),
                        "accountName": pg_optional_string(row, "account_name"),
                        "accountNameI18n": pg_json(row, "account_name_i18n"),
                        "accountMode": pg_string(row, "account_mode"),
                        "partnerProviderAccountId": pg_optional_string(row, "partner_provider_account_id"),
                        "environment": pg_string(row, "environment"),
                        "countryCode": pg_string(row, "country_code"),
                        "settlementCurrency": pg_string(row, "settlement_currency"),
                        "hasPrimarySecret": pg_bool(row, "has_primary_secret"),
                        "hasWebhookSecret": pg_bool(row, "has_webhook_secret"),
                        "hasCertificate": pg_bool(row, "has_certificate"),
                        "credentialStorage": credential_storage(&pg_string(row, "secret_ref")),
                        "capabilities": pg_json(row, "capabilities"),
                        "metadata": pg_json(row, "metadata"),
                        "status": pg_string(row, "status"),
                        "lastTestedAt": pg_optional_string(row, "last_tested_at"),
                        "lastTestStatus": pg_optional_string(row, "last_test_status"),
                        "certificateExpiresAt": pg_optional_string(row, "certificate_expires_at"),
                        "createdAt": pg_string(row, "created_at"),
                        "updatedAt": pg_string(row, "updated_at"),
                    })
                })
                .collect();
            Ok(BackendJsonListPage { items, total_items })
        })
    }
    fn upsert_provider_account<'a>(
        &'a self,
        payload: BackendProviderAccountPayload,
    ) -> CommerceBackendPaymentAdminFuture<'a, serde_json::Value> {
        Box::pin(async move {
            let id = payload.id.clone().unwrap_or_else(|| {
                stable_storage_id(&["provider-account", &payload.tenant_id, &payload.account_no])
            });
            let now = current_timestamp_string();
            let capabilities = payload.capabilities.as_ref().map(Value::to_string);
            let metadata = payload.metadata.as_ref().map(Value::to_string);
            let credential_write = payload.credential_write.clone();
            // Platform rows persist the sentinel organization scope (`"0"`) so
            // tenant (personal) sessions never write NULL into the NOT NULL
            // `organization_id` column.
            let organization_id = payload
                .organization_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("0");
            let row = if payload.id.is_some() {
                sqlx::query(
                    r#"
                    UPDATE commerce_payment_provider_account SET
                        provider_code = COALESCE($1, provider_code),
                        merchant_id = COALESCE($2, merchant_id),
                        account_name = COALESCE($3, account_name),
                        account_mode = COALESCE($4, account_mode),
                        partner_provider_account_id = COALESCE($5, partner_provider_account_id),
                        environment = COALESCE($6, environment),
                        country_code = COALESCE($7, country_code),
                        settlement_currency = COALESCE($8, settlement_currency),
                        secret_ref = COALESCE($9, secret_ref),
                        webhook_secret_ref = COALESCE($10, webhook_secret_ref),
                        certificate_ref = COALESCE($11, certificate_ref),
                        capabilities = COALESCE(CAST($12 AS JSONB), capabilities),
                        metadata = COALESCE(CAST($13 AS JSONB), metadata),
                        status = COALESCE($14, status),
                        version = version + 1,
                        updated_at = CAST($15 AS TIMESTAMPTZ)
                    WHERE id = CAST($16 AS TEXT)
                      AND tenant_id = CAST($17 AS TEXT)
                      AND ((organization_id = CAST($18 AS TEXT)) OR (organization_id IS NULL AND $18 IS NULL) OR (organization_id = '0' AND $18 IS NULL))
                      AND deleted_at IS NULL
                    RETURNING id, account_no, provider_code, merchant_id, account_name,
                              account_name_i18n, account_mode,
                              partner_provider_account_id, environment, country_code, settlement_currency,
                              secret_ref, webhook_secret_ref, certificate_ref, capabilities, metadata,
                              status, created_at, updated_at
                    "#,
                )
                .bind(payload.provider_code.as_deref())
                .bind(payload.merchant_id.as_deref())
                .bind(payload.account_name.as_deref())
                .bind(payload.account_mode.as_deref())
                .bind(payload.partner_provider_account_id.as_deref())
                .bind(payload.environment.as_deref())
                .bind(payload.country_code.as_deref())
                .bind(payload.settlement_currency.as_deref())
                .bind(payload.secret_ref.as_deref())
                .bind(payload.webhook_secret_ref.as_deref())
                .bind(payload.certificate_ref.as_deref())
                .bind(capabilities.as_deref())
                .bind(metadata.as_deref())
                .bind(payload.status.as_deref())
                .bind(&now)
                .bind(&id)
                .bind(&payload.tenant_id)
                .bind(organization_id)
                .fetch_one(&self.pool)
                .await
            } else {
                sqlx::query(
                    r#"
                    INSERT INTO commerce_payment_provider_account
                        (id, tenant_id, organization_id, account_no, provider_code, merchant_id, account_name, account_mode,
                         partner_provider_account_id, environment, country_code, settlement_currency, secret_ref,
                         webhook_secret_ref, certificate_ref, capabilities, metadata, status, created_at, updated_at)
                    VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, CAST($16 AS JSONB), CAST($17 AS JSONB), $18, CAST($19 AS TIMESTAMPTZ), CAST($19 AS TIMESTAMPTZ))
                    ON CONFLICT (tenant_id, (COALESCE(organization_id, '')), account_no)
                    WHERE deleted_at IS NULL DO UPDATE SET
                        provider_code = EXCLUDED.provider_code,
                        merchant_id = EXCLUDED.merchant_id,
                        account_name = EXCLUDED.account_name,
                        account_mode = EXCLUDED.account_mode,
                        partner_provider_account_id = EXCLUDED.partner_provider_account_id,
                        environment = EXCLUDED.environment,
                        country_code = EXCLUDED.country_code,
                        settlement_currency = EXCLUDED.settlement_currency,
                        secret_ref = EXCLUDED.secret_ref,
                        webhook_secret_ref = EXCLUDED.webhook_secret_ref,
                        certificate_ref = EXCLUDED.certificate_ref,
                        capabilities = EXCLUDED.capabilities,
                        metadata = EXCLUDED.metadata,
                        status = EXCLUDED.status,
                        version = commerce_payment_provider_account.version + 1,
                        updated_at = EXCLUDED.updated_at
                    RETURNING id, account_no, provider_code, merchant_id, account_name,
                              account_name_i18n, account_mode,
                              partner_provider_account_id, environment, country_code, settlement_currency,
                              secret_ref, webhook_secret_ref, certificate_ref, capabilities, metadata,
                              status, created_at, updated_at
                    "#,
                )
                .bind(&id)
                .bind(&payload.tenant_id)
                .bind(organization_id)
                .bind(&payload.account_no)
                .bind(payload.provider_code.as_deref())
                .bind(payload.merchant_id.as_deref())
                .bind(payload.account_name.as_deref())
                .bind(payload.account_mode.as_deref())
                .bind(payload.partner_provider_account_id.as_deref())
                .bind(payload.environment.as_deref())
                .bind(payload.country_code.as_deref())
                .bind(payload.settlement_currency.as_deref())
                .bind(payload.secret_ref.as_deref())
                .bind(payload.webhook_secret_ref.as_deref())
                .bind(payload.certificate_ref.as_deref())
                .bind(capabilities.as_deref())
                .bind(metadata.as_deref())
                .bind(payload.status.as_deref())
                .bind(&now)
                .fetch_one(&self.pool)
                .await
            }
            .map_err(|error| CommerceServiceError::storage(format!("failed to upsert provider account: {error}")))?;
            sdkwork_payment_repository_sqlx::rotate_provider_credentials_postgres(
                &self.pool,
                &payload.tenant_id,
                Some(organization_id),
                &id,
                credential_write,
            )
            .await?;
            Ok(pg_provider_account_value(&row))
        })
    }
    fn provider_account_ready_for_activation<'a>(
        &'a self,
        scope: BackendTenantScope,
        provider_account_id: String,
    ) -> CommerceBackendPaymentAdminFuture<'a, bool> {
        Box::pin(async move {
            let ready = sqlx::query(
                r#"
                SELECT 1
                FROM commerce_payment_provider_account
                WHERE id = CAST($1 AS TEXT)
                  AND tenant_id = CAST($2 AS TEXT)
                  AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL))
                  AND last_test_status = 'success'
                  AND last_tested_at IS NOT NULL
                  AND updated_at IS NOT NULL
                  AND last_tested_at >= updated_at
                  AND COALESCE((metadata->>'configureBeforeActivation')::boolean, false) = false
                  AND NOT EXISTS (
                      SELECT 1
                      FROM commerce_payment_provider_account active_account
                      WHERE active_account.tenant_id = commerce_payment_provider_account.tenant_id
                        AND active_account.organization_id IS NOT DISTINCT FROM commerce_payment_provider_account.organization_id
                        AND LOWER(active_account.provider_code) = LOWER(commerce_payment_provider_account.provider_code)
                        AND active_account.id <> commerce_payment_provider_account.id
                        AND active_account.status = 'active'
                        AND active_account.deleted_at IS NULL
                  )
                  AND deleted_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(&provider_account_id)
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                CommerceServiceError::storage(format!(
                    "failed to validate provider account activation readiness: {error}"
                ))
            })?;
            Ok(ready.is_some())
        })
    }
    fn delete_provider_account<'a>(
        &'a self,
        scope: BackendTenantScope,
        provider_account_id: String,
    ) -> CommerceBackendPaymentAdminFuture<'a, ()> {
        Box::pin(async move {
            let channel_reference = sqlx::query(
                r#"
                SELECT 1
                FROM commerce_payment_channel
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND (organization_id = CAST($2 AS TEXT) OR (organization_id IS NULL AND $2::text IS NULL) OR (organization_id = '0' AND $2::text IS NULL))
                  AND provider_account_id = CAST($3 AS TEXT)
                  AND deleted_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .bind(&provider_account_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                CommerceServiceError::storage(format!(
                    "failed to check provider account channel references: {error}"
                ))
            })?;
            if channel_reference.is_some() {
                return Err(CommerceServiceError::conflict(
                    "provider account is referenced by payment channels; unbind them before deleting the account",
                ));
            }
            let sub_merchant_reference = sqlx::query(
                r#"
                SELECT 1
                FROM commerce_payment_sub_merchant
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND organization_id = CAST($2 AS TEXT)
                  AND provider_account_id = CAST($3 AS TEXT)
                  AND deleted_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .bind(&provider_account_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                CommerceServiceError::storage(format!(
                    "failed to check provider account sub-merchant references: {error}"
                ))
            })?;
            if sub_merchant_reference.is_some() {
                return Err(CommerceServiceError::conflict(
                    "provider account is referenced by sub-merchants; unbind them before deleting the account",
                ));
            }
            let result = sqlx::query(
                r#"
                UPDATE commerce_payment_provider_account
                SET deleted_at = $1
                WHERE id = CAST($2 AS TEXT)
                  AND tenant_id = CAST($3 AS TEXT)
                  AND (organization_id = CAST($4 AS TEXT) OR (organization_id IS NULL AND $4::text IS NULL) OR (organization_id = '0' AND $4::text IS NULL))
                  AND deleted_at IS NULL
                "#,
            )
            .bind(current_timestamp_string())
            .bind(&provider_account_id)
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .execute(&self.pool)
            .await
            .map_err(|error| {
                CommerceServiceError::storage(format!("failed to delete provider account: {error}"))
            })?;
            if result.rows_affected() == 0 {
                return Err(CommerceServiceError::not_found("provider account not found"));
            }
            Ok(())
        })
    }
    fn list_channels<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage> {
        Box::pin(async move {
            let scope = query.scope;
            let rows = sqlx::query(
                r#"
                SELECT id, channel_no, provider_account_id, method_id, scene_code, currency_code,
                       country_code, status, priority, COUNT(*) OVER() AS total_count
                FROM commerce_payment_channel
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND (organization_id = CAST($2 AS TEXT) OR (organization_id IS NULL AND $2::text IS NULL) OR (organization_id = '0' AND $2::text IS NULL))
                ORDER BY priority ASC, created_at ASC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| CommerceServiceError::storage(format!("failed to list channels: {error}")))?;
            let total_items = pg_total_count(&rows);
            let items = rows
                .iter()
                .map(|row| {
                    serde_json::json!({
                        "id": pg_string(row, "id"),
                        "channelNo": pg_string(row, "channel_no"),
                        "providerAccountId": pg_string(row, "provider_account_id"),
                        "methodId": pg_string(row, "method_id"),
                        "sceneCode": pg_string(row, "scene_code"),
                        "currencyCode": pg_string(row, "currency_code"),
                        "countryCode": pg_string(row, "country_code"),
                        "status": pg_string(row, "status"),
                        "priority": row.try_get::<i32,_>("priority").map(i64::from).unwrap_or(0),
                    })
                })
                .collect();
            Ok(BackendJsonListPage { items, total_items })
        })
    }
    fn upsert_channel<'a>(
        &'a self,
        payload: BackendPaymentChannelPayload,
    ) -> CommerceBackendPaymentAdminFuture<'a, serde_json::Value> {
        Box::pin(async move {
            let id = payload.id.clone().unwrap_or_else(|| {
                stable_storage_id(&["payment-channel", &payload.tenant_id, &payload.channel_no])
            });
            let now = current_timestamp_string();
            // Platform rows persist the sentinel organization scope (`"0"`) so
            // tenant (personal) sessions never write NULL into the NOT NULL
            // `organization_id` column.
            let organization_id = payload
                .organization_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("0");
            // `commerce_payment_channel.provider_code` is NOT NULL and the
            // channel API does not carry it; the channel inherits the code of
            // its bound provider account.
            let provider_code = sqlx::query_scalar::<_, String>(
                "SELECT provider_code FROM commerce_payment_provider_account WHERE id = CAST($1 AS TEXT) AND tenant_id = CAST($2 AS TEXT) AND deleted_at IS NULL",
            )
            .bind(&payload.provider_account_id)
            .bind(&payload.tenant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                CommerceServiceError::storage(format!(
                    "failed to resolve provider code for payment channel: {error}"
                ))
            })?
            .ok_or_else(|| {
                CommerceServiceError::not_found(
                    "payment provider account was not found for payment channel",
                )
            })?;
            let row = sqlx::query(
                r#"
                INSERT INTO commerce_payment_channel
                    (id, tenant_id, organization_id, channel_no, provider_code, provider_account_id, method_id, scene_code, currency_code, country_code, status, priority, created_at, updated_at)
                VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, CAST($6 AS TEXT), CAST($7 AS TEXT), $8, $9, $10, $11, $12, CAST($13 AS TIMESTAMPTZ), CAST($13 AS TIMESTAMPTZ))
                ON CONFLICT (tenant_id, (COALESCE(organization_id, '')), channel_no) WHERE deleted_at IS NULL DO UPDATE SET
                    provider_code = EXCLUDED.provider_code,
                    provider_account_id = EXCLUDED.provider_account_id,
                    method_id = EXCLUDED.method_id,
                    scene_code = EXCLUDED.scene_code,
                    currency_code = EXCLUDED.currency_code,
                    country_code = EXCLUDED.country_code,
                    status = EXCLUDED.status,
                    priority = EXCLUDED.priority,
                    updated_at = EXCLUDED.updated_at
                RETURNING id, channel_no, provider_code, provider_account_id, method_id, scene_code, currency_code, country_code, status, priority
                "#,
            )
            .bind(&id)
            .bind(&payload.tenant_id)
            .bind(organization_id)
            .bind(&payload.channel_no)
            .bind(&provider_code)
            .bind(&payload.provider_account_id)
            .bind(&payload.method_id)
            .bind(&payload.scene_code)
            .bind(&payload.currency_code)
            .bind(&payload.country_code)
            .bind(&payload.status)
            .bind(payload.priority)
            .bind(&now)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| CommerceServiceError::storage(format!("failed to upsert channel: {error}")))?;
            Ok(serde_json::json!({
                "id": pg_string(&row, "id"),
                "channelNo": pg_string(&row, "channel_no"),
                "providerAccountId": pg_string(&row, "provider_account_id"),
                "methodId": pg_string(&row, "method_id"),
                "sceneCode": pg_string(&row, "scene_code"),
                "currencyCode": pg_string(&row, "currency_code"),
                "countryCode": pg_string(&row, "country_code"),
                "status": pg_string(&row, "status"),
                "priority": row.try_get::<i32,_>("priority").map(i64::from).unwrap_or(0),
            }))
        })
    }
    fn list_route_rules<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage> {
        Box::pin(async move {
            let scope = query.scope;
            let rows = sqlx::query(
                r#"
                SELECT id, rule_no, priority, purchase_type, country_code, currency_code, client_platform,
                       amount_min, amount_max, user_segment, risk_level, channel_id, status, starts_at,
                       ends_at, COUNT(*) OVER() AS total_count
                FROM commerce_payment_route_rule
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND (organization_id = CAST($2 AS TEXT) OR (organization_id IS NULL AND $2::text IS NULL) OR (organization_id = '0' AND $2::text IS NULL))
                ORDER BY priority ASC, created_at ASC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| CommerceServiceError::storage(format!("failed to list route rules: {error}")))?;
            let total_items = pg_total_count(&rows);
            let items = rows.iter().map(map_route_rule_pg).collect();
            Ok(BackendJsonListPage { items, total_items })
        })
    }
    fn upsert_route_rule<'a>(
        &'a self,
        payload: BackendRouteRulePayload,
    ) -> CommerceBackendPaymentAdminFuture<'a, serde_json::Value> {
        Box::pin(async move {
            let id = payload.id.clone().unwrap_or_else(|| {
                stable_storage_id(&["payment-route-rule", &payload.tenant_id, &payload.rule_no])
            });
            let now = current_timestamp_string();
            // Platform rows persist the sentinel organization scope (`"0"`) so
            // tenant (personal) sessions never write NULL into the NOT NULL
            // `organization_id` column.
            let organization_id = payload
                .organization_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("0");
            let row = sqlx::query(
                r#"
                INSERT INTO commerce_payment_route_rule
                    (id, tenant_id, organization_id, rule_no, priority, purchase_type, country_code, currency_code, client_platform, amount_min, amount_max, user_segment, risk_level, channel_id, status, starts_at, ends_at, created_at, updated_at)
                VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, $6, $7, $8, $9, $10, $11, $12, $13, CAST($14 AS TEXT), $15, $16, $17, CAST($18 AS TIMESTAMPTZ), CAST($18 AS TIMESTAMPTZ))
                ON CONFLICT (tenant_id, (COALESCE(organization_id, '')), rule_no) WHERE deleted_at IS NULL DO UPDATE SET
                    priority = EXCLUDED.priority,
                    purchase_type = EXCLUDED.purchase_type,
                    country_code = EXCLUDED.country_code,
                    currency_code = EXCLUDED.currency_code,
                    client_platform = EXCLUDED.client_platform,
                    amount_min = EXCLUDED.amount_min,
                    amount_max = EXCLUDED.amount_max,
                    user_segment = EXCLUDED.user_segment,
                    risk_level = EXCLUDED.risk_level,
                    channel_id = EXCLUDED.channel_id,
                    status = EXCLUDED.status,
                    starts_at = EXCLUDED.starts_at,
                    ends_at = EXCLUDED.ends_at,
                    updated_at = EXCLUDED.updated_at
                RETURNING id, rule_no, priority, purchase_type, country_code, currency_code, client_platform, amount_min, amount_max, user_segment, risk_level, channel_id, status, starts_at, ends_at
                "#,
            )
            .bind(&id)
            .bind(&payload.tenant_id)
            .bind(organization_id)
            .bind(&payload.rule_no)
            .bind(payload.priority)
            .bind(payload.purchase_type.as_deref())
            .bind(payload.country_code.as_deref())
            .bind(payload.currency_code.as_deref())
            .bind(payload.client_platform.as_deref())
            .bind(payload.amount_min.as_deref())
            .bind(payload.amount_max.as_deref())
            .bind(payload.user_segment.as_deref())
            .bind(payload.risk_level.as_deref())
            .bind(&payload.channel_id)
            .bind(&payload.status)
            .bind(payload.starts_at.as_deref())
            .bind(payload.ends_at.as_deref())
            .bind(&now)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| CommerceServiceError::storage(format!("failed to upsert route rule: {error}")))?;
            Ok(map_route_rule_pg(&row))
        })
    }
    fn delete_route_rule<'a>(
        &'a self,
        scope: BackendTenantScope,
        route_rule_id: String,
    ) -> CommerceBackendPaymentAdminFuture<'a, ()> {
        Box::pin(async move {
            sqlx::query(
                "DELETE FROM commerce_payment_route_rule WHERE id = CAST($1 AS TEXT) AND tenant_id = CAST($2 AS TEXT) AND (organization_id = CAST($3 AS TEXT) OR (organization_id IS NULL AND $3::text IS NULL) OR (organization_id = '0' AND $3::text IS NULL))",
            )
                .bind(&route_rule_id)
                .bind(&scope.tenant_id)
                .bind(scope.organization_id.as_deref())
                .execute(&self.pool)
                .await
                .map_err(|error| {
                    CommerceServiceError::storage(format!("failed to delete route rule: {error}"))
                })?;
            Ok(())
        })
    }
    fn list_attempts<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage> {
        Box::pin(async move {
            let scope = query.scope;
            let rows = sqlx::query(
                r#"
                SELECT id, payment_intent_id, attempt_no, provider_code, channel_id, amount, currency_code,
                       status, provider_transaction_id, created_at, COUNT(*) OVER() AS total_count
                FROM commerce_payment_attempt
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND (organization_id = CAST($2 AS TEXT) OR (organization_id IS NULL AND $2::text IS NULL) OR (organization_id = '0' AND $2::text IS NULL))
                ORDER BY created_at DESC, id DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| CommerceServiceError::storage(format!("failed to list attempts: {error}")))?;
            let total_items = pg_total_count(&rows);
            let items = rows.iter().map(map_attempt_pg).collect();
            Ok(BackendJsonListPage { items, total_items })
        })
    }
    fn list_webhook_events<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage> {
        Box::pin(async move {
            let scope = query.scope;
            let rows = sqlx::query(
                r#"
                SELECT id, event_id, provider_code, event_type, status, received_at, processed_at, retries,
                       COUNT(*) OVER() AS total_count
                FROM commerce_payment_webhook_event
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND (organization_id = CAST($2 AS TEXT) OR (organization_id IS NULL AND $2::text IS NULL) OR (organization_id = '0' AND $2::text IS NULL))
                ORDER BY received_at DESC, id DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| CommerceServiceError::storage(format!("failed to list webhook events: {error}")))?;
            let total_items = pg_total_count(&rows);
            let items = rows.iter().map(map_webhook_event_pg).collect();
            Ok(BackendJsonListPage { items, total_items })
        })
    }
    fn replay_webhook_event<'a>(
        &'a self,
        scope: BackendTenantScope,
        event_id: String,
    ) -> CommerceBackendPaymentAdminFuture<'a, WebhookReplayResult> {
        Box::pin(async move {
            use sdkwork_payment_repository_sqlx::{
                replay_stored_webhook_event_postgres, StoredWebhookReplayResult,
                WebhookStoredReplayScope,
            };
            let replay_scope = WebhookStoredReplayScope {
                tenant_id: scope.tenant_id,
                organization_id: scope.organization_id,
            };
            match replay_stored_webhook_event_postgres(&self.pool, replay_scope, event_id).await? {
                StoredWebhookReplayResult::Applied { webhook_event, .. } => {
                    Ok(WebhookReplayResult::Queued(webhook_event.to_json()))
                }
                StoredWebhookReplayResult::NotFound => Ok(WebhookReplayResult::NotFound),
                StoredWebhookReplayResult::LimitExceeded { current_retries } => {
                    Ok(WebhookReplayResult::LimitExceeded { current_retries })
                }
            }
        })
    }
    fn list_reconciliation_runs<'a>(
        &'a self,
        query: BackendTenantListQuery,
    ) -> CommerceBackendPaymentAdminFuture<'a, BackendJsonListPage> {
        Box::pin(async move {
            let scope = query.scope;
            let rows = sqlx::query(
                r#"
                SELECT id, run_no, provider_code, provider_account_id, reconciliation_type, period_start,
                       period_end, status, matched_count, mismatched_count, currency_code, created_at,
                       COUNT(*) OVER() AS total_count
                FROM commerce_payment_reconciliation_run
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND (organization_id = CAST($2 AS TEXT) OR (organization_id IS NULL AND $2::text IS NULL) OR (organization_id = '0' AND $2::text IS NULL))
                ORDER BY created_at DESC, id DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(&scope.tenant_id)
            .bind(scope.organization_id.as_deref())
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| CommerceServiceError::storage(format!("failed to list reconciliation runs: {error}")))?;
            let total_items = pg_total_count(&rows);
            let items = rows.iter().map(map_reconciliation_run_pg).collect();
            Ok(BackendJsonListPage { items, total_items })
        })
    }
    fn create_reconciliation_run<'a>(
        &'a self,
        payload: BackendReconciliationRunPayload,
    ) -> CommerceBackendPaymentAdminFuture<'a, serde_json::Value> {
        Box::pin(async move {
            let now = current_timestamp_string();
            let id = stable_storage_id(&[
                "reconciliation-run",
                &payload.tenant_id,
                &payload.provider_account_id,
                &payload.period_start,
            ]);
            let run_no =
                stable_storage_id(&["recon", &payload.provider_code, &payload.period_start]);
            let row = sqlx::query(
                "INSERT INTO commerce_payment_reconciliation_run (id, tenant_id, organization_id, run_no, provider_code, provider_account_id, reconciliation_type, period_start, period_end, status, matched_count, mismatched_count, unmatched_count, total_difference_amount, currency_code, request_no, idempotency_key, created_at, updated_at) VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, CAST($6 AS TEXT), $7, CAST($8 AS TIMESTAMPTZ), CAST($9 AS TIMESTAMPTZ), 'queued', 0, 0, 0, '0', $10, $11, $12, CAST($13 AS TIMESTAMPTZ), CAST($13 AS TIMESTAMPTZ)) RETURNING id, run_no, provider_code, provider_account_id, reconciliation_type, period_start, period_end, status, matched_count, mismatched_count, currency_code, created_at",
            )
            .bind(&id)
            .bind(&payload.tenant_id)
            .bind(
                payload
                    .organization_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("0"),
            )
            .bind(&run_no)
            .bind(&payload.provider_code)
            .bind(&payload.provider_account_id)
            .bind(&payload.reconciliation_type)
            .bind(&payload.period_start)
            .bind(&payload.period_end)
            .bind(&payload.currency_code)
            .bind(&payload.request_no)
            .bind(&payload.idempotency_key)
            .bind(&now)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| CommerceServiceError::storage(format!("failed to create reconciliation run: {error}")))?;
            Ok(map_reconciliation_run_pg(&row))
        })
    }
}
pub fn backend_payment_admin_router_with_postgres_pool(pool: PgPool) -> Router {
    build_backend_payment_admin_router(Arc::new(PostgresBackendPaymentAdminStore::new(pool)))
}
pub fn build_backend_payment_admin_router(
    store: Arc<dyn CommerceBackendPaymentAdminStore>,
) -> Router {
    Router::new()
        .route(
            "/backend/v3/api/payments/methods",
            get(list_methods).post(create_method),
        )
        .route(
            "/backend/v3/api/payments/methods/{methodKey}",
            patch(update_method),
        )
        .route(
            "/backend/v3/api/payments/provider_accounts",
            get(list_provider_accounts).post(create_provider_account),
        )
        .route(
            "/backend/v3/api/payments/provider_accounts/{providerAccountId}",
            patch(update_provider_account).delete(delete_provider_account),
        )
        .route(
            "/backend/v3/api/payments/channels",
            get(list_channels).post(create_channel),
        )
        .route(
            "/backend/v3/api/payments/route_rules",
            get(list_route_rules).post(create_route_rule),
        )
        .route(
            "/backend/v3/api/payments/route_rules/{routeRuleId}",
            patch(update_route_rule).delete(delete_route_rule),
        )
        .route("/backend/v3/api/payments/attempts", get(list_attempts))
        .route(
            "/backend/v3/api/payments/webhook_events",
            get(list_webhook_events),
        )
        .route(
            "/backend/v3/api/payments/webhook_events/{eventId}/replay",
            post(replay_webhook_event),
        )
        .route(
            "/backend/v3/api/payments/reconciliation_runs",
            get(list_reconciliation_runs).post(create_reconciliation_run),
        )
        .with_state(BackendPaymentAdminState { store })
}
async fn list_methods(
    State(state): State<BackendPaymentAdminState>,
    Query(params): Query<BackendPaymentMethodListParams>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let query = BackendPaymentMethodListQuery {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        status: params.status,
        offset: page_params.offset,
        limit: page_params.page_size,
    };
    match state.store.list_payment_methods(query).await {
        Ok(page) => {
            let items: Vec<_> = page.items.into_iter().map(map_method).collect();
            success_list(ctx, items, page.total_items, page_params)
        }
        Err(error) => {
            backend_payment_error_response(ctx, "payment method list is unavailable", error)
        }
    }
}
async fn create_method(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<UpsertPaymentMethodBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let write_headers =
        match validate_backend_write_payload(ctx, &headers, "payment-method-upsert", &body, "pm") {
            Ok(headers) => headers,
            Err(response) => return response,
        };
    let method_key = match require_trimmed_string(ctx, body.method_key, "methodKey") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let display_name = match require_trimmed_string(ctx, body.display_name, "displayName") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let provider_code = match require_trimmed_string(ctx, body.provider_code, "providerCode") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let status = body.status.unwrap_or_else(|| "active".to_owned());
    let sort_order = body.sort_order.unwrap_or(0);
    let command = UpsertBackendPaymentMethodCommand {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        method_key,
        display_name,
        provider_code,
        status,
        sort_order,
        request_no: write_headers.request_no,
        idempotency_key: write_headers.idempotency_key,
    };
    match state.store.upsert_payment_method(command).await {
        Ok(view) => success_created_item(ctx, map_method(view)),
        Err(error) => backend_payment_error_response(ctx, "payment method upsert failed", error),
    }
}
async fn update_method(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Path(method_key): Path<String>,
    Json(body): Json<UpsertPaymentMethodBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let write_headers =
        match validate_backend_write_payload(ctx, &headers, "payment-method-upsert", &body, "pm") {
            Ok(headers) => headers,
            Err(response) => return response,
        };
    let display_name = match require_trimmed_string(ctx, body.display_name, "displayName") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let provider_code = match require_trimmed_string(ctx, body.provider_code, "providerCode") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let command = UpsertBackendPaymentMethodCommand {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        method_key,
        display_name,
        provider_code,
        status: body.status.unwrap_or_else(|| "active".to_owned()),
        sort_order: body.sort_order.unwrap_or(0),
        request_no: write_headers.request_no,
        idempotency_key: write_headers.idempotency_key,
    };
    match state.store.upsert_payment_method(command).await {
        Ok(view) => success_item(ctx, map_method(view)),
        Err(error) => backend_payment_error_response(ctx, "payment method upsert failed", error),
    }
}
async fn list_provider_accounts(
    State(state): State<BackendPaymentAdminState>,
    Query(params): Query<BackendListQueryParams>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let query = BackendTenantListQuery {
        scope: BackendTenantScope {
            tenant_id: subject.tenant_id,
            organization_id: subject.organization_id,
        },
        offset: page_params.offset,
        limit: page_params.page_size,
    };
    match state.store.list_provider_accounts(query).await {
        Ok(page) => success_list(ctx, page.items, page.total_items, page_params),
        Err(error) => backend_payment_error_response(
            ctx,
            "payment provider account list is unavailable",
            error,
        ),
    }
}
async fn create_provider_account(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<UpsertProviderAccountBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    upsert_provider_account_inner(state, runtime_context, ctx, headers, None, body).await
}
async fn update_provider_account(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Path(provider_account_id): Path<String>,
    Json(body): Json<UpsertProviderAccountBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    upsert_provider_account_inner(
        state,
        runtime_context,
        ctx,
        headers,
        Some(provider_account_id),
        body,
    )
    .await
}
async fn delete_provider_account(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(provider_account_id): Path<String>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let scope = BackendTenantScope {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
    };
    match state
        .store
        .delete_provider_account(scope, provider_account_id)
        .await
    {
        Ok(()) => success_no_content(ctx),
        Err(error) => {
            backend_payment_error_response(ctx, "payment provider account delete failed", error)
        }
    }
}
async fn upsert_provider_account_inner(
    state: BackendPaymentAdminState,
    runtime_context: Option<Extension<IamAppContext>>,
    ctx: Option<&WebRequestContext>,
    headers: HeaderMap,
    provider_account_id: Option<String>,
    body: UpsertProviderAccountBody,
) -> Response {
    let is_create = provider_account_id.is_none();
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let _write_headers = match validate_backend_write_payload(
        ctx,
        &headers,
        "payment-provider-account-upsert",
        &body,
        "provider-account",
    ) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let requests_activation = body
        .status
        .as_deref()
        .is_some_and(|status| status.trim().eq_ignore_ascii_case("active"));
    if requests_activation && is_create {
        return conflict(
            ctx,
            "create the provider account as inactive, save its configuration, pass dry-run validation, then activate it",
        );
    }
    if requests_activation {
        let is_status_only_patch = body.account_no.is_none()
            && body.provider_code.is_none()
            && body.merchant_id.is_none()
            && body.account_name.is_none()
            && body.environment.is_none()
            && body.country_code.is_none()
            && body.settlement_currency.is_none()
            && body.secret_ref.is_none()
            && body.webhook_secret_ref.is_none()
            && body.certificate_ref.is_none()
            && body.primary_secret.is_none()
            && body.webhook_secret.is_none()
            && body.certificate.is_none()
            && body.account_mode.is_none()
            && body.partner_provider_account_id.is_none()
            && body.capabilities.is_none()
            && body.metadata.is_none();
        if !is_status_only_patch {
            return conflict(
                ctx,
                "provider account activation must be a status-only patch after configuration is saved and tested",
            );
        }
        let provider_account_id = provider_account_id
            .as_ref()
            .expect("create activation is rejected above")
            .clone();
        let scope = BackendTenantScope {
            tenant_id: subject.tenant_id.clone(),
            organization_id: subject.organization_id.clone(),
        };
        match state
            .store
            .provider_account_ready_for_activation(scope, provider_account_id)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                return conflict(
                    ctx,
                    "provider account configuration is untested, stale, or still contains mock values; run a successful dry-run before activation",
                )
            }
            Err(error) => {
                return backend_payment_error_response(
                    ctx,
                    "payment provider account readiness validation failed",
                    error,
                )
            }
        }
    }
    let account_no = match body.account_no {
        Some(value) => match require_trimmed_string(ctx, Some(value), "accountNo") {
            Ok(value) => value,
            Err(response) => return response,
        },
        None => match provider_account_id.as_ref() {
            Some(value) => value.clone(),
            None => return validation(ctx, "accountNo is required"),
        },
    };
    let required_for_create = |value: Option<String>, field: &str| {
        let value = value
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if is_create && value.is_none() {
            Err(validation(ctx, format!("{field} is required")))
        } else {
            Ok(value)
        }
    };
    let provider_code = match required_for_create(body.provider_code, "providerCode") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let merchant_id = match required_for_create(body.merchant_id, "merchantId") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let account_name = body
        .account_name
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if account_name
        .as_deref()
        .is_some_and(|value| value.chars().count() > 128)
    {
        return validation(ctx, "accountName must be at most 128 characters");
    }
    let environment = match required_for_create(body.environment, "environment") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let country_code = match required_for_create(body.country_code, "countryCode") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let settlement_currency =
        match required_for_create(body.settlement_currency, "settlementCurrency") {
            Ok(value) => value,
            Err(response) => return response,
        };
    let primary_secret = body
        .primary_secret
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let webhook_secret = body
        .webhook_secret
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let certificate = body
        .certificate
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let legacy_secret_ref = body
        .secret_ref
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if is_create && primary_secret.is_none() && legacy_secret_ref.is_none() {
        return validation(ctx, "primarySecret is required");
    }
    let secret_ref = legacy_secret_ref.or_else(|| {
        primary_secret
            .as_ref()
            .map(|_| "database:primary_secret".to_owned())
    });
    let webhook_secret_ref = body
        .webhook_secret_ref
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            webhook_secret
                .as_ref()
                .map(|_| "database:webhook_secret".to_owned())
        });
    let certificate_ref = body
        .certificate_ref
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            certificate
                .as_ref()
                .map(|_| "database:certificate".to_owned())
        });
    let account_mode = body
        .account_mode
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| is_create.then(|| "direct".to_owned()));
    if account_mode
        .as_deref()
        .is_some_and(|value| !matches!(value, "direct" | "partner"))
    {
        return validation(ctx, "accountMode must be direct or partner");
    }
    let partner_provider_account_id = body
        .partner_provider_account_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if account_mode.as_deref() == Some("partner") && partner_provider_account_id.is_none() {
        return validation(
            ctx,
            "partnerProviderAccountId is required for partner accounts",
        );
    }
    if body
        .capabilities
        .as_ref()
        .is_some_and(|value| !value.is_object())
    {
        return validation(ctx, "capabilities must be a JSON object");
    }
    if body
        .metadata
        .as_ref()
        .is_some_and(|value| !value.is_object())
    {
        return validation(ctx, "metadata must be a JSON object");
    }
    let payload = BackendProviderAccountPayload {
        id: provider_account_id,
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        account_no,
        provider_code,
        merchant_id,
        account_name,
        environment,
        country_code,
        settlement_currency,
        secret_ref,
        webhook_secret_ref,
        certificate_ref,
        credential_write: sdkwork_payment_repository_sqlx::ProviderCredentialWrite {
            primary_secret,
            webhook_secret,
            certificate,
        },
        account_mode,
        partner_provider_account_id,
        capabilities: body.capabilities.or_else(|| {
            is_create.then(
                || serde_json::json!({"pay": true, "refund": true, "close": true, "query": true}),
            )
        }),
        metadata: body
            .metadata
            .or_else(|| is_create.then(|| serde_json::json!({}))),
        status: body
            .status
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or_else(|| is_create.then(|| "inactive".to_owned())),
    };
    match state.store.upsert_provider_account(payload).await {
        Ok(item) if is_create => success_created_item(ctx, item),
        Ok(item) => success_item(ctx, item),
        Err(error) => {
            backend_payment_error_response(ctx, "payment provider account upsert failed", error)
        }
    }
}
async fn list_channels(
    State(state): State<BackendPaymentAdminState>,
    Query(params): Query<BackendListQueryParams>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let query = BackendTenantListQuery {
        scope: BackendTenantScope {
            tenant_id: subject.tenant_id,
            organization_id: subject.organization_id,
        },
        offset: page_params.offset,
        limit: page_params.page_size,
    };
    match state.store.list_channels(query).await {
        Ok(page) => success_list(ctx, page.items, page.total_items, page_params),
        Err(error) => {
            backend_payment_error_response(ctx, "payment channel list is unavailable", error)
        }
    }
}
async fn create_channel(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<UpsertChannelBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let _write_headers = match validate_backend_write_payload(
        ctx,
        &headers,
        "payment-channel-upsert",
        &body,
        "payment-channel",
    ) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let channel_no = match require_trimmed_string(ctx, body.channel_no, "channelNo") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let provider_account_id =
        match require_trimmed_string(ctx, body.provider_account_id, "providerAccountId") {
            Ok(value) => value,
            Err(response) => return response,
        };
    let method_id = match require_trimmed_string(ctx, body.method_id, "methodId") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let scene_code = match require_trimmed_string(ctx, body.scene_code, "sceneCode") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let currency_code = match require_trimmed_string(ctx, body.currency_code, "currencyCode") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let country_code = match require_trimmed_string(ctx, body.country_code, "countryCode") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let payload = BackendPaymentChannelPayload {
        id: None,
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        channel_no,
        provider_account_id,
        method_id,
        scene_code,
        currency_code,
        country_code,
        status: body.status.unwrap_or_else(|| "active".to_owned()),
        priority: body.priority.unwrap_or(0),
    };
    match state.store.upsert_channel(payload).await {
        Ok(item) => success_created_item(ctx, item),
        Err(error) => backend_payment_error_response(ctx, "payment channel upsert failed", error),
    }
}
async fn list_route_rules(
    State(state): State<BackendPaymentAdminState>,
    Query(params): Query<BackendListQueryParams>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let query = BackendTenantListQuery {
        scope: BackendTenantScope {
            tenant_id: subject.tenant_id,
            organization_id: subject.organization_id,
        },
        offset: page_params.offset,
        limit: page_params.page_size,
    };
    match state.store.list_route_rules(query).await {
        Ok(page) => success_list(ctx, page.items, page.total_items, page_params),
        Err(error) => {
            backend_payment_error_response(ctx, "payment route rule list is unavailable", error)
        }
    }
}
async fn create_route_rule(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<UpsertRouteRuleBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    upsert_route_rule_inner(state, runtime_context, ctx, headers, None, body).await
}
async fn update_route_rule(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Path(route_rule_id): Path<String>,
    Json(body): Json<UpsertRouteRuleBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    upsert_route_rule_inner(
        state,
        runtime_context,
        ctx,
        headers,
        Some(route_rule_id),
        body,
    )
    .await
}
async fn upsert_route_rule_inner(
    state: BackendPaymentAdminState,
    runtime_context: Option<Extension<IamAppContext>>,
    ctx: Option<&WebRequestContext>,
    headers: HeaderMap,
    route_rule_id: Option<String>,
    body: UpsertRouteRuleBody,
) -> Response {
    let is_create = route_rule_id.is_none();
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let _write_headers = match validate_backend_write_payload(
        ctx,
        &headers,
        "payment-route-rule-upsert",
        &body,
        "payment-route-rule",
    ) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let rule_no = match body.rule_no {
        Some(value) => match require_trimmed_string(ctx, Some(value), "ruleNo") {
            Ok(value) => value,
            Err(response) => return response,
        },
        None => match route_rule_id.clone() {
            Some(value) => value,
            None => return validation(ctx, "ruleNo is required"),
        },
    };
    let channel_id = match require_trimmed_string(ctx, body.channel_id, "channelId") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let payload = BackendRouteRulePayload {
        id: route_rule_id,
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        rule_no,
        priority: body.priority.unwrap_or(0),
        purchase_type: body.purchase_type,
        country_code: body.country_code,
        currency_code: body.currency_code,
        client_platform: body.client_platform,
        amount_min: body.amount_min,
        amount_max: body.amount_max,
        user_segment: body.user_segment,
        risk_level: body.risk_level,
        channel_id,
        status: body.status.unwrap_or_else(|| "active".to_owned()),
        starts_at: body.starts_at,
        ends_at: body.ends_at,
    };
    match state.store.upsert_route_rule(payload).await {
        Ok(item) if is_create => success_created_item(ctx, item),
        Ok(item) => success_item(ctx, item),
        Err(error) => {
            backend_payment_error_response(ctx, "payment route rule upsert failed", error)
        }
    }
}
async fn delete_route_rule(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(route_rule_id): Path<String>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let scope = BackendTenantScope {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
    };
    match state.store.delete_route_rule(scope, route_rule_id).await {
        Ok(()) => success_no_content(ctx),
        Err(error) => {
            backend_payment_error_response(ctx, "payment route rule delete failed", error)
        }
    }
}
async fn list_attempts(
    State(state): State<BackendPaymentAdminState>,
    Query(params): Query<BackendListQueryParams>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let query = BackendTenantListQuery {
        scope: BackendTenantScope {
            tenant_id: subject.tenant_id,
            organization_id: subject.organization_id,
        },
        offset: page_params.offset,
        limit: page_params.page_size,
    };
    match state.store.list_attempts(query).await {
        Ok(page) => success_list(ctx, page.items, page.total_items, page_params),
        Err(error) => {
            backend_payment_error_response(ctx, "payment attempt list is unavailable", error)
        }
    }
}
async fn list_webhook_events(
    State(state): State<BackendPaymentAdminState>,
    Query(params): Query<BackendListQueryParams>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let query = BackendTenantListQuery {
        scope: BackendTenantScope {
            tenant_id: subject.tenant_id,
            organization_id: subject.organization_id,
        },
        offset: page_params.offset,
        limit: page_params.page_size,
    };
    match state.store.list_webhook_events(query).await {
        Ok(page) => success_list(ctx, page.items, page.total_items, page_params),
        Err(error) => {
            backend_payment_error_response(ctx, "payment webhook event list is unavailable", error)
        }
    }
}
async fn replay_webhook_event(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let replay_body = serde_json::json!({ "eventId": event_id });
    let _write_headers = match validate_backend_write_payload(
        ctx,
        &headers,
        "payment-webhook-replay",
        &replay_body,
        "wh-replay",
    ) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let scope = BackendTenantScope {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
    };
    match state.store.replay_webhook_event(scope, event_id.clone()).await {
        Ok(WebhookReplayResult::Queued(_item)) => success_command_accepted(ctx, Some(event_id)),
        Ok(WebhookReplayResult::NotFound) => {
            not_found(ctx, "payment webhook event was not found")
        }
        Ok(WebhookReplayResult::LimitExceeded { current_retries }) => conflict(
            ctx,
            format!(
                "webhook event has reached the replay limit ({WEBHOOK_STORED_REPLAY_MAX_RETRIES}); current retries = {current_retries}"
            ),
        ),
        Err(error) => backend_payment_error_response(ctx, "payment webhook replay failed", error),
    }
}
async fn list_reconciliation_runs(
    State(state): State<BackendPaymentAdminState>,
    Query(params): Query<BackendListQueryParams>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let query = BackendTenantListQuery {
        scope: BackendTenantScope {
            tenant_id: subject.tenant_id,
            organization_id: subject.organization_id,
        },
        offset: page_params.offset,
        limit: page_params.page_size,
    };
    match state.store.list_reconciliation_runs(query).await {
        Ok(page) => success_list(ctx, page.items, page.total_items, page_params),
        Err(error) => backend_payment_error_response(
            ctx,
            "payment reconciliation run list is unavailable",
            error,
        ),
    }
}
async fn create_reconciliation_run(
    State(state): State<BackendPaymentAdminState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<CreateReconciliationRunBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(ctx, message),
    };
    let write_headers = match validate_backend_write_payload(
        ctx,
        &headers,
        "payment-reconciliation-run-create",
        &body,
        "recon",
    ) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let provider_code = match require_trimmed_string(ctx, body.provider_code, "providerCode") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let provider_account_id = match body
        .provider_account_id
        .or(body.account_id)
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            }
        }) {
        Some(value) => value,
        None => return validation(ctx, "providerAccountId is required"),
    };
    let reconciliation_type =
        match require_trimmed_string(ctx, body.reconciliation_type, "reconciliationType") {
            Ok(value) => value,
            Err(response) => return response,
        };
    let period_start = match body.period_start.or(body.statement_date).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    }) {
        Some(value) => value,
        None => return validation(ctx, "periodStart is required"),
    };
    let period_end = match body.period_end {
        Some(value) => match require_trimmed_string(ctx, Some(value), "periodEnd") {
            Ok(value) => value,
            Err(response) => return response,
        },
        None => period_start.clone(),
    };
    let currency_code = match require_trimmed_string(ctx, body.currency_code, "currencyCode") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let payload = BackendReconciliationRunPayload {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        provider_code,
        provider_account_id,
        reconciliation_type,
        period_start,
        period_end,
        currency_code,
        request_no: write_headers.request_no,
        idempotency_key: write_headers.idempotency_key,
    };
    match state.store.create_reconciliation_run(payload).await {
        Ok(item) => success_created_item(ctx, item),
        Err(error) => {
            backend_payment_error_response(ctx, "payment reconciliation run create failed", error)
        }
    }
}
fn map_method(value: BackendPaymentMethodView) -> BackendPaymentMethodResponse {
    BackendPaymentMethodResponse {
        id: value.id,
        method_key: value.method_key,
        display_name: value.display_name,
        provider_code: value.provider_code,
        status: value.status,
        sort_order: value.sort_order,
    }
}
fn backend_payment_error_response(
    ctx: Option<&WebRequestContext>,
    _context: &str,
    error: CommerceServiceError,
) -> Response {
    map_service_error(ctx, error)
}
fn unauthorized_response(ctx: Option<&WebRequestContext>, message: impl Into<String>) -> Response {
    unauthorized(ctx, message)
}
fn map_method_row_pg(row: &PgRow) -> BackendPaymentMethodView {
    BackendPaymentMethodView {
        id: pg_string(row, "id"),
        tenant_id: pg_string(row, "tenant_id"),
        organization_id: pg_optional_string(row, "organization_id"),
        method_key: pg_string(row, "method_key"),
        display_name: pg_string(row, "display_name"),
        provider_code: pg_string(row, "provider_code"),
        status: pg_string(row, "status"),
        sort_order: row.try_get::<i32, _>("sort_order").map(i64::from).unwrap_or(0),
        created_at: pg_string(row, "created_at"),
        updated_at: pg_string(row, "updated_at"),
    }
}
fn map_route_rule_pg(row: &PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": pg_string(row, "id"),
        "ruleNo": pg_string(row, "rule_no"),
        "priority": row.try_get::<i32,_>("priority").map(i64::from).unwrap_or(0),
        "purchaseType": pg_optional_string(row, "purchase_type"),
        "countryCode": pg_optional_string(row, "country_code"),
        "currencyCode": pg_optional_string(row, "currency_code"),
        "clientPlatform": pg_optional_string(row, "client_platform"),
        "amountMin": pg_optional_string(row, "amount_min"),
        "amountMax": pg_optional_string(row, "amount_max"),
        "userSegment": pg_optional_string(row, "user_segment"),
        "riskLevel": pg_optional_string(row, "risk_level"),
        "channelId": pg_string(row, "channel_id"),
        "status": pg_string(row, "status"),
        "startsAt": pg_optional_string(row, "starts_at"),
        "endsAt": pg_optional_string(row, "ends_at"),
    })
}
fn map_attempt_pg(row: &PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": pg_string(row, "id"),
        "paymentIntentId": pg_string(row, "payment_intent_id"),
        "attemptNo": pg_string(row, "attempt_no"),
        "providerCode": pg_string(row, "provider_code"),
        "channelId": pg_string(row, "channel_id"),
        "amount": pg_string(row, "amount"),
        "currencyCode": pg_string(row, "currency_code"),
        "status": pg_string(row, "status"),
        "providerTransactionId": pg_optional_string(row, "provider_transaction_id"),
        "createdAt": pg_string(row, "created_at"),
    })
}
fn map_webhook_event_pg(row: &PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": pg_string(row, "id"),
        "eventId": pg_string(row, "event_id"),
        "providerCode": pg_string(row, "provider_code"),
        "eventType": pg_string(row, "event_type"),
        "status": pg_string(row, "status"),
        "receivedAt": pg_string(row, "received_at"),
        "processedAt": pg_optional_string(row, "processed_at"),
        "retries": row.try_get::<i64,_>("retries").unwrap_or(0),
    })
}
fn map_reconciliation_run_pg(row: &PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": pg_string(row, "id"),
        "runNo": pg_string(row, "run_no"),
        "providerCode": pg_string(row, "provider_code"),
        "providerAccountId": pg_string(row, "provider_account_id"),
        "reconciliationType": pg_string(row, "reconciliation_type"),
        "periodStart": pg_string(row, "period_start"),
        "periodEnd": pg_string(row, "period_end"),
        "status": pg_string(row, "status"),
        "matchedCount": row.try_get::<i32,_>("matched_count").map(i64::from).unwrap_or(0),
        "mismatchedCount": row.try_get::<i32,_>("mismatched_count").map(i64::from).unwrap_or(0),
        "currencyCode": pg_string(row, "currency_code"),
        "createdAt": pg_string(row, "created_at"),
    })
}
fn pg_total_count(rows: &[PgRow]) -> i64 {
    rows.first()
        .and_then(|row| row.try_get::<i64, _>("total_count").ok())
        .unwrap_or(0)
}
#[allow(clippy::result_large_err)]
fn require_trimmed_string(
    ctx: Option<&WebRequestContext>,
    value: Option<String>,
    field: &str,
) -> Result<String, Response> {
    match value {
        Some(value) if !value.trim().is_empty() => Ok(value.trim().to_owned()),
        _ => Err(validation(ctx, format!("{field} is required"))),
    }
}
fn credential_storage(secret_ref: &str) -> &'static str {
    if secret_ref.starts_with("database:") {
        "database_encrypted"
    } else if secret_ref.trim().is_empty() {
        "none"
    } else {
        "legacy_reference"
    }
}
fn pg_optional_string(row: &PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>, _>(
                column,
            )
            .ok()
            .flatten()
            .map(|value| value.to_rfc3339())
        })
}
fn pg_string(row: &PgRow, column: &str) -> String {
    pg_optional_string(row, column).unwrap_or_default()
}
fn pg_bool(row: &PgRow, column: &str) -> bool {
    row.try_get::<bool, _>(column).unwrap_or(false)
}
fn pg_json(row: &PgRow, column: &str) -> Value {
    row.try_get::<Value, _>(column)
        .unwrap_or_else(|_| serde_json::json!({}))
}
fn pg_provider_account_value(row: &PgRow) -> Value {
    serde_json::json!({
        "id": pg_string(row, "id"),
        "accountNo": pg_string(row, "account_no"),
        "providerCode": pg_string(row, "provider_code"),
        "merchantId": pg_string(row, "merchant_id"),
        "accountName": pg_optional_string(row, "account_name"),
        "accountNameI18n": pg_json(row, "account_name_i18n"),
        "accountMode": pg_string(row, "account_mode"),
        "partnerProviderAccountId": pg_optional_string(row, "partner_provider_account_id"),
        "environment": pg_string(row, "environment"),
        "countryCode": pg_string(row, "country_code"),
        "settlementCurrency": pg_string(row, "settlement_currency"),
        "hasPrimarySecret": pg_bool(row, "has_primary_secret") || pg_string(row, "secret_ref").starts_with("database:"),
        "hasWebhookSecret": pg_bool(row, "has_webhook_secret") || pg_optional_string(row, "webhook_secret_ref").is_some_and(|value| value.starts_with("database:")),
        "hasCertificate": pg_bool(row, "has_certificate") || pg_optional_string(row, "certificate_ref").is_some_and(|value| value.starts_with("database:")),
        "credentialStorage": credential_storage(&pg_string(row, "secret_ref")),
        "capabilities": pg_json(row, "capabilities"),
        "metadata": pg_json(row, "metadata"),
        "status": pg_string(row, "status"),
        "createdAt": pg_string(row, "created_at"),
        "updatedAt": pg_string(row, "updated_at"),
    })
}
#[allow(clippy::result_large_err)]
fn validate_backend_write_payload(
    ctx: Option<&WebRequestContext>,
    headers: &HeaderMap,
    scope: &str,
    body: &impl Serialize,
    request_no_prefix: &str,
) -> Result<AppWriteCommandHeaders, Response> {
    validate_write_payload(headers, scope, body, |idempotency_key| {
        format!("{request_no_prefix}-{idempotency_key}")
    })
    .map_err(|error| backend_write_header_error(ctx, error))
}
fn backend_write_header_error(
    ctx: Option<&WebRequestContext>,
    error: WriteCommandHeaderError,
) -> Response {
    let message = match error {
        WriteCommandHeaderError::InvalidHeader(message) => message.to_owned(),
    };
    validation(ctx, message)
}
fn current_timestamp_string() -> String {
    sqlx::types::chrono::Utc::now().to_rfc3339()
}
fn stable_storage_id(parts: &[&str]) -> String {
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