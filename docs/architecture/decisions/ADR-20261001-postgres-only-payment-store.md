# ADR-20261001 Payment Store Is PostgreSQL-Only And Refund Completeness Lives On Refund Rows

Status: accepted
Owner: SDKWork payment maintainers
Date: 2026-10-01
Specs: DATABASE_SPEC.md, ARCHITECTURE_DECISION_SPEC.md, API_SPEC.md, RUST_CODE_SPEC.md

## Context

The payment capability shipped with contradictory dual-engine claims: the
database manifest and DDL baseline declare `engines: [postgres]` only, while
several documents and code comments advertised SQLite behaviors that have no
implementation — a process-local keyed mutex "on SQLite", a
`BEGIN IMMEDIATE` refund guard, and helper docs claiming use "across both
PostgreSQL and SQLite". The workspace SQLx feature set never included
`sqlite`. Documentation described mechanisms that do not exist.

In the same audit window, the payment wire state machine turned out to
advertise `refunding`/`refunded` payment states that no writer ever produced,
while a WeChat `REFUND` payment-status webhook mapped onto those values and
would have violated the `commerce_payment_attempt.status`/`commerce_payment_intent.status`
CHECK constraints at runtime, failing webhook ingestion. Partial refunds make
payment-level refund states wrong by construction: a partially refunded
payment is not `refunded`.

## Decision

1. **PostgreSQL is the only storage engine for sdkwork-payment.** The SQLx
   feature set, every repository implementation, the DDL baseline, and the
   lifecycle manifest stay PostgreSQL-only. All SQLite claims are removed from
   code comments and documentation; this ADR is the canonical statement.
   Should an embedded engine ever be required (single-node self-hosted
   delivery), it is a new architecture effort with its own migration path,
   not an implicit dual implementation.
2. **Refund completeness is tracked on `commerce_refund` rows, not on payment
   rows.** A payment keeps its terminal capture state `succeeded` through full
   or partial refunds (the Stripe model). `refunding`/`refunded` are not valid
   payment-status wire values; the domain machine rejects them, and the
   refundable-amount reservation math (`submitted`+`processing`+`succeeded`
   refunds against the paid amount) is the completeness authority.
3. **The wire machines live in the domain crate.** `validate_payment_wire_transition`
   and `validate_refund_wire_transition` are the single transition authority;
   webhook ingestion delegates to them. The refund machine carries the
   synchronous-PSP edges (`submitted → succeeded`, `failed → succeeded/closed`)
   and creation anchors at `submitted`.
4. **Checkout serialization is PostgreSQL advisory-lock based** and guarded by
   a process-wide connection gate sized to half the pool capacity with a
   bounded wait, because the lock transaction and its follow-up pool queries
   otherwise deadlock the pool at concurrency ≈ pool max.

## Consequences

- WeChat `REFUND` payment-status notifications no longer mutate attempt
  status; refund notifications carry their own refund-status family and are
  applied to refund rows.
- Documents must not describe engines or transitions that the code does not
  implement; the manifest `engines: [postgres]` is the storage contract.
- Consumers keying on a payment-level `refunded` state must read refund rows
  (or the refundable-amount summary) instead.

## Verification

- `cargo test --workspace` — domain machine tests pin the accepted wire set
  and the rejected refund-derived payment states.
- `database/database.manifest.json` declares `engines: [postgres]`.
- `crates/sdkwork-payment-repository-sqlx` compiles with no `sqlite` feature.
