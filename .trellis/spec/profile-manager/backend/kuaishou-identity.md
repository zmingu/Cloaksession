# Kuaishou identity persistence

`multizen-core/kuaishou_identity.rs` owns the fixed snapshot/status DTO and internal Observation envelope (snapshot/session_id/avatar_url). Runtime UUID and remote avatar URL are not renderer fields. `profile-manager/kuaishou_identity.rs` owns two additive tables created by run_migrations:

- `kuaishou_identities`: platform_user_id TEXT PK, nickname/avatar_key/avatar_url and last_seen_at. Independent platform archive, survives Profile deletion. Only detected observations update this table; ID is full 5..32 ASCII digits, not a numeric JS/Rust value.
- `kuaishou_identity_observations`: profile_id PK/FK ON DELETE CASCADE, JSON-valid observation envelope. One latest state per Profile. Snapshot nulls and RFC3339 strings round-trip; old success identity fields remain historical on errors/not-detected/conflict/closed/skipped.

`kuaishou_identity_save` runs a transaction on the original connection, checks Profile exists and compares the complete expected BusinessProfileState with current state. Changed binding/alias/ID/scope => Ok(None), no writes. Retained jinniu scope or nonshop account => skipped. Different manually registered ID => conflict. Neither table mutates business_accounts, scopes, Profile name or aliases. A new detected ID never receives old Profile avatar data. Successful avatar references stay in the archive after Profile removal; observations are deleted by FK.

The runtime caller must additionally hold the current registry session through this synchronous transaction and verify the session UUID/cancellation/deadline. This library cannot inspect browser liveness. Stored detected is historical after app restart: the app compares the recorded UUID against the current session and returns unknown/closed when not current. Do not expose raw stored status directly as login state.

API: observations(), observation(profile_id), save(observation, expected_business), avatar_referenced(key), all prefixed `kuaishou_identity_`. Avatar retrieval checks references in platform archive or observation; file validation is the Tauri avatar owner. Remote URLs are internal reuse metadata only.

`tests/kuaishou_identity.rs` covers original-schema/repeated migration, reopen, retained avatar reference on delete, last-success retention, changed-ID clearing, scope skipping, manual mismatch/no mutation, stale unbind rejection and malformed success. Tests use local SQLite/tempdirs, no platform traffic.
