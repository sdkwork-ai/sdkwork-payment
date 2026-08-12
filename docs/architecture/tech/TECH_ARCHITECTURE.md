# Payment Technical Architecture

Specs: ARCHITECTURE_DECISION_SPEC.md, DOCUMENTATION_SPEC.md, API_SPEC.md, WEB_FRAMEWORK_SPEC.md, WEB_BACKEND_SPEC.md, SECURITY_SPEC.md, PAGINATION_SPEC.md

Status: active
Owner: SDKWork maintainers
Updated: 2026-07-26

## 1. Architecture Overview

`sdkwork-payment` owns the **payment executor** for the SDKWork commerce domain: payment intents, attempts, owner-order pay side-effects (via order orchestration), refunds, backend admin (methods, providers, channels, webhook event storage, reconciliation). PSP webhooks are **HTTP-owned by sdkwork-order**; payment exposes ingest ports only. Points recharge is **not** in this repository — use `sdkwork-order` (`/app/v3/api/recharges/*`).

**Dependency rule:** `sdkwork-payment` must not take a crate dependency on `sdkwork-order`. Order orchestration calls payment in-process; payment validates `orderId` via read-only SQL in `order_reference.rs` and owns `commerce_payment_method` listing. Shared pay/settlement types are defined in `sdkwork-payment-service`. See `specs/commerce-dependency-boundary.spec.json`.

## Capability stack

| Layer | Path |
| --- | --- |
| Domain contracts (Rust) | `crates/sdkwork-payment-service/` |
| PSP adapters (Stripe/Alipay/WeChat) | `crates/sdkwork-payment-providers/` |
| SQL repositories | `crates/sdkwork-payment-repository-sqlx/` |
| HTTP routers | `crates/sdkwork-routes-payment-app-api/`, `crates/sdkwork-routes-payment-backend-api/` |
| Gateway assembly | `crates/sdkwork-api-payment-assembly/` |
| API server | `crates/sdkwork-api-payment-standalone-gateway/` |
| PC application | `apps/sdkwork-payment-pc/` |
| TypeScript facade | `apps/sdkwork-payment-common/packages/sdkwork-payment-service/` |

### Admin console packages (`apps/sdkwork-payment-pc/packages/`)

| Package | Responsibility |
| --- | --- |
| `sdkwork-payment-pc-admin-core` | Shared infrastructure: `AdminFieldLabel`, `ConfirmDialog`, `CopyButton`, `SdkworkPaymentListPaginationControls`, provider/filter/payment-method constants, coercion helpers, standard exports (sdk/modules/host/session) |
| `sdkwork-payment-pc-admin-provider` | Provider account management: list/create/update/test/rotate credentials + sub-merchant CRUD (Alipay sub_appid / WeChat sub_mch_id / Stripe Connected Account) |
| `sdkwork-payment-pc-admin-channel` | Channel and route rule management: payment methods, channels (scene_code mapping), route rules (priority-based provider selection) |
| `sdkwork-payment-pc-admin-devconfig` | Dev config: certificate CRUD, environment switcher, webhook event integration logs + replay, webhook debugger (sandbox trigger + signature test) |
| `sdkwork-payment-pc-admin-monitor` | Operations monitoring: payment intents, payment attempts, webhook events (with signature status + payload viewer), reconciliation runs |

Each admin package follows the same controller pattern: `createSdkWorkPagedListSession` for paged server-side lists, defensive `map*` projections from `unknown` SDK payloads, and a React-friendly external store (`subscribe` / `getState`). All packages consume `SdkworkPaymentBackendService` via the port-adapter-service pattern (APP_SDK_INTEGRATION_SPEC.md §9); they never import `@sdkwork/payment-backend-sdk` directly.

## API ownership

- App API prefix: `/app/v3/api/payments`, `/app/v3/api/refunds`
- Backend API prefix: `/backend/v3/api/payments`
- Table prefix: `commerce_`

## HTTP contract layer

### SdkWorkApiResponse envelope (`API_SPEC.md` §4.5 / §15 / §16)

All app-api and backend-api success handlers use `api_response.rs` helpers:

- Single resource: `{ "code": 0, "data": { "item": T }, "traceId": "..." }`
- Lists: `{ "code": 0, "data": { "items": [...], "pageInfo": { "mode": "offset", ... } }, "traceId": "..." }`
- Commands: `{ "code": 0, "data": { "accepted": true, "resourceId"?: "...", "status"?: "..." }, "traceId": "..." }`

Errors use HTTP 4xx/5xx `application/problem+json` (`SdkWorkProblemDetail`) with numeric platform `code` and `traceId`. All error helpers set `Content-Type: application/problem+json` explicitly.

### Provider integrations (`sdkwork-payment-providers`)

