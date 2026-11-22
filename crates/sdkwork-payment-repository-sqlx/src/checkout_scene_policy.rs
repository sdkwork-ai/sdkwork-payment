//! Checkout scene resolution: which provider product serves a buyer's context.
//!
//! Whether an iOS app paying with WeChat must use `wechat_app` or falls back to
//! `wechat_h5`, and which product a Pad or a mini-program gets, is deployment and
//! tenant configuration — not a compile-time table. This module resolves the
//! product from `commerce_payment_checkout_scene_policy` with a documented
//! specificity order, so an operator can add a platform without a release.

use sdkwork_contract_service::CommerceServiceError;
use sqlx::PgPool;

/// A configured answer to "which product serves this buyer context".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutSceneProduct {
    pub product_code: String,
    /// How specific the winning rule was: `os` > `architecture` > `scene`.
    pub matched_scope: &'static str,
}

/// Resolves the product for one buyer context.
///
/// Most specific match wins, then the highest `priority`:
///
/// 1. scene + architecture + os + method
/// 2. scene + architecture + method (os column NULL or `any`)
/// 3. scene + method
///
/// An architecture-specific rule never matches a different architecture, so
/// `ios` can be pinned to `wechat_app` while Android keeps the H5 fallback.
pub async fn resolve_checkout_scene_product_postgres(
    pool: &PgPool,
    tenant_id: &str,
    organization_id: Option<&str>,
    scene: &str,
    app_architecture: &str,
    os_family: &str,
    payment_method: &str,
) -> Result<Option<CheckoutSceneProduct>, CommerceServiceError> {
    // Platform rows persist the sentinel organization scope so personal sessions
    // match the platform defaults (DATABASE_SPEC DB090).
    let organization_id = organization_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0");
    let scene = scene.trim().to_ascii_lowercase();
    let app_architecture = app_architecture.trim().to_ascii_lowercase();
    let os_family = os_family.trim().to_ascii_lowercase();
    let payment_method = payment_method.trim().to_ascii_lowercase();
    if scene.is_empty() || app_architecture.is_empty() || payment_method.is_empty() {
        return Err(CommerceServiceError::validation(
            "scene, appArchitecture and paymentMethod are required to resolve a checkout product",
        ));
    }
    let row = sqlx::query(
        "SELECT product_code, \
                CASE \
                    WHEN os_family IS NOT NULL AND os_family <> '' AND os_family <> 'any' THEN 'os' \
                    WHEN app_architecture IS NOT NULL AND app_architecture <> '' AND app_architecture <> 'any' THEN 'architecture' \
                    ELSE 'scene' \
                END AS matched_scope, \
                priority \
         FROM commerce_payment_checkout_scene_policy \
         WHERE tenant_id = CAST($1 AS TEXT) \
           AND organization_id = CAST($2 AS TEXT) \
           AND deleted_at IS NULL \
           AND status = 'active' \
           AND LOWER(scene) = $3 \
           AND LOWER(payment_method) = $4 \
           AND ( \
                (LOWER(app_architecture) = $5 AND LOWER(COALESCE(NULLIF(os_family, ''), 'any')) = $6) \
                OR (LOWER(app_architecture) = $5 AND LOWER(COALESCE(NULLIF(os_family, ''), 'any')) = 'any') \
                OR (LOWER(COALESCE(NULLIF(app_architecture, ''), 'any')) = 'any' \
                    AND LOWER(COALESCE(NULLIF(os_family, ''), 'any')) = 'any') \
           ) \
         ORDER BY \
           CASE \
               WHEN os_family IS NOT NULL AND os_family <> '' AND os_family <> 'any' THEN 0 \
               WHEN app_architecture IS NOT NULL AND app_architecture <> '' AND app_architecture <> 'any' THEN 1 \
               ELSE 2 \
           END ASC, \
           priority ASC, \
           created_at ASC \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(&scene)
    .bind(&payment_method)
    .bind(&app_architecture)
    .bind(&os_family)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        CommerceServiceError::storage(format!("failed to resolve checkout scene product: {error}"))
    })?;
    let Some(row) = row else {
        return Ok(None);
    };
    use sqlx::Row;
    let matched_scope: String = row.try_get("matched_scope").unwrap_or_else(|_| "scene".to_owned());
    Ok(Some(CheckoutSceneProduct {
        product_code: row.try_get("product_code").unwrap_or_default(),
        matched_scope: match matched_scope.as_str() {
            "os" => "os",
            "architecture" => "architecture",
            _ => "scene",
        },
    }))
}

