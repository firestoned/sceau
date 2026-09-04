# Developer

Working **on** sceau rather than deploying it: building from source, running
against a swtpm simulator, and following the project's governing methodology.

- **[Local Development](local-development.md)** — toolchain, `make` targets,
  the swtpm dev loop, and the documentation requirements that gate every
  change.

If instead you want to *run* sceau on a cluster, start with the
**[Guides](../guides/index.md)**.

## Methodology: ADD

sceau follows **ADD — Architecture Driven Development**. An architecturally
significant change starts with an
[ADR](https://github.com/firestoned/sceau/tree/main/docs/adr) and a
[CALM](https://calm.finos.org/) architecture update, *then* test-driven
implementation:

```text
ADR → CALM → TDD → implement → docs
```

Full ADR + CALM is required for things like new TPM object types or key
hierarchies, KMS contract changes (proto, envelope format, `key_id`
derivation), new deployment topologies, and anything where "why A over B" is
worth recording. Typos, isolated bug fixes, and mechanical refactors go
straight to TDD.

The rules live in the repository under `.claude/rules/` — read them before
writing code:

- `architecture-driven-development.md` — the ADD cycle
- `testing.md` — TDD is mandatory; tests in separate `*_tests.rs` files
- `rust-style.md` — guard clauses, no magic numbers, no `unwrap()`
- `documentation.md` — changelog + docs updates are part of every task
- `github-workflows.md` — Makefile-driven CI, pinned actions
- `no-real-infrastructure.md` — placeholder hostnames/IPs only

## Quality gate

After any Rust change (non-negotiable):

```sh
make lint    # cargo fmt --check + clippy -D warnings
make test    # cargo test --all-features
```

And before calling any task done: `make calm-validate` passes, the changelog
has an entry with `**Author:**`, and the docs still match the real CLI flags
and proto.