| Provider | Create | Query | Close | Refund | Webhook verify |
| --- | --- | --- | --- | --- | --- |
| `stripe` | PaymentIntent + `clientSecret` | GET intent | cancel | POST refund | HMAC-SHA256 |
| `alipay` | `trade.precreate` → `qrCodeUrl` | `trade.query` | `trade.close` | `trade.refund` | RSA2 form sign |
| `wechat_pay` | Native → `code_url` | out-trade-no query | close | domestic refund | platform RSA + AES-GCM |

- Registry: tenant-scoped `commerce_payment_provider_account` rows load encrypted, versioned credentials from `commerce_payment_provider_credential`; the adapter receives plaintext only in memory. `PaymentProviderRegistry::from_env()` remains a migration fallback for legacy deployments.
- Routability: bootstrap catalog, channels, and accounts live in tenant default organization `0`. Authenticated organization rows override organization `0`, which overrides legacy unscoped rows. Catalog methods and channels may be pre-enabled, but a channel bound to a provider account is returned and accepted only while that account is active. Multiple active accounts at the winning tenant/organization/provider scope fail closed until deterministic channel routing is configured.
- Pay flow: after repository persists intent/attempt, shared `enrich_owner_order_payment_*` (`owner_order_checkout.rs`) first revalidates that the current attempt is still active, resolves the immutable channel/provider-account snapshot, closes conflicting active attempts through their historical provider accounts, then calls the selected PSP and atomically merges `providerTransactionId` / `providerStatus` into attempt `callback_payload` for later close/cancel. A waiting attempt superseded by another checkout returns conflict before any provider call. PSP close failure leaves the old attempt locally active and prevents a second provider checkout; an explicit absent/already-closed provider response is the idempotent success case because there is no external trade left open. The whole revalidate -> close -> provider checkout -> persistence sequence is serialized per tenant/organization/Order by a process-local keyed mutex on SQLite and a bounded, transaction-scoped PostgreSQL advisory lock; PostgreSQL pools must allow at least two connections so the lock owner can execute repository work while holding the dedicated lock transaction.
- Reconcile (app): `POST /payments/reconcile` is a **lookup** command that returns the latest payment record for `orderId` or `outTradeNo`; PSP status repair is not performed inline (use backend webhook replay or order settlement).
- Close: `POST /payments/{paymentId}/close` invokes the bound historical PSP channel before committing the local terminal state; PSP failure leaves the payment retryable (Stripe uses `providerTransactionId` from attempt `callback_payload` when present).
- Refund: `POST /refunds` persists the refund row, then submits `create_refund` to the PSP with up to three transient retries. Explicit terminal provider success becomes `succeeded`; explicit terminal provider failure becomes `failed`; accepted, pending, unknown, and ambiguous network/provider outcomes remain `processing` and continue reserving refundable amount. App idempotency replay and backend retry can resubmit both `processing` and `failed` records with the same refund number and PSP idempotency key. Deterministic local validation/context failures become `failed`. The Attempt provider and historical account provider must match before any refund request is sent.
- Checkout expiry and polling: each provider checkout expires at `min(order expiry, creation time + 15 minutes)`. The exact value is persisted on the attempt, sent as WeChat `time_expire` or Alipay `timeout_express`, and returned in payment params as `expiresAt`. Because Alipay accepts minute-granularity `timeout_express`, checkout fails closed when less than 60 seconds remain instead of rounding up beyond the Order boundary. `GET /payments/checkout/{paymentId}` re-enriches only unexpired pending attempts; an expired provider checkout creates a fresh attempt under the same payable Order.
- Webhook ingest: Payment resolves the attempt by `provider_code` plus `outTradeNo`, applies tenant and organization scope when available, and relies on the database unique constraint `(tenant_id, provider_code, out_trade_no)` to keep identity unambiguous. The resolved payment attempt id is carried into Order settlement. Events without resolvable `outTradeNo` are persisted as `unmatched` only when tenant scope is available.
- Sandbox: when `provider_code` is `sandbox` or PSP credentials are absent, local cashier URLs from `sdkwork-utils-rust` are used without external HTTP.

### Provider and async processing

- `SandboxPaymentProvider` remains for contract tests and offline draft generation.
- Backend admin `webhook_events` replay re-applies stored payment attempt status inline; order settlement uses order `payment_confirmations`.

### Webhook replay (admin)

Replay increments `retries` atomically with `COALESCE(retries, 0) < 5`; limit exceeded → 409, missing event → 404. `POST .../webhook_events/{eventId}/replay` requires `Idempotency-Key` and `Sdkwork-Request-Hash`; response uses command envelope (`data.accepted`).

### Payment methods catalog

