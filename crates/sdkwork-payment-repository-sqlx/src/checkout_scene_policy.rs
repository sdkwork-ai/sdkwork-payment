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
