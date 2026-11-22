//! Checkout scene resolution must pick the most specific configured rule.
//!
//! The mapping from (scene, app architecture, os, payment method) to a provider
//! product is deployment configuration. These tests execute the shipped baseline
//! DDL and prove the documented specificity order, so a new platform is a data
//! change rather than a code change.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; skips cleanly when unset.

use sdkwork_payment_repository_sqlx::resolve_checkout_scene_product_postgres;
use sqlx::postgres::{PgPool, PgPoolOptions};

fn test_database_url() -> Option<String> {
    std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL").ok()
}

async fn scene_pool(label: &str) -> Option<PgPool> {
    let base = test_database_url()?;
    let schema = format!("payment_checkout_scene_{}_{}", std::process::id(), label);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .after_connect({
            let schema = schema.clone();
            move |connection, _| {
                let schema = schema.clone();
                Box::pin(async move {
                    sqlx::query(sqlx::AssertSqlSafe(format!(
                        "SET search_path TO {schema}, public"
                    )))
                    .execute(connection)
                    .await?;
                    Ok(())
                })
            }
        })
        .connect(&base)
        .await
        .expect("connect test postgres");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(&pool)
    .await
    .expect("drop stale schema");
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&pool)
        .await
        .expect("create schema");
    sqlx::raw_sql(sqlx::AssertSqlSafe(include_str!(
        "../../../database/ddl/baseline/postgres/0001_payment_baseline.sql"
    )))
    .execute(&pool)
    .await
    .expect("apply payment baseline");
    Some(pool)
}

async fn seed(pool: &PgPool, id: &str, arch: &str, os: &str, product: &str, priority: i64) {
    sqlx::query(sqlx::AssertSqlSafe(
        "INSERT INTO commerce_payment_checkout_scene_policy \
         (id, tenant_id, organization_id, scene, app_architecture, os_family, payment_method, product_code, priority) \
         VALUES ($1, '100001', '0', 'checkout', $2, $3, 'wechat', $4, $5)"
            .to_owned(),
    ))
    .bind(id)
    .bind(arch)
    .bind(os)
    .bind(product)
    .bind(priority)
    .execute(pool)
    .await
    .expect("seed scene policy");
}

#[tokio::test]
async fn the_most_specific_rule_wins_and_platforms_do_not_bleed() {
    let Some(pool) = scene_pool("specificity").await else {
        return;
    };

    // Baseline rules every tenant starts from.
    seed(&pool, "p-scene", "any", "any", "wechat_h5", 100).await;
    // An iOS app must invoke the native WeChat flow instead of the H5 page.
    seed(&pool, "p-ios", "app", "ios", "wechat_app", 10).await;
    // Android keeps the H5 fallback until its own rule is added.
    seed(&pool, "p-android", "app", "any", "wechat_h5", 50).await;

    let ios = resolve_checkout_scene_product_postgres(
        &pool, "100001", None, "checkout", "app", "ios", "wechat",
    )
    .await
    .expect("resolve ios")
    .expect("ios product");
    assert_eq!("wechat_app", ios.product_code);
    assert_eq!("os", ios.matched_scope, "the os-specific rule must win");

    let android = resolve_checkout_scene_product_postgres(
        &pool, "100001", None, "checkout", "app", "android", "wechat",
    )
    .await
    .expect("resolve android")
    .expect("android product");
    assert_eq!(
        "wechat_h5", android.product_code,
        "an ios rule must never serve an android buyer"
    );
    assert_eq!("architecture", android.matched_scope);

    // A context with no architecture-specific rule falls back to the scene rule.
    let web = resolve_checkout_scene_product_postgres(
        &pool, "100001", None, "checkout", "web", "any", "wechat",
    )
    .await
    .expect("resolve web")
    .expect("web product");
    assert_eq!("wechat_h5", web.product_code);
    assert_eq!("scene", web.matched_scope);

    // An unknown method resolves to nothing rather than guessing a product.
    let unknown = resolve_checkout_scene_product_postgres(
        &pool, "100001", None, "checkout", "app", "ios", "alipay",
    )
    .await
    .expect("resolve unknown");
    assert!(unknown.is_none(), "an unconfigured method must not resolve");

    // Deactivated rules stop serving immediately.
    sqlx::query(sqlx::AssertSqlSafe(
        "UPDATE commerce_payment_checkout_scene_policy SET status = 'inactive' WHERE id = 'p-ios'"
            .to_owned(),
    ))
    .execute(&pool)
    .await
    .expect("deactivate ios rule");
    let after = resolve_checkout_scene_product_postgres(
        &pool, "100001", None, "checkout", "app", "ios", "wechat",
    )
    .await
    .expect("resolve ios after deactivation")
    .expect("fallback product");
    assert_eq!(
        "wechat_h5", after.product_code,
        "a deactivated rule must fall through to the architecture rule"
    );
}

#[tokio::test]
async fn the_shipped_defaults_cover_every_supported_platform() {
    let Some(pool) = scene_pool("defaults").await else {
        return;
    };
    sqlx::raw_sql(sqlx::AssertSqlSafe(include_str!(
        "../../../database/seeds/common/010_checkout_scene_policy.sql"
    )))
    .execute(&pool)
    .await
    .expect("apply shipped checkout scene defaults");

    // The documented architecture matrix must resolve out of the box; a missing
    // default means every fresh deployment starts by failing the first payment on
    // that platform.
    for (arch, os, method, expected) in [
        ("web", "any", "wechat", "wechat_h5"),
        ("mini_program", "any", "wechat", "wechat_jsapi"),
        ("app", "ios", "wechat", "wechat_app"),
        ("app", "android", "wechat", "wechat_app"),
        ("web", "any", "alipay", "alipay_pc"),
        ("h5", "any", "alipay", "alipay_wap"),
        ("app", "ios", "alipay", "alipay_app"),
        ("app", "ios", "stripe", "stripe_apple_pay"),
        ("app", "android", "stripe", "stripe_google_pay"),
        ("web", "any", "stripe", "stripe_card"),
    ] {
        let resolved = resolve_checkout_scene_product_postgres(
            &pool, "100001", None, "checkout", arch, os, method,
        )
        .await
        .expect("resolve default")
        .unwrap_or_else(|| panic!("no default policy for {arch}/{os}/{method}"));
        assert_eq!(
            expected, resolved.product_code,
            "{arch}/{os}/{method} resolved to the wrong product"
        );
    }

    // The purchase scenes the order service sends must be covered too: a
    // membership or recharge checkout without a rule has no product at all.
    for (scene, arch, os, method, expected) in [
        ("membership", "app", "ios", "wechat", "wechat_app"),
        ("membership", "web", "any", "stripe", "stripe_card"),
        ("points_recharge", "mini_program", "any", "wechat", "wechat_jsapi"),
        ("points_recharge", "app", "ios", "alipay", "alipay_app"),
    ] {
        let resolved = resolve_checkout_scene_product_postgres(
            &pool, "100001", None, scene, arch, os, method,
        )
        .await
        .expect("resolve purchase scene")
        .unwrap_or_else(|| panic!("no default policy for {scene}/{arch}/{os}/{method}"));
        assert_eq!(expected, resolved.product_code, "{scene} resolved wrongly");
    }
    // A platform without a default must report "unconfigured" instead of silently
    // reusing another platform's product.
    let pad = resolve_checkout_scene_product_postgres(
        &pool, "100001", None, "checkout", "pad", "ipados", "wechat",
    )
    .await
    .expect("resolve pad");
    assert!(
        pad.is_none(),
        "an unconfigured architecture must not inherit a rule by accident"
    );
}