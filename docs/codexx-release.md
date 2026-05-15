# codexx Release

This fork keeps OpenAI Codex's normal `CODEX_HOME` layout, but installs a
separate launcher named `codexx`.

`codexx` exports `CODEX_AUTH_PROFILE=codexx` before starting the upstream
`codex` binary. That means it shares the normal `~/.codex` config, history, MCP
servers, plugins, and project state, while keeping credentials separate from
official Codex:

- official Codex: `~/.codex/auth.json`
- codexx: `~/.codex/auth-codexx.json`
- codexx account router state: `~/.codex/multi_accounts/accounts.json`

When `codexx` starts and has no profiled credentials yet, it can import the
default Codex credentials into the profiled store. Later token refreshes, logout,
and account switches stay inside the `codexx` profile.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/xiusmo/codex/codexx/scripts/install-codexx.sh | sh
```

Install a specific release:

```sh
curl -fsSL https://raw.githubusercontent.com/xiusmo/codex/codexx/scripts/install-codexx.sh | sh -s -- --release codexx-v0.1.0
```

Update by running the same install command again.

## Release

Create and push a `codexx-vX.Y.Z` tag:

```sh
git tag -a codexx-v0.1.0 -m "Release codexx 0.1.0"
git push origin codexx-v0.1.0
```

Then build and upload the current machine's native artifact:

```sh
scripts/release-local-codexx.sh codexx-v0.1.0
```

The fork-specific workflow at `.github/workflows/codexx-release.yml` is kept as
a manual fallback only. It does not use OpenAI's official signing, npm, PyPI,
Winget, DotSlash, or website publishing pipeline.
