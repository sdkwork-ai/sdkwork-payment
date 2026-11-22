# Unified cashier commercial delivery checklist

This runbook is the operator-facing acceptance path for the unified cashier: the
things that must be true, in order, before the payment centre is considered
deliverable for a paying tenant. Every command below was executed against the
development host during the hardening pass; the numbers are the verified
baselines, not estimates.

## 1. Build and package

The container image packages a **prebuilt release binary**; the Dockerfile is a
single runtime stage and does not compile Rust.

```bash
# 1. release binary (needs pkg-config + libssl-dev; on a host without the build
#    toolchain run this inside rust:1-bookworm with the workspace mounted)
docker run --rm \
  -v "$PWD/..":/src -v "$HOME/.cargo/registry:/usr/local/cargo/registry" \
  -e CARGO_TARGET_DIR=/src/sdkwork-api-cloud-gateway/target \
  -w /src/sdkwork-api-cloud-gateway rust:1-bookworm \
  bash -c 'apt-get update -qq && apt-get install -y -qq pkg-config libssl-dev && \
           cargo build --release --bin sdkwork-api-cloud-gateway \
             --features platform-foundation-suite-container'

# 2. image
cd sdkwork-api-cloud-gateway
node scripts/build-api-cloud-gateway-container.mjs --skip-build \
  --tag registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:<version> \
  --version <version>
```

`--skip-build` reuses `target/release/sdkwork-api-cloud-gateway`; omit it when the
build toolchain is present on the host.

**`libopus0` must be installed in the runtime image.** The audio/opus codec path
records a `DT_NEEDED` entry for `libopus.so.0`; without it the entrypoint exits
`127` before any log line exists. The Dockerfile now declares it.

## 2. Deploy

```bash
cd /opt/deploy/sdkwork-api-cloud-gateway/bundle/compose
cp ../env/development.env ../env/development.env.bak.$(date -u +%Y%m%dT%H%M%SZ)
sed -i 's|^GATEWAY_IMAGE=.*|GATEWAY_IMAGE=registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:<version>|' \
  ../env/development.env

# -p <existing project> is mandatory: without it compose derives a new project from
# the directory name and collides on the published port.
docker compose -p sdkwork-api-cloud-gateway-development-i1 \
  -f docker-compose.bundle.yml -f docker-compose.bundle.embedded.yml \
  --env-file ../env/development.env up -d gateway
```

The bundle's `env/<environment>.env` must define every variable the compose file
interpolates. A missing key fails with `required variable ... is missing a value`;
compare against `docker/env/<environment>.env.example` and fill the gaps, plus
`POSTGRES_ASSETS_DIR` and `GATEWAY_INSTANCE_HOST_PORT` when the bundle expects them.

## 3. Verify after deploy

```bash
curl -s -o /dev/null -w 'healthz=%{http_code}\n' http://127.0.0.1:3910/healthz
curl -s -o /dev/null -w 'readyz=%{http_code}\n'  http://127.0.0.1:3910/readyz

# An authenticated admin route answers 401 when unauthenticated. A 404 means the
# container is still running the previous image.
curl -s -o /dev/null -w 'audit_events=%{http_code}\n' \
  'http://127.0.0.1:3910/backend/v3/api/payments/audit_events?page=1&page_size=5'
```

`healthz`/`readyz` must be `200` and the admin route must be `401`.

## 4. Secrets and rotation

| Secret | Where | Rotation |
| --- | --- | --- |
| Cashier signing KEK | `SDKWORK_COMMERCE_CASHIER_TOKEN_SECRET` (or the durable file mount) | Move the current value into `SDKWORK_COMMERCE_CASHIER_TOKEN_SECRET_RETIRED`, install the new active value, restart. Both keys stay readable during the overlap; only the active key seals. |
| PSP credentials (API keys, webhook secrets, certificates) | `commerce_payment_provider_credential`, sealed by the host-installed credential cipher | Rotate through the admin API; every create/rotate/reveal/delete is written to `commerce_payment_admin_audit_log`. |
| Tenant signing secrets (cashier tokens) | `commerce_payment_cashier_token_setting` | Rotate through the admin endpoint (epoch +1, previous key valid for the overlap window). |

