# Local Development

Build sceau from source and iterate against a swtpm simulator — no hardware
TPM, no cluster. For installing a **release** on a real host, use the
[Guides](../guides/index.md) instead.

## Prerequisites

- Rust toolchain (**1.88+**).
- TPM2 TSS development libraries and `protoc`
  (`libtss2-dev protobuf-compiler` on Debian/Ubuntu).
- `docker` (for `build-linux-*` / `docker-image`).
- `swtpm` for local TPM simulation.
- Node.js (`npx`) for the CALM toolchain; [Poetry](https://python-poetry.org)
  for the docs.

```sh
git clone https://github.com/firestoned/sceau
cd sceau
```

## The Makefile is the source of truth

`make help` lists every target. The ones you will use daily:

| Target | What it does |
| --- | --- |
| `make build` / `make build-debug` | Release / debug binary via cargo. |
| `make test` | `cargo test --all-features`. |
| `make lint` | `cargo fmt --check` + `clippy -D warnings`. |
| `make audit` / `make deny` | `cargo audit` and `cargo deny check` (licenses, advisories, sources). |
| `make sbom` | CycloneDX SBOM (`sceau.cdx.json`). |
| `make calm-validate` | Validate the CALM architecture against the meta-schema (hard CI gate). |
| `make calm-diagrams` | Re-render the Mermaid diagrams into `docs/src/architecture/`. |
| `make build-linux-amd64` / `make build-linux-arm64` | Linux binary + staged TSS libraries under `binaries/<arch>/` (runs in a `rust:1-bookworm` container). |
| `make docker-image` | Distroless image from the prebuilt binary — see [Internal Registry](../guides/internal-registry.md). |
| `make docs` / `make docs-serve` | Build the docs site, or serve it with live reload at `http://127.0.0.1:8000`. |

## The swtpm dev loop

```sh
# Terminal 1 — the simulator
mkdir -p /tmp/swtpm-state
swtpm socket --tpm2 \
  --tpmstate dir=/tmp/swtpm-state \
  --server port=2321 \
  --ctrl type=tcp,port=2322 \
  --flags not-need-init

# Terminal 2 — sceau against it
cargo run -- \
  --socket /tmp/sceau.sock \
  --tcti "swtpm:host=127.0.0.1,port=2321"
```

Then drive the KMS API with `grpcurl` as shown in the
[Quickstart](../guides/quickstart.md#exercise-the-kms-api). The same loop
works against a *remote* simulator or real TPM by changing the TCTI string —
take the value from the environment rather than hard-coding a host:

```sh
SCEAU_TCTI="swtpm:host=bar.foo.io,port=2321" \
  cargo run -- --socket /tmp/sceau.sock --tcti "$SCEAU_TCTI"
```

!!! note "Logging"
    `RUST_LOG` controls verbosity (`tracing-subscriber` env filter):
    `RUST_LOG=debug cargo run -- ...`. The startup line always reports the
    derived `key_id`.

## Tests

- Tests live in separate `*_tests.rs` files next to the code (see
  `.claude/rules/testing.md`).
- `make test` runs the full suite; TPM-dependent tests use whatever TCTI the
  environment provides.
- End-to-end tests driving a real apiserver against a swtpm-backed sceau are
  on the roadmap.

## The ADD workflow for a change

1. **ADR** — write `docs/adr/NNNN-title.md` (Status / Context / Decision /
   Consequences). One decision per ADR.
2. **CALM** — update `docs/architecture/calm/architecture.json`, then:

    ```sh
    make calm-validate   # hard gate
    make calm-diagrams   # re-render docs/src/architecture/{system,flows}.md
    ```

3. **TDD** — failing test first, then the minimum implementation, then
   refactor. After any `.rs` change: `make lint && make test`.
4. **Docs** — changelog entry in `.claude/CHANGELOG.md` (the `**Author:**`
   line is mandatory), README if the CLI or deployment shape changed, and
   these pages when behaviour or flags change. Every YAML/flag example must
   match the real code (`src/main.rs`, `proto/kms/v2/api.proto`) — never
   guess.

## Building the docs

```sh
make docs         # regenerate CALM diagrams + strict mkdocs build into docs/site/
make docs-serve   # live-reload at http://127.0.0.1:8000
```

The site configuration is `docs/mkdocs.yml`; sources are `docs/src/`. The
`system.md` / `flows.md` architecture pages are generated — edit the CALM
model, not the rendered files.
