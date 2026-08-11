//! C17 淇锛歱ayment backend-api 鐨?HTTP route manifest銆?
//!
//! 閬靛惊 `API_SPEC.md` 搂4.2.1 涓?`WEB_BACKEND_SPEC.md` 搂4.2/搂4.3 鐨勮姹傦紝backend-api
//! route crate `MUST` 瀵煎嚭 `backend_route_manifest` 涓?`gateway_route_manifest`锛?
//! 骞堕€氳繃 framework contract 绫诲瀷 `HttpRoute` 澹版槑 manifest锛屼互渚匡細
//! 1. 妗嗘灦杩愯鏃惰В鏋?operationId / rate-limit tier / 鍏叡璺緞锛?
//! 2. 妗嗘灦鑷姩娲剧敓 `ContractFallbackConfig`锛屼负 manifest 鍐呮湭鎸傝浇 handler 鐨勮矾寰?
//!    杩斿洖 501 Problem+json銆佷负瀹屽叏鏈煡璺緞杩斿洖 404 Problem+json锛?
//! 3. OpenAPI 鐗╁寲鍣ㄧ敓鎴?owner-only authority 鏂囨。銆?
//!
//! 鎵€鏈夊彈淇濇姢璺敱缁熶竴浣跨敤 `RouteAuth::DualToken`锛坄API_SPEC.md` 搂4.2.1 瑙勫畾鍙椾繚鎶?
//! backend-api `MUST` 浣跨敤 `dual-token`锛宎gent 璺敱鍙敤 `agent-token`锛宲ayment
//! backend 鏆傛棤 agent 璺敱锛夈€傚啓鎿嶄綔锛圥OST/PATCH锛夋爣璁?`idempotent = true`锛岃〃绀?
//! 璇ヨ矾鐢辨帴鍙?`Idempotency-Key` / `Sdkwork-Request-Hash` 鍛戒护澶村苟鍙備笌骞傜瓑浠撳偍灞?
//! 鍘婚噸銆侱ELETE 涓?replay action 涓嶆爣璁?idempotent锛圚TTP DELETE 鏈韩骞傜瓑锛?
//! replay 鏄€掑 retries 鐨勫姩浣滐紝闈炲箓绛夛級銆?

use sdkwork_web_core::{HttpMethod, HttpRoute, HttpRouteManifest};

/// payment backend-api 璺敱鍓嶇紑锛坄API_SPEC.md` 搂4.2.1 瑙勫畾 backend-api `MUST`
/// 浣跨敤 `/backend/v3/api`锛夈€?
pub const BACKEND_API_PREFIX: &str = "/backend/v3/api";

/// payment backend-api 鍏叡璺緞鍓嶇紑銆備粎鍖呭惈 infra 鍋ュ悍妫€鏌ヨ矾寰勶紝涓氬姟璺緞鍏ㄩ儴鍙?
/// dual-token 淇濇姢銆?
pub fn payment_backend_api_public_path_prefixes() -> Vec<String> {
    sdkwork_web_bootstrap::infra_public_path_prefixes()
}