If the credential cipher is not installed, credential writes answer **503
`provider_unavailable`** with an actionable message instead of a generic 500, and
writes without secrets still succeed.

## 5. Migration pre-checks

Migration `0009` adds the unique index on `(tenant_id, provider_code, out_trade_no)`.
It fails loudly rather than keeping data that would let a channel callback settle
the wrong attempt. Check first:

```sql
SELECT tenant_id, provider_code, out_trade_no, COUNT(*)
FROM commerce_payment_attempt
WHERE out_trade_no IS NOT NULL AND deleted_at IS NULL
GROUP BY 1, 2, 3
HAVING COUNT(*) > 1;
```

An empty result means the migration applies cleanly. Otherwise reconcile the rows
by hand (normally keep the `succeeded` attempt and soft-delete the rest).

## 6. Checkout scene policy (platform matrix)

Which provider product serves a buyer is configuration, not code:

```bash
# What would serve an iOS WeChat payment today?
curl -s "$BASE/backend/v3/api/payments/checkout_scene_policy/resolve?scene=membership&app_architecture=app&os_family=ios&payment_method=wechat"

# Add a platform the defaults do not cover (e.g. an iPad app shell).
curl -s -X PUT "$BASE/backend/v3/api/payments/checkout_scene_policies/<id>" \
  -H 'content-type: application/json' \
  -H "Idempotency-Key: $(uuidgen)" -H "X-Request-Id: $(uuidgen)" \
  -d '{"scene":"membership","appArchitecture":"pad","osFamily":"ipados",
       "paymentMethod":"wechat","productCode":"wechat_app","status":"active","priority":10}'
```

The shipped defaults cover `wechat` (`wechat_h5` web, `wechat_jsapi`
mini-program, `wechat_app` iOS/Android), `alipay` (`alipay_pc`, `alipay_wap`,
`alipay_app`) and `stripe` (`stripe_card`, `stripe_apple_pay`,
`stripe_google_pay`) for the `checkout`, `membership` and `points_recharge`
scenes. An unconfigured context reports `configured: false` — it never silently
reuses another platform's product.

## 7. Regression gates (must all be green)

```bash
# payment
cargo test -p sdkwork-payment-providers -p sdkwork-payment-repository-sqlx \
           -p sdkwork-routes-payment-backend-api -p sdkwork-routes-payment-app-api
# order
cargo test -p sdkwork-order-integration-payment -p sdkwork-routes-order-app-api
# utils
cargo test -p sdkwork-utils-rust
# gateway deployment contract + payment spec checks
node scripts/deployment-contract.test.mjs
node ../sdkwork-specs/tools/check-api-operation-patterns.mjs --workspace .
node ../sdkwork-specs/tools/check-pagination.mjs --workspace .
node ../sdkwork-specs/tools/check-api-response-envelope.mjs --workspace .
# database-gated suites need a real postgres
export SDKWORK_DATABASE_TEST_POSTGRES_URL=postgresql://.../sdkwork_payment_test
cargo test -p sdkwork-routes-payment-backend-api \
  --test backend_payment_admin_audit_postgres \
  --test backend_payment_provider_account_atomicity_postgres \
  --test backend_payment_channel_delete_guard_postgres \
  --test backend_payment_checkout_scene_policy_postgres \
  --test backend_payment_provider_accounts_postgres \
  --test backend_payment_refund_postgres \
  --test backend_payment_filtered_lists_postgres
cargo test -p sdkwork-payment-repository-sqlx \
  --test payment_attempt_trade_uniqueness_postgres \
  --test checkout_scene_policy_postgres
```

Verified baselines: payment 270, order 92, utils 209, gateway contract 27, database
gated suites 12, PC cashier 8, H5 4, mini-program 17.

## 8. Rollback

1. Restore the previous `GATEWAY_IMAGE` value from the `development.env.bak.*`
   backup and re-run the compose `up -d gateway` above.
2. Database: run the paired `.down.sql` for the migrations applied in this release
   (`0009`, `0010`, `0011`) — every migration in this repository ships a down file.
3. Secrets: restoring the previous KEK requires the previous value; a KEK rotation
   that already re-sealed secrets cannot be undone, which is why the previous value
   belongs in `..._SECRET_RETIRED` for the overlap window instead of being deleted.
