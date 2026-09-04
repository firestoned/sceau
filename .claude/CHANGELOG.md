# Changelog

## [2026-09-04 18:23] - MkDocs documentation site + docs CI workflow

**Author:** Erick Bourgeois

### Changed
- `docs/mkdocs.yml`, `docs/pyproject.toml`, `docs/README.md`: MkDocs Material site config (theme/plugins/Mermaid setup adapted from banlieue), Poetry-managed docs dependencies
- `docs/src/`: full documentation — landing (`index.md`), `overview.md`, `concepts/` (`kms-v2.md`, `tpm-sealing.md`, `threat-model.md` + section index), `architecture/index.md`, `guides/` (`quickstart.md`, `k0s-setup.md`, `kairos-deployment.md`, `internal-registry.md` + section index), `developer/` (`index.md`, `local-development.md`), `reference/` (`cli.md`, `api.md`, `security.md`, `license.md`), plus `stylesheets/extra.css` and `javascripts/mermaid-init.js`
- `Makefile`: new `docs` / `docs-serve` / `docs-clean` / `docs-deploy` targets (Poetry-based, strict build); `CALM_DIAGRAMS_OUT` now `docs/src/architecture` so generated diagrams land in the site; `make docs-serve` added to the dev-loop header comment
- `.github/workflows/docs.yaml`: Documentation workflow — CALM validation gate, strict MkDocs build on PRs, deploy to GitHub Pages (actions-based) on push to main; SHA-pinned actions
- `.github/requirements/poetry.{in,txt}`: hash-locked Poetry install for CI (Scorecard Pinned-Dependencies), same pins as banlieue
- `README.md`: Documentation workflow badge + docs-site badge (https://firestoned.github.io/sceau/) and a pointer to the docs site
- `docs/architecture/calm/README.md`: diagram output paths updated to `docs/src/architecture/`
- `docs/architecture/{system,flows}.md`: removed — the generated diagrams now live at `docs/src/architecture/`
- `.gitignore`: ignore `docs/site/`, `docs/.venv/`, `docs/__pycache__/`

### Why
Give sceau the same published documentation surface as banlieue: a strict-built MkDocs Material site at https://firestoned.github.io/sceau/ with the CALM-generated architecture diagrams rendered straight into it, and a Makefile-driven CI workflow that builds on PRs and deploys on main.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only

## [2026-09-04 20:40] - README badges

**Author:** Erick Bourgeois

### Changed
- `README.md`: added badge rows mirroring banlieue — Build, SAST, CodeQL, OpenSSF Scorecard, license, Rust version, status, issues, last commit, PRs welcome — plus the SPDX comment header

### Why
Match the firestoned project presentation standard.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only

## [2026-09-04 20:15] - Makefile: PUSH flag, IMAGE tag alias, optional ORG in image ref

**Author:** Erick Bourgeois

### Changed
- `Makefile`: `docker-image` gains `PUSH=true` (buildx `--push` instead of `--load`); `IMAGE` is now an alias for `IMAGE_TAG`; `IMAGE_REF` omits the ORG path segment when `ORG=` is empty (banlieue pattern); `make help` documents the new variables

### Why
Support pushing the distroless image to an internal registry mirror with a single make invocation, e.g. `make docker-image ARCH=amd64 PUSH=true BASE_IMAGE=<mirror>/distroless/cc-debian13:nonroot REGISTRY=<registry>/<namespace> ORG= IMAGE=v0.1.0`.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [x] Config change only
- [ ] Documentation only

## [2026-09-04 16:30] - Initial scaffold: KMS v2 TPM plugin + ADD retroactive architecture

**Author:** Erick Bourgeois

### Changed
- `src/main.rs`, `src/kms.rs`, `src/tpm.rs`, `build.rs`, `proto/kms/v2/api.proto`: initial KMS v2 gRPC server sealing DEKs with a TPM 2.0 (fixed for tss-esapi 7.x API)
- `docs/adr/0001-tpm-sealed-kms-v2-plugin.md`, `docs/adr/0002-release-and-supply-chain-pipeline.md`: architecture decisions recorded retroactively (code predates the ADD record)
- `docs/architecture/calm/architecture.json`: FINOS CALM model of the apiserver → sceau → TPM path
- `Makefile`, `.github/workflows/`, `Dockerfile`: build/test/lint/audit/SBOM/CALM/docker pipeline
- `.claude/rules/`, `CLAUDE.md`, `AGENTS.md`: project rules mirrored from banlieue and adapted

### Why
New project: encryption at rest for k0s on Kairos without external key management.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [ ] Documentation only
- [x] Initial commit — no prior behavior
