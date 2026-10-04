# Business account / Profile persistence

`multizen-core::business` defines the independent account/state/input contract. This does not add fields to Profile/ProfileSummary or import/export old account data. An account is manually registered metadata, **never login evidence**; no password, token, cookie or platform action is involved.

## Tables and transaction boundary

`profile-manager/src/business_accounts.rs::migrate` is invoked by the existing idempotent migration, with a transaction for both new tables/indexes. It uses the manager's original Connection (foreign_keys=ON):

- `business_accounts`: id PK; five checked kebab-case kinds; non-null display_name; optional platform_user_id; optional UNIQUE profile_id FK `profiles(id) ON DELETE SET NULL`; RFC3339 created/updated strings.
- Unique partial index `(kind,platform_user_id) WHERE platform_user_id IS NOT NULL`. Empty platform IDs are normalized to NULL before writes, never a merge key across kinds.
- `business_profile_scopes`: profile_id PK/FK `ON DELETE CASCADE`, checked scope `jinniu|kuaishou`. First binding determines scope. Unbinding deliberately does not remove it because cookies stay in place.

Save validates and writes account + scope within one Connection transaction; no intermediate partial state escapes on SQL/uniqueness/validation errors. Input normalization: displayName.trim(), 1..100 Unicode characters, no control chars; platformUserId.trim(), blank→None, max128, no control chars; only missing/null id creates, any supplied string (including blank) must exactly identify an existing record. Invalid explicit IDs must never silently create a record. Kind is immutable even when unbound. Already bound elsewhere means explicit unbind first, not silent movement. A bound Profile cannot be overwritten by a new account. Duplicate kind+platform ID says to edit/rebind the original record. Cross-scope rebind is refused.

List includes records whose Profile has been removed or unbound. Profile-state of a missing Profile is NotFound. Unbind is idempotent for an existing unbound record and NotFound for unknown ID. Unbind retains account, root and cookies; profile deletion retains account with profileId:null while removing its reservation. There is no account-delete API in this batch.

## Runtime boundary

This library cannot inspect browser liveness and must not depend on browser-launcher. Desktop callers use launcher-thread `save_business_account` / `unbind_business_account` and `update_profile_guarded`, not direct manager writes. These require real launcher `is_running_async` before business mutations and validate actual directory policy before saving. Manager methods are storage primitives; calling them directly does not perform runtime validation.

Tests: `tests/business_accounts.rs` covers original/new DB, repeated migration, reopen, explicit null/normalization, unique binding/kind/ID errors, rebind, preserved scope/cookie marker, FK deletion, trigger-induced INSERT/UPDATE rollback. Core `business.rs` has wire tests.

## Shop-account onboarding boundary (账号即环境)

The Kuaishou shop-account wizard does **not** use `business_accounts`. A shop account *is* a browser Profile: the wizard creates a Profile, signs it in through a hidden window, and reads its Kuaishou ID through the read-only identity path. Keep these consequences:

- **First step is a Chromix-style sectioned form (主页 / 代理 / 扩展 / 指纹, no "General" section).** The home section defaults to `https://s.kwaixiaodian.com/zone/home` (editable); proxy is optional; extensions are staged; the fingerprint section uses the Chromix options component (`ChromixProfileOptions`). All labels are Chinese.
- **The create call sends no `fingerprint` field.** The former CloakBrowser fingerprint is dead — Chromix owns identity. When a proxy is entered the wizard instead sets `chromixOptions.geoip = true` so the SDK aligns locale/timezone to the proxy exit region (the native Chromix alignment, not a separate `fingerprint_reconcile`).

- No new table, column or migration, and no `business_accounts` row is written by onboarding (`git status --porcelain -- crates/profile-manager/` stays empty).
- The shop list renders `profiles_list` + identity snapshots, so there is never a second source of account truth.
- `business_accounts` remains what it was: optional manual registration metadata, never proof of login. Deleting a Profile deletes that shop account (the accepted trade-off), unlike a business record, which survives with `profileId: null`.
