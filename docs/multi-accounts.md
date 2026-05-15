# Multi-account Codex CLI fork

This fork keeps multi-account behavior isolated in `codex-rs/multi-account`.
Upstream-facing integration points are intentionally small:

- `codex-rs/cli/src/accounts.rs` adds `codex accounts` management commands.
- `codex-rs/login/src/auth/manager.rs` exposes account-store helpers and account switching.
- `codex-rs/core/src/session/mod.rs` records rate-limit snapshots for the active saved account.
- `codex-rs/core/src/session/turn.rs` retries a turn after switching accounts on `usage_limit_reached`.

## Usage

Save the current login as a named account:

```sh
codex login
codex accounts add main
```

Add another account:

```sh
codex logout
codex login
codex accounts add backup
```

Inspect and switch accounts:

```sh
codex accounts list
codex accounts use main
codex accounts remove backup
```

When a turn receives a usage-limit error, Codex marks the active account exhausted until the
server-provided reset time, writes the next available saved account into the normal auth store,
resets the websocket session, and retries the turn.

## Rebase Workflow

Keep local work on a feature branch:

```sh
git switch multi-account-router
git fetch origin
git rebase origin/main
cargo check -p codex-cli
cargo test -p codex-multi-account
```

Most future conflicts should be limited to the four integration points listed above. The account
store format lives at `$CODEX_HOME/multi_accounts/accounts.json` and is independent from upstream
`auth.json`, so upstream auth changes usually require only adjusting the serialize/deserialize
boundary in the CLI and `AuthManager` switch helper.