/// One configured checkout scene rule as an operator sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutScenePolicyRow {
    pub id: String,
    pub scene: String,
    pub app_architecture: String,
    pub os_family: String,
    pub payment_method: String,
    pub product_code: String,
    pub status: String,
    pub priority: i64,
}

/// Insert or update one scene rule.
#[derive(Debug, Clone)]
pub struct UpsertCheckoutScenePolicyCommand<'a> {
    pub id: &'a str,
    pub tenant_id: &'a str,
    pub organization_id: Option<&'a str>,
    pub scene: &'a str,
    pub app_architecture: &'a str,
    pub os_family: &'a str,
    pub payment_method: &'a str,
    pub product_code: &'a str,
    pub status: &'a str,
    pub priority: i64,
}

/// Lists the tenant's active and inactive scene rules, newest first.
pub async fn list_checkout_scene_policies_postgres(
    pool: &PgPool,
    tenant_id: &str,
    organization_id: Option<&str>,
    limit: i64,
) -> Result<Vec<CheckoutScenePolicyRow>, CommerceServiceError> {
    let organization_id = normalize_scope(organization_id);
    let rows = sqlx::query(
        "SELECT id, scene, app_architecture, os_family, payment_method, product_code, \
                status, priority \
         FROM commerce_payment_checkout_scene_policy \
         WHERE tenant_id = CAST($1 AS TEXT) \
           AND organization_id = CAST($2 AS TEXT) \
           AND deleted_at IS NULL \
         ORDER BY priority ASC, scene ASC, app_architecture ASC, os_family ASC, payment_method ASC \
         LIMIT $3",
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(limit.clamp(1, 500))
    .fetch_all(pool)
    .await
    .map_err(|error| {
        CommerceServiceError::storage(format!("failed to list checkout scene policies: {error}"))
    })?;
    use sqlx::Row;
    Ok(rows
        .into_iter()
        .map(|row| CheckoutScenePolicyRow {
            id: row.try_get("id").unwrap_or_default(),
            scene: row.try_get("scene").unwrap_or_default(),
            app_architecture: row.try_get("app_architecture").unwrap_or_else(|_| "any".to_owned()),
            os_family: row.try_get("os_family").unwrap_or_else(|_| "any".to_owned()),
            payment_method: row.try_get("payment_method").unwrap_or_default(),
            product_code: row.try_get("product_code").unwrap_or_default(),
            status: row.try_get("status").unwrap_or_else(|_| "active".to_owned()),
            priority: row.try_get("priority").unwrap_or(100),
        })
        .collect())
}

/// Inserts or replaces one scene rule.
///
/// The unique index is case-insensitive, so codes are normalized here as well:
/// `WeChat`/`wechat` must not become two rules that both claim the same context.
/// Inserts or replaces one scene rule inside a caller-owned transaction, so the
/// rule and its audit record commit together.
pub async fn upsert_checkout_scene_policy_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    command: &UpsertCheckoutScenePolicyCommand<'_>,
) -> Result<CheckoutScenePolicyRow, CommerceServiceError> {
    let organization_id = normalize_scope(command.organization_id);
    let scene = command.scene.trim().to_ascii_lowercase();
    let app_architecture = normalize_code(command.app_architecture, "any");
    let os_family = normalize_code(command.os_family, "any");
    let payment_method = command.payment_method.trim().to_ascii_lowercase();
    let product_code = command.product_code.trim().to_ascii_lowercase();
    if scene.is_empty() || payment_method.is_empty() || product_code.is_empty() {
        return Err(CommerceServiceError::validation(
            "scene, paymentMethod and productCode are required",
        ));
    }
    if !matches!(command.status.trim(), "active" | "inactive") {
        return Err(CommerceServiceError::validation(
            "status must be one of active, inactive",
        ));
    }
    let row = sqlx::query(
        "INSERT INTO commerce_payment_checkout_scene_policy \
             (id, tenant_id, organization_id, scene, app_architecture, os_family, \
              payment_method, product_code, status, priority) \
         VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), $4, $5, $6, $7, $8, $9, $10) \
         ON CONFLICT (tenant_id, organization_id, LOWER(scene), LOWER(app_architecture), LOWER(os_family), LOWER(payment_method)) \
         WHERE deleted_at IS NULL \
         DO UPDATE SET product_code = EXCLUDED.product_code, \
                       status = EXCLUDED.status, \
                       priority = EXCLUDED.priority, \
                       updated_at = NOW() \
         RETURNING id, scene, app_architecture, os_family, payment_method, product_code, status, priority",
    )
    .bind(command.id)
    .bind(command.tenant_id)
    .bind(organization_id)
    .bind(&scene)
    .bind(&app_architecture)
    .bind(&os_family)
    .bind(&payment_method)
    .bind(&product_code)
    .bind(command.status.trim())
    .bind(command.priority)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| {
        CommerceServiceError::storage(format!("failed to upsert checkout scene policy: {error}"))
    })?;
    use sqlx::Row;
    Ok(CheckoutScenePolicyRow {
        id: row.try_get("id").unwrap_or_default(),
        scene: row.try_get("scene").unwrap_or_default(),
        app_architecture: row.try_get("app_architecture").unwrap_or_else(|_| "any".to_owned()),
        os_family: row.try_get("os_family").unwrap_or_else(|_| "any".to_owned()),
        payment_method: row.try_get("payment_method").unwrap_or_default(),
        product_code: row.try_get("product_code").unwrap_or_default(),
        status: row.try_get("status").unwrap_or_default(),
        priority: row.try_get("priority").unwrap_or(100),
    })
}

