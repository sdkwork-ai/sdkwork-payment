# Payment PRD

Status: active
Owner: SDKWork maintainers
Application: payment
Updated: 2026-07-26
Specs: REQUIREMENTS_SPEC.md, DOCUMENTATION_SPEC.md

## Document Map

- Commerce repository dissolution: `../sdkwork-specs/MIGRATION_SPEC.md` §8

## 1. Background And Problem

Payments, intents, refunds, and provider admin require strict idempotency, auditability, and provider isolation.

This repository is a **T1 commerce capability building block**. The commerce monolith has been dissolved; this repository is self-contained with its own domain logic, persistence, HTTP route builders, API server, and IAM middleware for the **payment** capability.

## 2. Target Users

Buyers, finance operators, payment integrators, and reconciliation staff.

## 3. Goals And Non-Goals

### Goals

- Own payment SQL, app payment/refund surfaces, and backend payment admin routes.
- Keep write operations protected by command headers and tenant-scoped stores.
- Make payment accounts the primary provider configuration object and expose advanced channel and routing composition progressively.
- Give payment operators a tenant-scoped, auditable refund execution and recovery workflow without replacing order-domain approval.

### Non-Goals

- Order header lifecycle and points recharge checkout (owned by order capability).
- Raw provider HTTP without domain service boundaries.

## 4. Scope

- Payment methods, records, statistics, reconcile flows.
- Payment intents, attempts, and owner-order payment orchestration.
- Refunds.
- Backend payment admin: methods, payment accounts, channels, route rules, sub-merchants, certificates, refund operations, webhook event management, reconciliation.

Primary API prefixes:

- App: `/app/v3/api/payments`, `/app/v3/api/refunds`
- Backend: `/backend/v3/api/payments`

Migration status: **complete** (Phase 5 production hardening closed — see PRD §10).

## 5. Payment Method Catalog

The system supports 16 payment method keys across 5 providers, defined in `admin-constants.ts` and the backend OpenAPI `PaymentMethod` schema:

| Method Key | Label | Provider | Description |
| --- | --- | --- | --- |
| `stripe_card` | Credit / Debit Card | stripe | Visa, Mastercard, Amex, Discover, JCB, UnionPay via Stripe |
| `stripe_apple_pay` | Apple Pay | stripe | Apple Pay wallet via Stripe (requires Dashboard + domain verification) |
| `stripe_google_pay` | Google Pay | stripe | Google Pay wallet via Stripe (requires Dashboard configuration) |
| `stripe_alipay` | Alipay (cross-border) | stripe | Alipay via Stripe for cross-border CNY settlement |
| `stripe_wechat_pay` | WeChat Pay (cross-border) | stripe | WeChat Pay via Stripe for cross-border settlement |
| `alipay_qr` | Alipay In-store QR | alipay | `alipay.trade.precreate` — merchant scans buyer QR |
| `alipay_pc` | Alipay PC Website | alipay | `alipay.trade.page.pay` — desktop browser redirect |
| `alipay_wap` | Alipay WAP (Mobile) | alipay | `alipay.trade.wap.pay` — mobile browser redirect |
| `alipay_app` | Alipay App | alipay | `alipay.trade.app.pay` — native App SDK |
| `alipay_jsapi` | Alipay JSAPI | alipay | `alipay.trade.create` — in-page JSAPI (requires buyer_id) |
| `wechat_native` | WeChat Pay Native (QR) | wechat_pay | `/v3/pay/transactions/native` — buyer scans merchant QR |
| `wechat_jsapi` | WeChat Pay JSAPI | wechat_pay | `/v3/pay/transactions/jsapi` — Official Account / Mini Program (requires openid) |
| `wechat_h5` | WeChat Pay H5 | wechat_pay | `/v3/pay/transactions/h5` — mobile browser (requires client_ip) |
| `wechat_app` | WeChat Pay App | wechat_pay | `/v3/pay/transactions/app` — native App SDK |
| `paypal` | PayPal | paypal | PayPal Orders v2 — hosted approval redirect, capture via webhook, refund via `/v2/payments/captures/{id}/refund` |
| `sandbox_test` | Sandbox Test | sandbox | Local cashier URL — no external HTTP |

For payment creation, `wechat_jsapi` requires `payerOpenId` and `wechat_h5` requires `clientIp`. The selected payment method, rather than the generic UI/client scene, determines the upstream WeChat V3 endpoint.

## 6. Webhook Event Management

Inbound PSP webhook events are stored and managed through the backend admin API:

