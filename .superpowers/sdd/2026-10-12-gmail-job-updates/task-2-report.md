# Task 2 report: Gmail account storage

Implemented `MailAccount` storage with the three `mail.*` settings and the `gmail` secret provider. Account loading returns `None` if any of the four stored values are missing; saving validates the account and strips whitespace from the app password; removal deletes all three settings and the secret. Added five focused tests for round trip, password normalization, incomplete data, removal, and invalid fields.

The account persistence methods are async because the existing `Store` methods are async. Added `Store::delete_setting` so removal deletes setting rows instead of replacing them with empty values.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml mail::tests`: 5 passed. The Tauri build requires the ignored local `src-tauri/binaries/llama-server-aarch64-apple-darwin` file, which was missing in this worktree. A temporary empty placeholder was created only for each test run and removed immediately afterward.
- `rustfmt --edition 2021 --check src-tauri/src/mail/account.rs src-tauri/src/mail/tests.rs src-tauri/src/mail/mod.rs`: passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: reports formatting differences across existing unrelated files; no repository-wide formatting changes were applied.

The successful Cargo test build emitted an existing linker warning that the `__eh_frame` section is too large for compact unwind offsets.

## Follow-up: keep app passwords out of Debug output

Removed `Debug` from `MailAccount` so its plaintext password cannot be exposed by derived formatting. Updated the equality assertions to avoid requiring `Debug` on the account type.

- `cargo test --manifest-path src-tauri/Cargo.toml mail::tests`: 5 passed (using and removing a temporary placeholder for the missing ignored Tauri sidecar).
- `rustfmt --edition 2021 --check src-tauri/src/mail/account.rs src-tauri/src/mail/tests.rs src-tauri/src/mail/mod.rs`: passed.