const HTTP_ROUTES: &[HttpRoute] = &[
    // === Payment Intent锛堟煡璇級 ===
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/intents",
        "payments",
        "intents.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/intents/{paymentIntentId}",
        "payments",
        "intents.retrieve",
    ),
    // === Refund operations ===
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/refunds",
        "payments",
        "refunds.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/refunds",
        "payments",
        "refunds.create",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/refunds/{refundId}",
        "payments",
        "refunds.retrieve",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/refunds/{refundId}/retry",
        "payments",
        "refunds.retry",
    )
    .with_idempotent(true),
    // === Payment Method ===
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/methods",
        "payments",
        "methods.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/methods",
        "payments",
        "methods.create",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Patch,
        "/backend/v3/api/payments/methods/{methodKey}",
        "payments",
        "methods.update",
    )
    .with_idempotent(true),
    // === Provider Account ===
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/provider_accounts",
        "payments",
        "providerAccounts.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/provider_accounts",
        "payments",
        "providerAccounts.create",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Patch,
        "/backend/v3/api/payments/provider_accounts/{providerAccountId}",
        "payments",
        "providerAccounts.update",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Delete,
        "/backend/v3/api/payments/provider_accounts/{providerAccountId}",
        "payments",
        "providerAccounts.delete",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/provider_accounts/{providerAccountId}/test",
        "payments",
        "providerAccounts.test",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/provider_accounts/{providerAccountId}/credentials/rotate",
        "payments",
        "providerAccounts.credentials.rotate",
    )
    .with_idempotent(true),
    // === Channel ===
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/channels",
        "payments",
        "channels.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/channels",
        "payments",
        "channels.create",
    )
    .with_idempotent(true),
    // === Route Rule ===
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/route_rules",
        "payments",
        "routeRules.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/route_rules",
        "payments",
        "routeRules.create",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Patch,
        "/backend/v3/api/payments/route_rules/{routeRuleId}",
        "payments",
        "routeRules.update",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Delete,
        "/backend/v3/api/payments/route_rules/{routeRuleId}",
        "payments",
        "routeRules.delete",
    ),
    // === Partner sub-merchants ===
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/sub_merchants",
        "payments",
        "subMerchants.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/sub_merchants",
        "payments",
        "subMerchants.create",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/sub_merchants/{subMerchantId}",
        "payments",
        "subMerchants.retrieve",
    ),
    HttpRoute::dual_token(
        HttpMethod::Patch,
        "/backend/v3/api/payments/sub_merchants/{subMerchantId}",
        "payments",
        "subMerchants.update",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Delete,
        "/backend/v3/api/payments/sub_merchants/{subMerchantId}",
        "payments",
        "subMerchants.delete",
    ),
    // === Provider certificates ===
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/certificates",
        "payments",
        "certificates.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/certificates",
        "payments",
        "certificates.create",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/certificates/{certificateId}",
        "payments",
        "certificates.retrieve",
    ),
    HttpRoute::dual_token(
        HttpMethod::Delete,
        "/backend/v3/api/payments/certificates/{certificateId}",
        "payments",
        "certificates.delete",
    ),
    // === Attempt / Webhook / Reconciliation ===
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/attempts",
        "payments",
        "attempts.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/webhook_events",
        "payments",
        "webhookEvents.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/webhook_events/{eventId}/replay",
        "payments",
        "webhookEvents.replay",
    ),
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/payments/reconciliation_runs",
        "payments",
        "reconciliationRuns.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/reconciliation_runs",
        "payments",
        "reconciliationRuns.create",
    )
    .with_idempotent(true),
    // === Development diagnostics ===
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/dev/sandbox_trigger",
        "payments",
        "dev.sandboxTrigger",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/dev/test_payments",
        "payments",
        "dev.testPayments",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/dev/check_attempt_status",
        "payments",
        "dev.checkAttemptStatus",
    )
    .with_idempotent(true),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/payments/dev/webhook_signature_test",
        "payments",
        "dev.webhookSignatureTest",
    )
    .with_idempotent(true),
];

