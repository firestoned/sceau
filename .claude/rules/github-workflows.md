# GitHub Workflows & CI/CD Standards

## CRITICAL: Never Replace `firestoned/github-actions` With Direct Action Calls

ALL GitHub Actions workflows MUST use composite actions from the `firestoned/github-actions` library. NEVER replace them with direct action calls, even if the underlying action version is outdated.

**Why:** `firestoned/github-actions` is owned by the user (Erick Bourgeois). When an underlying action needs a version bump, fix it in the `firestoned/github-actions` repo — NOT by inlining here.

**Fix process:**
1. Update action version in the `firestoned/github-actions` repository
2. Tag a new release (e.g., v1.3.7)
3. Update the version reference in this repo's workflows

```yaml
# ✅ CORRECT
- name: Cache cargo dependencies
  uses: firestoned/github-actions/rust/cache-cargo@53b483254bc648903c364ee3c73a546d0936a91e # v1.3.6

# ❌ WRONG
- name: Cache cargo dependencies
  uses: actions/cache@v5
```

**Action families:**
- `firestoned/github-actions/rust/cache-cargo` — Cargo dependency caching
- `firestoned/github-actions/rust/setup-rust-build` — Linux cross-compilation setup
- `firestoned/github-actions/rust/build-binary` — Binary compilation
- `firestoned/github-actions/rust/generate-sbom` — SBOM generation
- `firestoned/github-actions/rust/security-scan` — Cargo audit
- `firestoned/github-actions/docker/setup-docker` — Docker login + buildx
- `firestoned/github-actions/security/license-check` — SPDX header verification
- `firestoned/github-actions/security/verify-signed-commits` — Commit signature verification
- `firestoned/github-actions/security/trivy-scan` — Container vulnerability scan
- `firestoned/github-actions/security/cosign-sign` — Keyless Cosign signing
- `firestoned/github-actions/versioning/extract-version` — Image tag generation

---

## CRITICAL: All Workflows Must Be Makefile-Driven

Workflows MUST only: install tools, set env vars, and call Makefile targets. All business logic lives in the Makefile.

Installing system packages the runner lacks (e.g. `apt-get install libtss2-dev protobuf-compiler`) is tool setup and belongs in the workflow; everything after that is a `make` call.

```yaml
# ✅ GOOD
- name: Install system dependencies
  run: sudo apt-get update && sudo apt-get install -y libtss2-dev protobuf-compiler

- name: Run tests
  run: make test

# ❌ BAD
- name: Build and test
  run: |
    cargo build --release
    cargo test --all-features -- --nocapture
    strip target/release/sceau
    # ... 30 more lines of bash ...
```

**Rules:**
- No multi-line bash scripts (except simple tool setup)
- All `run:` commands MUST call Makefile targets (e.g., `make audit` not `cargo audit`)
- Makefile targets MUST work identically locally and in CI
- Document targets with `## comments` for `make help`

---

## CRITICAL: Third-Party Actions Are SHA-Pinned

Every third-party action (`actions/checkout`, `dtolnay/rust-toolchain`, …) is
pinned by full commit SHA with a version comment. Never a floating tag.

```yaml
# ✅ CORRECT
- uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2

# ❌ WRONG
- uses: actions/checkout@v6
```

Top-level workflow permissions stay read-only (`permissions: contents: read`);
jobs that need more (GHCR push, SARIF upload, OIDC for Cosign) declare it at
the job level.

---

## Checklist before adding a new workflow

- [ ] Can this be a job in an existing workflow?
- [ ] Does it use `firestoned/github-actions` composites where one exists?
- [ ] Is every third-party action SHA-pinned with a version comment?
- [ ] Does every `run:` step call a Makefile target (after tool setup)?
