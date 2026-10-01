//! Source-level regression guards for the PostgreSQL payment provider credential store.
//!
//! Live PostgreSQL integration belongs to the database lifecycle gate. These fast checks keep
//! the advisory-lock execution shape visible in every workspace test run.
//!
//! `pg_advisory_xact_lock` returns SQL `VOID`; only its `pg_try_advisory_xact_lock` sibling
//! returns `bool`. Decoding the blocking lock as a scalar compiles cleanly and then fails at
//! runtime under sqlx 0.9 with `Rust type "bool" (as SQL type "BOOL") is not compatible with
//! SQL type "VOID"`, which aborted `pnpm dev` gateway startup on 2026-10-01.

const PROVIDER_CREDENTIAL_SOURCE: &str = include_str!("../src/provider_credential.rs");

/// Blocking advisory lock. The `pg_try_` variant does not contain this fragment.
const BLOCKING_ADVISORY_LOCK: &str = "pg_advisory_xact_lock(";

#[test]
fn blocking_advisory_lock_calls_are_executed_as_statements() {
    let mut guarded = 0;
    for (index, _) in PROVIDER_CREDENTIAL_SOURCE.match_indices(BLOCKING_ADVISORY_LOCK) {
        let prefix = &PROVIDER_CREDENTIAL_SOURCE[..index];
        let receiver = prefix.rfind("sqlx::query").map(|start| &prefix[start..]);
        let Some(receiver) = receiver else {
            panic!("a blocking advisory lock must be issued through sqlx");
        };
        assert!(
            receiver.starts_with("sqlx::query("),
            "a blocking advisory lock returns SQL VOID and must run through \
             sqlx::query(...).execute(...), not a scalar decode"
        );
        guarded += 1;
    }
    assert!(
        guarded >= 2,
        "expected the rotation and bootstrap advisory locks to stay guarded"
    );
}
