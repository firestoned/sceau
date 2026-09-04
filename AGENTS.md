# sceau

*sceau* (French for "seal") is a Kubernetes KMS v2 gRPC plugin written in Rust
that encrypts etcd/secrets at rest by sealing data encryption keys (DEKs) with
a TPM 2.0. It targets Kairos OS hosts running k0s.

## Rules — read first

Read and follow **every** file in `.claude/rules/` before writing code:

- `.claude/rules/architecture-driven-development.md` — the governing methodology
- `.claude/rules/testing.md` — TDD is mandatory; tests in separate `*_tests.rs` files
- `.claude/rules/rust-style.md` — guard clauses, no magic numbers, no `unwrap()`
- `.claude/rules/documentation.md` — changelog + docs updates are part of every task
- `.claude/rules/github-workflows.md` — Makefile-driven CI, `firestoned/github-actions` composites, SHA pins
- `.claude/rules/no-real-infrastructure.md` — never commit real hostnames/IPs/accounts

The governing methodology line is:

```
ADR → CALM → TDD → implement → docs
```

Architecture decisions are recorded in `docs/adr/` and modeled in
`docs/architecture/calm/architecture.json`; `make calm-validate` is a hard CI
gate. The Makefile is the single source of workflow truth — `make help` lists
every target.