`GET /payments/methods` resolves one effective catalog row per `method_key` using authenticated organization -> organization `0` -> legacy unscoped precedence, joins only channels/accounts visible in that same effective scope, maps active `commerce_payment_channel.scene_code` values to API `productTypes` (`web` -> `pc`, `app`, `mini_program`, `api`), and paginates in SQL (`page`/`page_size`, `data.items` + `pageInfo`). Optional `clientType` filters by channel `scene_code` in the repository layer (not in-process). A channel configured only for another organization never makes a method eligible and never enables deployment-credential fallback.

### Route manifest

- `sdkwork-routes-payment-app-api/src/http_route_manifest.rs`
- `sdkwork-routes-payment-backend-api/src/http_route_manifest.rs`

Manifests are injected via `WebFrameworkLayer::with_route_manifest`. Idempotent write routes require `Idempotency-Key` and `Sdkwork-Request-Hash` at the handler layer.

### Pagination (`PAGINATION_SPEC.md` §2)

List/search endpoints push `page` / `page_size` to SQL `LIMIT`/`OFFSET` with `COUNT(*) OVER()` (or equivalent aggregate) in the repository layer. Covered paths include payment records, order payments, refunds, backend admin lists, and **app payment methods**. Process-memory `fetch_all` + `skip`/`take` is forbidden on P0 paths.

### Idempotency and transactions

- Owner-order pay: `PayOwnerOrderCommand` carries `idempotency_key` + `request_no`; repository replays by `(tenant_id, order_id, idempotency_key)` and uses deterministic intent/attempt IDs.
- Same-method checkout reuses an active attempt only when its immutable `paymentScene` and `paymentMetadata` snapshots match the new command. A different HTTP idempotency key alone does not create another PSP trade; a different scene or payer snapshot creates a separate Attempt instead of returning incompatible cashier parameters. PSP enrichment reloads the attempt's original provider, channel, provider account, merchant trade number, business idempotency key, scene, and payer metadata from persistence. Every Attempt stores `request_no` and `idempotency_key`. Domain callback fields and the provider-input snapshot coexist in `callback_payload`. PSP create/cancel/refund keys are fixed-width ASCII SHA-256 derivatives, and new merchant trade numbers are deterministic 32-character ASCII identifiers.
- Order references include `expired_at`. Intent and attempt creation fail closed when the Order expiry is missing, empty, expired, or invalid. Attempts initially inherit the Order boundary, then provider enrichment narrows it to at most 15 minutes. Reusable-attempt queries require explicit future expiry timestamps on both the attempt and Order.
- Webhook and confirmation settlement lock records in `order -> payment_intent -> payment_attempt` order. Webhooks confirm the exact resolved attempt; order-only manual confirmation is accepted only when one matching attempt is unambiguous.
- Payment timestamps use UTC RFC3339 at service boundaries. PostgreSQL stores and reads `TIMESTAMPTZ`; SQLite stores the same RFC3339 representation as text. Confirmation replay returns the first persisted non-empty `paid_at`.
- Refunds: idempotency replay + transactional refund-sum guard under `BEGIN IMMEDIATE` (SQLite) / `FOR UPDATE` (PostgreSQL). `submitted`, `processing`, and `succeeded` amounts remain reserved; an ambiguous PSP result is not downgraded to `failed` and cannot free amount for a duplicate refund. Processing recovery queries Stripe by `pi_*` plus exact `metadata.refund_no`, and queries Alipay/WeChat by the original merchant payment/refund numbers. Terminal transitions update the refund row and append the matching audit event in one transaction; a concurrent status change makes the losing transition fail closed.
- Close / cancel / reconcile: command headers enforced at handler; close is idempotent when record already terminal.
- Provider close commits local cancellation only from an active attempt. A concurrent successful webhook has precedence and returns conflict, which stops payment-method replacement before another PSP trade is created.
- Checkout expiration cleanup is limited to the currently locked Order; cross-Order expired cleanup belongs to the maintenance operation. Stripe refunds resolve the original persisted `pi_*` transaction id, while Alipay and WeChat resolve the merchant trade number.
- Domain wire transitions (`validate_payment_wire_transition` / `validate_refund_wire_transition`) enforced on cancel, close, refund create, and owner-order payment confirmation.

### IAM boundary (backend-api)

`backend_runtime_subject_from_extension` enforces organization session, `can_access_backend_api()`, and tenant scope from IAM context (never from URL).

## Data stores

The authoritative Payment database has one DDL baseline at `database/ddl/baseline/postgres/`. The manifest declares `databaseRole: authoritative-server` and `engines: [postgres]`; SQLite fixtures and lifecycle runs are not release or compatibility evidence for this service boundary.

## Production hardening

### Legacy PSP environment-variable fallback

