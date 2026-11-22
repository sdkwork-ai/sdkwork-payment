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

## 9. Synchronising the workspace before verification

The gateway and the router service depend on sibling repositories by **path**, so a
checkout that pulls only the service under test can fail to compile for reasons that
are not in that service:

| Consumer | Path dependency that must be synced |
| --- | --- |
| sdkwork-cloudrouter | `../sdkwork-models` (`sdkwork_models_catalog_service`) |

Observed failure mode: `error[E0407]: method candidate_hashes is not a member of
trait ApiKeySecretHasher` in the router service. The trait was present in
`sdkwork-models` `origin/main`; the sibling checkout was two commits behind. Sync
every submodule the build touches before treating a compile error as a regression:

```bash
cd /opt/source/sdkwork-space
for r in sdkwork-payment sdkwork-order sdkwork-utils sdkwork-cloudrouter \
         sdkwork-api-cloud-gateway sdkwork-models; do
  git -C "$r" fetch --quiet origin && git -C "$r" pull --ff-only --quiet origin main
done
```

## 10. Golden and locked-count tests

`openapi_billing_matrix` fails when an endpoint is added or its billing class
changes, which is intended: every difference is a billing change. When the change is
intended, refresh and move the locked counts in the same commit:

```bash
cd sdkwork-cloudrouter
UPDATE_BILLING_MATRIX=1 cargo test -p sdkwork-cloudrouter-router-service \
  --test openapi_billing_matrix
# then update the literals in commercial_readiness_counts_are_exactly_*_of_*:
#   total operations, Free count, sellable count
```

The refresh must show only the intended endpoint rows in the golden diff; anything
else is a real regression.

## 11. Known gaps at handover

These are the items that were **not** completed in this pass. They are listed so the
delivery boundary is auditable rather than implied.

1. **Checkout scene policy is not yet consulted by the product resolver.** The
   table, resolver, dry-run endpoint, CRUD, audit and default seeds are live, but the
   checkout path still derives the product from the method catalog. Closing this
   needs the client to report its OS (`ios`/`android`/`harmony`) next to the scene
   and client type; wiring it before that would route app buyers to the H5 fallback
   rule, which is worse than the current behaviour.
2. **Flutter native payment launch** is implemented only up to the contract: the
   client resolves a launch plan and tells the payer what to do, but the WeChat/Alipay
   SDK invocation and the browser/QR surfaces need the platform plugins.
3. **H5 membership and points-recharge purchase entry** do not exist yet; the
   membership subscription page component exists but nothing configures its runtime
   in the H5 host.
4. **Payment admin "operation history" screen** is not built; the audit API it needs
   is live.
5. **Non-payment security gaps found in the development environment** and still
   open: the memory web runtime logs that authorization policy, tenant isolation,
   Redis rate limiting, idempotency, admission control and request deadlines are
   disabled; the LR store warns that upstream API keys are stored in plaintext
   without its encryption secret; the deploy service warns that certificate custody
   is unconfigured. None of these regress the payment flows, but each must be
   fail-closed before a production claim.
6. **An image built from the current workspace `main` refuses to start.**
   `sdkwork-api-cloud-gateway:0.1.109` (built from the synced siblings) panics during
   route composition:

   ```
   cloud router backend-api manifest composition failed: composed route collision for
   GET /backend/v3/api/mcp/categories: sdkwork-mcp (mcpAdmin.listCategories)
   conflicts with sdkwork-cloudrouter (categories.list)
   ```

   The guard is working as designed — a duplicate path between two composed modules
   is rejected instead of leaving one handler silently shadowed — but one of the two
   declarations has to move before a new image can be promoted. The development stack
   was rolled back to `0.1.108-smoke`, which was verified healthy
   (`healthz=200`, `readyz=200`).