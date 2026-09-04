# Changelog

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