pub async fn upsert_checkout_scene_policy_postgres(
    pool: &PgPool,
    command: &UpsertCheckoutScenePolicyCommand<'_>,
) -> Result<CheckoutScenePolicyRow, CommerceServiceError> {
    let organization_id = normalize_scope(command.organization_id);
    let scene = command.scene.trim().to_ascii_lowercase();
    let app_architecture = normalize_code(command.app_architecture, "any");
    let os_family = normalize_code(command.os_family, "any");
    let payment_method = command.payment_method.trim().to_ascii_lowercase();
    let product_code = command.product_code.trim().to_ascii_lowercase();
    if scene.is_empty() || payment_method.is_empty() || product_code.is_empty() {
        return Err(CommerceServiceError::validation(
            "scene, paymentMethod and productCode are required",
        ));
    }
    if !matches!(command.status.trim(), "active" | "inactive") {
        return Err(CommerceServiceError::validation(
            "status must be one of active, inactive",
        ));
    }
    let row = sqlx::query(
        "INSERT INTO commerce_payment_checkout_scene_policy \
             (id, tenant_id, organization_id, scene, app_architecture, os_family, \
              payment_method, product_code, status, priority) \
         VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), $4, $5, $6, $7, $8, $9, $10) \
         ON CONFLICT (tenant_id, organization_id, LOWER(scene), LOWER(app_architecture), LOWER(os_family), LOWER(payment_method)) \
         WHERE deleted_at IS NULL \
         DO UPDATE SET product_code = EXCLUDED.product_code, \
                       status = EXCLUDED.status, \
                       priority = EXCLUDED.priority, \
                       updated_at = NOW() \
         RETURNING id, scene, app_architecture, os_family, payment_method, product_code, status, priority",
    )
    .bind(command.id)
    .bind(command.tenant_id)
    .bind(organization_id)
    .bind(&scene)
    .bind(&app_architecture)
    .bind(&os_family)
    .bind(&payment_method)
    .bind(&product_code)
    .bind(command.status.trim())
    .bind(command.priority)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        CommerceServiceError::storage(format!("failed to upsert checkout scene policy: {error}"))
    })?;
    use sqlx::Row;
    Ok(CheckoutScenePolicyRow {
        id: row.try_get("id").unwrap_or_default(),
        scene: row.try_get("scene").unwrap_or_default(),
        app_architecture: row.try_get("app_architecture").unwrap_or_else(|_| "any".to_owned()),
        os_family: row.try_get("os_family").unwrap_or_else(|_| "any".to_owned()),
        payment_method: row.try_get("payment_method").unwrap_or_default(),
        product_code: row.try_get("product_code").unwrap_or_default(),
        status: row.try_get("status").unwrap_or_default(),
        priority: row.try_get("priority").unwrap_or(100),
    })
}

fn normalize_scope(organization_id: Option<&str>) -> &str {
    organization_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0")
}

fn normalize_code(value: &str, fallback: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value
    }
}