/// 鏋勯€?payment backend-api 鐨?route manifest銆?
pub fn backend_route_manifest() -> HttpRouteManifest {
    HttpRouteManifest::new(HTTP_ROUTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_web_core::RouteAuth;
    use std::collections::BTreeSet;

    #[test]
    fn manifest_declares_all_routes_with_dual_token_auth() {
        let manifest = backend_route_manifest();
        assert!(!manifest.routes().is_empty());
        for route in manifest.routes() {
            assert_eq!(
                route.auth,
                RouteAuth::DualToken,
                "route {:?} {} must be dual-token protected",
                route.method,
                route.path,
            );
        }
    }

    #[test]
    fn manifest_routes_use_backend_api_prefix() {
        let manifest = backend_route_manifest();
        for route in manifest.routes() {
            assert!(
                route.path.starts_with(BACKEND_API_PREFIX),
                "route {:?} {} must start with backend-api prefix {}",
                route.method,
                route.path,
                BACKEND_API_PREFIX,
            );
        }
    }

    #[test]
    fn manifest_has_no_duplicate_method_path_pairs() {
        let manifest = backend_route_manifest();
        let mut seen = std::collections::HashSet::new();
        for route in manifest.routes() {
            let key = (format!("{:?}", route.method), route.path);
            assert!(
                seen.insert(key),
                "duplicate (method, path) pair in backend-api manifest: {:?} {}",
                route.method,
                route.path,
            );
        }
    }

    #[test]
    fn write_routes_are_marked_idempotent() {
        let manifest = backend_route_manifest();
        let idempotent_write_routes: Vec<_> = manifest
            .routes()
            .iter()
            .filter(|route| route.method == HttpMethod::Post || route.method == HttpMethod::Patch)
            .filter(|route| route.idempotent)
            .map(|route| route.operation_id)
            .collect();
        // 鏍稿績鍐欐搷浣滃繀椤绘爣璁板箓绛?
        assert!(idempotent_write_routes.contains(&"methods.create"));
        assert!(idempotent_write_routes.contains(&"methods.update"));
        assert!(idempotent_write_routes.contains(&"providerAccounts.create"));
        assert!(idempotent_write_routes.contains(&"channels.create"));
        assert!(idempotent_write_routes.contains(&"routeRules.create"));
        assert!(idempotent_write_routes.contains(&"routeRules.update"));
        assert!(idempotent_write_routes.contains(&"reconciliationRuns.create"));
        assert!(idempotent_write_routes.contains(&"refunds.create"));
        assert!(idempotent_write_routes.contains(&"refunds.retry"));
    }

    #[test]
    fn delete_and_replay_are_not_idempotent() {
        let manifest = backend_route_manifest();
        let non_idempotent: Vec<_> = manifest
            .routes()
            .iter()
            .filter(|route| !route.idempotent)
            .map(|route| route.operation_id)
            .collect();
        // DELETE 鏈韩骞傜瓑锛宺eplay 鏄€掑 retries 鐨勫姩浣滐紝鍧囦笉浣跨敤骞傜瓑澶?
        assert!(non_idempotent.contains(&"routeRules.delete"));
        assert!(non_idempotent.contains(&"webhookEvents.replay"));
    }

    #[test]
    fn manifest_passes_framework_validations() {
        use sdkwork_web_core::WebRequestContextProfile;

        let manifest = backend_route_manifest();
        let profile = WebRequestContextProfile {
            public_path_prefixes: payment_backend_api_public_path_prefixes(),
            ..WebRequestContextProfile::default()
        };
        manifest
            .validate_public_path_prefixes(&profile.public_path_prefixes)
            .expect("public prefixes must not cover protected manifest routes");
        manifest
            .validate_route_auth_for_surfaces(&profile)
            .expect("all backend-api routes must declare dual-token auth");
        manifest
            .validate_no_ambient_context_path_markers(&profile)
            .expect("manifest must not embed ambient tenant/org scoping");
    }

    #[test]
    fn manifest_operations_match_backend_openapi_authority() {
        let authority = include_str!(
            "../../../apis/backend-api/payment/sdkwork-payment-backend-api.openapi.yaml"
        )
        .trim_start_matches('\u{feff}');
        let document: serde_json::Value = serde_json::from_str(authority)
            .expect("payment backend OpenAPI must be valid JSON-compatible YAML");
        let openapi_operations = openapi_operations(&document);
        let manifest_operations = backend_route_manifest()
            .routes()
            .iter()
            .map(|route| {
                (
                    method_label(route.method).to_owned(),
                    route.path.to_owned(),
                    route.operation_id.to_owned(),
                )
            })
            .collect::<BTreeSet<_>>();

        assert_eq!(openapi_operations, manifest_operations);
    }

    fn openapi_operations(document: &serde_json::Value) -> BTreeSet<(String, String, String)> {
        let mut operations = BTreeSet::new();
        for (path, path_item) in document["paths"].as_object().expect("paths object") {
            for method in ["get", "post", "put", "patch", "delete"] {
                let Some(operation) = path_item.get(method) else {
                    continue;
                };
                operations.insert((
                    method.to_owned(),
                    path.to_owned(),
                    operation["operationId"]
                        .as_str()
                        .expect("operationId")
                        .to_owned(),
                ));
            }
        }
        operations
    }

    fn method_label(method: HttpMethod) -> &'static str {
        match method {
            HttpMethod::Get => "get",
            HttpMethod::Post => "post",
            HttpMethod::Put => "put",
            HttpMethod::Patch => "patch",
            HttpMethod::Delete => "delete",
        }
    }
}