- **Event lifecycle**: `queued` → `processing` → `processed` (success) or `failed` (transient) → `dead` (exhausted retries, max 5)
- **Signature verification**: events carry `signatureStatus` (`valid` / `invalid` / `unverified` / `unknown`) mirroring Stripe Dashboard's webhook signature indicator
- **Replay**: `POST /backend/v3/api/payments/webhook_events/{eventId}/replay` re-applies stored event payload; capped at `WEBHOOK_STORED_ADMIN_WEBHOOK_REPLAY_MAX_RETRIES` (5)
- **Sandbox trigger**: `POST /backend/v3/api/payments/dev/sandbox_trigger` simulates a PSP webhook event for development/sandbox environments
- **Signature test**: `POST /backend/v3/api/payments/dev/webhook_signature_test` verifies a raw payload + signature against the provider account's encrypted database credential

## 7. Reconciliation

Reconciliation runs compare internal payment records against PSP settlement reports:

- **Run types**: `daily`, `weekly`, `monthly`, `manual`, `settlement`
- **Run status lifecycle**: `pending` → `queued` → `running` → `succeeded` / `failed` / `canceled`
- **Metrics**: `matchedCount`, `mismatchedCount`, `unmatchedCount`, `totalDifferenceAmount`
- **Create**: `POST /backend/v3/api/payments/reconciliation_runs` creates a new reconciliation run
- **App reconcile lookup**: `POST /app/v3/api/payments/reconcile` is a lookup command that returns the latest payment record for an `orderId` or `outTradeNo` (PSP status repair is not performed inline)

## 8. User Scenarios

- A buyer pays for a pending order; payment record transitions to success with idempotent writes.
- An operator creates a payment account first, then optionally composes methods, channels, and routing rules for advanced traffic orchestration.
- An authorized operator initiates or retries a refund against the original successful payment attempt and provider account, with tenant isolation, amount bounds, idempotency, and audit attribution.
- An operator manages sub-merchants, reviews webhook event signatures, and replays failed events from the admin console.
- A reconciliation staff member creates reconciliation runs and reviews matched / mismatched / unmatched counts.

## 9. Success Metrics

- Payment standard tests pass in payment service crate.
- Commerce standalone-gateway payment tests pass through IAM thin wrappers.

## 10. Phases

- Phase 1 (complete): payment SQL + app/backend routers migrated.
- Phase 2 (complete): payment_intent/refund SQL owned by payment repository.
- Phase 3 (complete): SDK contract route `/payments/attempts/{paymentAttemptId}` owned by payment app router.
- Phase 4 (complete): owner-order pay/cancel payment side-effects owned by owner-order payment stores.
- Phase 5 (complete): production hardening: command envelopes on all write routes, SQL pagination (including app payment methods), PSP enrichment with atomic `callback_payload` persistence, checkout re-enrichment, provider close before terminal local commit, refund PSP submit with transient retry, explicit provider terminal-state propagation, ambiguity-safe `processing` recovery, webhook audit for unmatched events, and envelope/trace alignment per `API_SPEC.md` / `PAGINATION_SPEC.md`.
- Phase 6 (complete): order-expiration enforcement, attempt uniqueness, order-scoped checkout serialization, and deterministic organization inheritance. Payment rejects Order references with missing, empty, invalid, or expired payment boundaries, persists `min(order expiry, provider checkout TTL)` on each attempt, sends the provider expiry for WeChat and Alipay checkout, and fails closed when Alipay's minute-granularity timeout cannot fit inside the remaining Order lifetime. It does not reuse stale or boundary-less pending attempts, closes conflicts and current-Order expired attempts through their historical PSP account, serializes close/create/persist per Order across concurrent requests, and enforces tenant/provider/out-trade-number uniqueness before webhook identity resolution. Payment methods, channels, and accounts resolve authenticated organization first, tenant default organization `0` second, and legacy unscoped rows last; method discovery and checkout share the same isolation and fail-closed eligibility rules. Same-method replay reuses the original Attempt only when `paymentScene` and `paymentMetadata` match, so a new click-level idempotency key alone cannot create another upstream trade while a scene or payer change cannot return incompatible cashier parameters. Provider identifiers and operation keys are deterministic, bounded, and PSP-compatible; concurrent settlement wins over close; Stripe refunds use the persisted native PaymentIntent identity; every historical operation verifies the account/provider snapshot; explicit terminal refund responses propagate transactionally, while ambiguous outcomes retain their original refund identity and reserved amount.

## 11. Linked Requirements

- Payment account and refund operations: `../requirements/REQ-2026-0001-payment-account-refund-operations.md`
- Payment execution hardening and simplification: `../requirements/REQ-2026-0002-payment-execution-hardening.md`
- Commerce repository dissolution: `../sdkwork-specs/MIGRATION_SPEC.md` §8
- Component contract: `specs/component.spec.json`
- Machine contracts: local `specs/`, `database/ddl/`, route manifests

## 12. Open Questions

- Provider credential storage encryption policy and `PaymentProviderPort` implementation before external channel go-live.
- A dedicated async refund-retry worker deployment topology (queue consumer) remains an operational automation enhancement. Correctness does not depend on it: app/backend same-identity retry and webhook/reconciliation keep ambiguous refunds reserved and recoverable.