| Variable | Provider | Purpose |
| --- | --- | --- |
| `ORDER_PAYMENT_WEBHOOK_BASE_URL` | all | Base URL for `{base}/app/v3/api/orders/payments/webhooks/{providerCode}` notify endpoints (order gateway) |
| `STRIPE_SECRET_KEY` | stripe | API secret |
| `STRIPE_WEBHOOK_SECRET` | stripe | Webhook HMAC verification |
| `ALIPAY_APP_ID` | alipay | Application ID |
| `ALIPAY_PRIVATE_KEY_PEM` | alipay | Merchant RSA private key (PEM) |
| `ALIPAY_PUBLIC_KEY_PEM` | alipay | Alipay RSA public key for response verify |
| `ALIPAY_NOTIFY_URL` | alipay | Optional override notify URL |
| `WECHAT_PAY_MCH_ID` | wechat_pay | Merchant ID |
| `WECHAT_PAY_APP_ID` | wechat_pay | App ID |
| `WECHAT_PAY_API_V3_KEY` | wechat_pay | API v3 key |
| `WECHAT_PAY_MERCHANT_SERIAL_NO` | wechat_pay | Merchant certificate serial |
| `WECHAT_PAY_PRIVATE_KEY_PEM` | wechat_pay | Merchant RSA private key (PEM) |
| `WECHAT_PAY_PLATFORM_PUBLIC_KEY_PEM` | wechat_pay | WeChat platform certificate (PEM) |
| `SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED` | sandbox | Register the **unsigned** sandbox webhook adapter (accepts any body). Default: only in dev/test environments (`SDKWORK_ENVIRONMENT`). Production must never enable it — with the webhook route public, a forged sandbox webhook could settle an order as paid |

### Tenant provider accounts (`commerce_payment_provider_account`)

Backend admin upserts (methods, provider accounts, channels, route rules) and reconciliation run creation use `success_command_accepted` (`data.accepted` + optional `resourceId`). Provider credential inputs are write-only and encrypted before database persistence. At runtime pay/close/refund resolve the active account for `(tenant_id, organization_id, provider_code)`, decrypt its active credential versions, and merge them into the PSP registry.

| Field | Purpose |
| --- | --- |
| `commerce_payment_provider_credential` / `primary_secret` | Encrypted Stripe secret key or Alipay/WeChat merchant private key PEM |
| `commerce_payment_provider_credential` / `webhook_secret` | Encrypted Stripe webhook secret or WeChat API v3 key |
| `commerce_payment_provider_credential` / `certificate` | Encrypted Alipay public key or WeChat platform certificate PEM |
| `merchant_id` | Alipay `app_id` or WeChat `mch_id` |
| `metadata` | JSON extras: `appId`, `merchantSerialNo`, `notifyUrl`, `returnUrl`; production seeds start with explicit mock identifiers that can be replaced without changing adapter code |

WeChat product routing uses `paymentMethod` as the upstream V3 product key. `wechat_jsapi` requires `payerOpenId`; `wechat_h5` requires `clientIp`; Native and App do not require those payer fields. `paymentScene` remains a client/channel scene selector and is not substituted for the provider product key.

Credential envelopes use `PaymentCredentialCipher` with AES-256-GCM and an HKDF context bound to tenant, provider account, and credential kind. Filesystem discovery belongs to `PaymentServiceHost`, not the provider library. Development and test hosts create the local wrapping key once at `~/.sdkwork/commerce/secrets/payment-credential-master.key`; `SDKWORK_PAYMENT_CREDENTIAL_MASTER_KEY_FILE` may select another absolute path outside every source checkout. Production-like hosts never create an implicit local key: they must either point that variable at an existing protected shared secret file (for example `/etc/sdkwork/commerce/payment-credential-master.secret`) or install a KMS-backed implementation through `install_payment_credential_cipher` before Payment bootstrap. The wrapping key is never stored in the payment database. Back up or centrally manage it before running multiple replicas, because losing it makes stored credentials intentionally undecryptable.

- CORS: `PAYMENT_API_CORS_ORIGINS` whitelist (no `*`)
- Graceful shutdown, 30s request timeout, 1 MiB body limit
- Structured tracing via `WebRequestContext` / `x-sdkwork-trace-id`

## Verification

```powershell
cd E:\sdkwork-space\sdkwork-payment
cargo test --workspace
pnpm verify
node ../sdkwork-specs/tools/check-api-response-envelope.mjs --workspace .
node ../sdkwork-specs/tools/check-pagination.mjs --workspace .
```

## Related docs

- PRD: `docs/product/prd/PRD.md`
- Payment executor boundary: `specs/PAYMENT_EXECUTOR_SPEC.md`
- Backend API OpenAPI contract: `apis/backend-api/payment/sdkwork-payment-backend-api.openapi.yaml`
- Commerce migration: `../sdkwork-specs/MIGRATION_SPEC.md` §8
