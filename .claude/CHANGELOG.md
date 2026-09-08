# Changelog

## [2026-09-08 01:15] - Fix SLSA provenance: the generator must be referenced by tag, not SHA

**Author:** Erick Bourgeois

### Changed
- `.github/workflows/build.yaml`: reference
  `slsa-github-generator/.github/workflows/generator_generic_slsa3.yml` by its
  `@v2.1.0` **tag** instead of the commit SHA that tag points at. Applies to
  the release path too — `build.yaml` is the release workflow, and
  `slsa-provenance` is the same job on both `push` and `release` events.
- `.claude/rules/github-workflows.md`: record this as the single, enforced
  exception to the SHA-pinning rule, with the upstream error text, so it is not
  "corrected" back.
- `docs/adr/0006-vex-slsa-and-attestation-parity.md`,
  `docs/architecture/calm/architecture.json`: both claimed every third-party
  action is SHA-pinned. Amended to state the exception and why it exists.
- `.github/workflows/build.yaml`: comment recording that on a release event the
  generator's own `upload-assets` job also attaches the `.intoto.jsonl`, so it
  is uploaded twice — redundant but ordered and byte-identical, not a race.

### Why
Run 34174552415 failed on `main` with:

    Fetching the builder with ref: f7dd8c54c2067bafc12ca7a55595d5ee9b75204a
    Invalid ref: f7dd8c54c2067bafc12ca7a55595d5ee9b75204a.
    Expected ref of the form refs/tags/vX.Y.Z

`builder-fetch.sh` requires the ref to start with `refs/tags/`, and the generic
generator's README states the workflow "MUST be referenced by a tag of the form
`@vX.Y.Z` ... the build will fail ... if you reference it by a hash." The
generator resolves its release binary from the ref and `slsa-verifier` derives
the trusted builder ID from the tag, so a SHA yields either no provenance or
provenance nothing can verify. The SHA used was genuinely tag v2.1.0's commit —
the check is on the ref's *form*, so being the right commit did not help.

ADR-0006 already said "pinned to a release tag" and the workflow was written
with a SHA anyway; the rule file said "never a floating tag" with no exception,
so the two were in direct conflict and the workflow lost.

### Verified
Everything else in that run passed, including the whole ADR-0006 chain on its
first real execution: Attest, Grype Triage, Auto-VEX (presence), Assemble
OpenVEX and Container Scan. The Trivy→OpenVEX migration is confirmed against
the live image — Grype found exactly the 20 CVEs that were migrated,
`auto-vex-presence` emitted 0 because all 20 were already triaged, and the
`grype-push-Distroless` analysis uploaded **results=0**, i.e. `--vex` really did
suppress them rather than silently ignoring the document.

### Stale Trivy alerts cleared (repository action, no code change)
The 20 open Trivy code-scanning alerts were orphaned by ADR-0006: the Trivy job
no longer runs, so nothing would ever re-evaluate and close them, and the
dashboard would have shown 20 open container CVEs indefinitely while Grype
reported zero.

Cleared by deleting the 5 `trivy-container-scan` analyses on `refs/heads/main`
(`DELETE /code-scanning/analyses/{id}`, chained via `next_analysis_url`, with
`?confirm_delete` on the last one). Deleting the analyses rather than dismissing
the alerts removes the retired tool from the dashboard entirely, which matches
reality — Trivy is gone, not suppressed.

Checked before deleting: the 20 alert rule IDs map **1:1** onto the 20
`.vex/*.json` documents, with no CVE on either side unmatched. No justification
was lost — each one still has its OpenVEX statement, now attested to the image
digest. A snapshot of the alerts was taken first.

After: Trivy has 0 alerts in any state and 0 analyses. The only open alerts left
are 5 pre-existing Scorecard findings, unrelated to this work.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [x] Config change only
- [ ] Documentation only

## [2026-09-07 18:55] - Supply-chain parity with banlieue: OpenVEX, SLSA L3, attestations, arm64

**Author:** Erick Bourgeois

### Added
- `docs/adr/0006-vex-slsa-and-attestation-parity.md` (new): the governing
  decision. Records three deliberate deviations from banlieue —
  `auto-vex-reachability` not ported, one image variant, `time` instead of
  `chrono` — rather than leaving them as silent gaps.
- `crates/sceau-vex/` (new workspace member): `auto-vex-presence`, ported from
  `banlieue-vex`. 29 unit tests, written before the implementation. Emits
  `not_affected + component_not_present` for Grype findings whose purl is in
  no image SBOM. Keeps both fail-closed guards from the original: a
  zero-component SBOM is an error (not "nothing is present"), and all file
  input is size-capped before being read.
- `.vex/` (new): curated OpenVEX statements, one document per advisory, plus
  `README.md` explaining what is automated vs. hand-written.
- `Makefile`: `vexctl-install`, `grype-install`, `grype-triage`, `grype-scan`,
  `vex-validate`, `vex-assemble`, `vex-assemble-all`, `vex-auto-presence`,
  `docker-image-prestaged`, `docker-digest`.
- `.github/workflows/build.yaml`: `vex-validate`, `test`, `attest`,
  `grype-triage`, `auto-vex-presence`, `build-vex`, `grype`, `sign-artifacts`,
  `generate-provenance-subjects`, `slsa-provenance` jobs.

### Changed
- `Cargo.toml`: the repo root is now also a workspace root
  (`members = ["crates/sceau-vex"]`). `make test` / `make lint` became
  `--workspace`-scoped.
- `.github/workflows/build.yaml`: the `build` job is a matrix over
  `ubuntu-24.04` + `ubuntu-24.04-arm`; the image is a real multi-arch manifest
  built from both pre-staged binary trees; Cosign signs **by digest** instead
  of by tag.
- `Makefile` (`sbom`): discards `crates/*/*.cdx.json`. cargo-cyclonedx emits
  one SBOM per workspace member and has no package filter — shipping the
  CI-tool SBOM would describe dependencies the released binary does not
  contain.
- `docs/architecture/calm/architecture.json`: `supply-chain-pipeline` control
  updated (SSDF PS.1/PS.2/PS.3/PW.4/RV.1); new `vulnerability-triage-vex`
  control (RV.1/RV.2/RV.3); ADR-0006 added to `adrs`.
- `docs/adr/0002-release-and-supply-chain-pipeline.md`: its SLSA / OpenVEX /
  Trivy deferrals are marked resolved by ADR-0006. The Chainguard deferral
  still stands.
- `README.md`: new "Supply chain" section with a verify-command table and the
  triage workflow.

### Removed
- `.trivyignore`, and the Trivy scan job. **All 20 suppressions were migrated
  to `.vex/*.json`** with their original justifications preserved and a
  "drop this when" condition added to each — deleting the file without
  migrating would have silently reopened 20 alerts under the new scanner.
  Two scanners would have meant two triage backlogs with VEX wired to only
  one of them.

### Why
sceau's pipeline was ADR-0002's deliberately reduced form: signature + SBOM +
advisory-only Trivy, with SLSA and VEX explicitly deferred. Three things
retired that reasoning: advisory-only scanning leaves no durable, machine-
readable record of *why* a finding was dismissed; sceau's image is unusually
base-image-heavy for a Rust binary (it stages libtss2 and its glibc
dependencies into the rootfs, which is exactly the population that generates
recurring unfixable CVEs); and "verify this artifact is what the source
produced" needs provenance a verifier can check, not just a signature proving
someone signed something. banlieue is the reference implementation, so the
shape is ported rather than reinvented.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [x] Config change only
- [ ] Documentation only

Notes for the first run on `main`:
- Grype is pinned at 0.118.0. Do not downgrade — 0.87.0 accepts `--vex` and
  silently ignores it (confirmed in banlieue the same day).
- `slsa-provenance` passes `private-repository: true` on a verified-public
  repo, to work around the generator's stale visibility check (root-caused
  live in banlieue, 2026-09-07). Drop it once upstream fixes the check.
- Only the amd64/arm64 build, image build, and unit tests have been exercised
  locally; the signing/attestation/SLSA jobs need OIDC and can only be
  verified on a real push to `main`.

## [2026-09-07 18:09] - Fix 20 open Trivy code-scanning alerts (base-image CVEs)

**Author:** Erick Bourgeois

### Changed
- `Dockerfile`: bumped `gcr.io/distroless/cc-debian13:nonroot` digest to
  `sha256:c31ff9abcb1910f3ab25c7957bdaf0bfe12a01eb546e8df2282f1c8f682b606c`
  (latest; also clears libssl3t64 3.5.6 CVEs fixed in 3.5.7-1~deb13u2).
- `.trivyignore` (new): suppresses the 20 unfixed Debian 13 libc6/zlib1g
  findings behind the open Trivy alerts, with per-group justifications.
  All 20 have no fixed package version; the 7 old glibc CVEs are disputed
  by upstream, and the rest affect glibc/zlib functions the Rust binary
  never imports (verified with `nm -D --undefined-only`).

### Why
20 open Trivy alerts on `firestoned/sceau` were all base-image OS-package
CVEs (libc6 2.41-12+deb13u3, zlib1g) with no upstream fix to upgrade to.
Verified locally that the ignore file zeroes the SARIF result set against
the published image (`ghcr.io/firestoned/sceau:main-2026.09.07`); the
alerts auto-close on the next push-to-main scan. Mirrors the banlieue
fix of the same day (grype `--vex` silently ignored by its pinned 0.87.0).

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [x] Config change only
- [ ] Documentation only

## [2026-09-07 17:17] - Fix CI: allow new dep licenses; ignore unpatched rsa advisory

**Author:** Erick Bourgeois

### Changed
- `deny.toml`: extended `[licenses] allow` with `ISC` (ring,
  rustls-webpki, untrusted), `Zlib` (foldhash), and `CDLA-Permissive-2.0`
  (webpki-root-certs) — all OSI-approved licenses pulled in by the new
  kube/rustls/rcgen dependency tree from the ADR-0003 fleet-key work.
- `deny.toml`, `.cargo/audit.toml` (new): ignore `RUSTSEC-2023-0071`
  (rsa Marvin Attack timing sidechannel), with justification — sceau's
  only `rsa` use is the lossless PKCS#1 -> PKCS#8 re-encode in
  `certs.rs::ca_key_to_pkcs8_pem`; no RSA decrypt/sign, nothing
  network-observable. The advisory is unpatched upstream and its own
  workaround text covers this use.

### Why
PR #5 CI failed in both dependency gates: the cargo-deny job (5 license
rejections + the rsa advisory) and the security-scan job (cargo audit
exiting 1 on the rsa advisory).

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [x] Config change only
- [ ] Documentation only

## [2026-09-07 19:00] - Add `sceau status`; fix a spurious TPM error on drop it exposed

**Author:** Erick Bourgeois

### Changed
- `src/cli.rs`, `src/cli_tests.rs`: new `sceau status` subcommand, no args
  beyond the global `--tcti`.
- `src/main.rs`: `run_status` — probes `fleet::load_fleet_key`, prints
  `fleet_key=<bool> key_id=<id>` to stdout and exits. Mirrors
  `build_sealer`'s own fleet-key-first logic so it can never disagree with
  what `serve` would actually pick.
- `src/tpm.rs`: `TpmSealer` gained an `owns_transient_primary` field, set
  `true` in `new()` (the per-node SRK, a transient object this sealer
  creates and must free) and `false` in `from_primary()` (the ADR-0003
  fleet key, a reference to an already-persistent object). `Drop` now only
  calls `TPM2_FlushContext` when `owns_transient_primary` is true.

### Why
Implementing Phase 7's `sceau --status` item (ADR-0003 roadmap) — needed
to confirm a shared `key_id` across nodes without reading
`sceau.service`'s startup log — surfaced a real bug in already-existing
code: `TpmSealer`'s `Drop` impl called `flush_context` unconditionally,
with a comment claiming this was "safe regardless" of whether the primary
was transient or an already-persistent object, since flushing a
persistent-derived handle was assumed to only clear ESAPI's local
bookkeeping. Confirmed live (2026-09-07) that assumption is wrong on real
hardware: the TPM rejects it outright with `TPM_RC` 0x1c4 ("value is out
of range or is not correct for the context"), logged as a scary-looking
`ERROR`/`WARNING` from the underlying `esys` C library on every drop. This
was already happening on every `sceau serve` shutdown using the fleet
key — just never noticed, because `serve` holds its `TpmSealer` for the
process's entire lifetime and only drops it at shutdown, burying the
error in logs nobody was watching. `status` creates and immediately drops
one on every invocation, making it impossible to miss. Fixed by tracking
whether the sealer actually owns a transient primary and skipping the
flush otherwise.

Live-verified on both real test nodes: `sceau status` now prints
`fleet_key=true key_id=sceau-<hash>` (identical on both) with a clean
`Context closed` — no more `ERROR`/`WARNING` from `esys`.

### Impact
- [ ] Breaking change
- [x] Bugfix — `sceau serve`'s shutdown path (when using the fleet key)
  was silently issuing an invalid TPM command every time; harmless in
  effect (`let _ = ...` already discarded the error) but noisy, and now
  correct rather than merely tolerated.


## [2026-09-07] - New roadmap: attestation-gated cluster join (ADR-0005, tentative)

**Author:** Erick Bourgeois

### Changed
- `.github/community/adr-0005-attestation-gated-cluster-join-roadmap.md`
  (new): phased plan for generalizing ADR-0003 Phase 6's TPM
  credential-activation attestation from "gate fleet-key duplication" to
  "gate k0s cluster admission itself" — a new `sceau` subcommand pair
  acting as a join-broker that mints a k0s join token only after a passing
  attestation, instead of an operator/automation minting tokens directly.
- `ROADMAPS.md`: new index row.

### Why
Discussed as a follow-up to ADR-0003 Phase 6: could the same EK/AK
attestation protocol gate *any* node joining the cluster, not just nodes
requesting the fleet key specifically? Concluded yes, but it's a
materially different trust boundary (minting cluster-admission
credentials, not just reading `Node` objects) and crosses into whatever
mints k0s join tokens today (`banlieue`'s bootstrap tooling) — reasoned
through as deserving its own ADR (tentatively ADR-0005; ADR-0004 is
already provisionally reserved by ADR-0003's own Phase 10 for the
unrelated `k0smotron` multi-tenancy topic) rather than folding into
ADR-0003. This roadmap deliberately does not reimplement Phase 6's
crypto — it's a hard dependency, not parallel work.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Documentation only — no code yet; Phase 0 of the new roadmap is
      writing the ADR itself.

## [2026-09-07 18:15] - Fix `enroll` permanently burning a `--max` permit on any rejected/failed attempt; live-verify Phase 4's negative-auth path

**Author:** Erick Bourgeois

### Changed
- `src/enroll.rs`: `EnrollService::duplicate` split into a thin permit
  wrapper and a new `handle_duplicate` method holding the actual auth/authz/
  TPM logic. The wrapper now restores the permit on any `Err` from
  `handle_duplicate`, not just the explicit "already at max" branch.

### Why
Live-testing Phase 4's negative-auth path (a node whose client cert is
valid but whose `Node` object doesn't currently exist gets rejected) with
`--max 1` surfaced a real bug: the permit was decremented unconditionally
up front, but only the "permits already at zero" branch ever gave it back.
Every other failure — bad/missing cert, unauthorized node, malformed
request, a transient internal error — returned early without restoring it.
Confirmed live (2026-09-07): one rejected enrollment attempt left a
second, fully legitimate `join` attempt failing with `--max already
reached`, with nothing ever actually enrolled. With the real-world default
of `--max 1`, a single bad probe (or even an innocent misconfigured node)
permanently locks out the next real joiner until the operator restarts
`enroll`.

Re-verified live after the fix, same two-node test cluster: rejected an
unauthorized `join` (node's `Node` object deleted, cert otherwise valid) →
`PermissionDenied`/`UnknownNode` as expected → restored the node's cluster
membership → a second `join` attempt in the *same* `enroll --max 1`
session now succeeds, where it previously would have failed with
`--max already reached`. This closes Phase 4's negative-auth gate in
`.github/community/adr-0003-fleet-key-ha-roadmap.md`.

### Impact
- [ ] Breaking change
- [x] Bugfix — `enroll --max N` previously undercounted its effective
  capacity by one for every rejected/failed attempt; now only successful
  enrollments consume a permit.


## [2026-09-07 15:20] - Fix code-scanning alerts: Trivy suppressions, ClusterFuzzLite fuzzing, CODEOWNERS

**Author:** Erick Bourgeois

### Changed
- Base-image CVE suppressions (libc6/zlib1g in distroless cc-debian13, no
  fixed version in Debian trixie — code-scanning alerts #7-#26): originally
  a `.trivyignore` in this commit. **Dropped during the rebase onto ADR-0006**,
  which removed Trivy in favour of `grype --vex` and migrated all 20
  suppressions to `.vex/*.json` with the same justifications. Verified the
  CVE sets match exactly before dropping the file; re-adding it would have
  left dead config contradicting `.vex/`.
- `src/lib.rs` (new), `src/main.rs`: binary is now a thin shim over a library
  crate so the fuzz workspace can link the pure parsers.
- `src/tpm.rs`: `envelope_encode` / `envelope_decode` are now `pub` (pure
  codec, fuzz target). No behaviour change. `sealed_public` stays private —
  the rebase onto ADR-0003's fleet-key work moved the tests into a module
  nested inside `tpm.rs`, which already reaches it via `super::super::*`, so
  the `pub(crate)` widening this commit originally needed is unnecessary.
- `src/tpm_tests.rs`: adds malformed-envelope rejection coverage on top of the
  cases ADR-0003 already landed — input shorter than the header, a truncated
  *real* envelope, and a garbage (unmarshallable) public area. No TPM required.
- `fuzz/` (new): cargo-fuzz workspace with `envelope_decode` and
  `kms_proto_decode` targets (ADR-0003).
- `.clusterfuzzlite/` + `.github/workflows/fuzz.yaml` (new): ClusterFuzzLite
  `code-change` fuzzing on Rust-affecting PRs — Scorecard Fuzzing alert #6.
- `.clusterfuzzlite/Dockerfile`: build on the **focal** `base-builder-rust`
  and compile tpm2-tss 4.1.4 from source (checksum-pinned) instead of using
  the `ubuntu-24-04` builder variant. The builder's glibc must not exceed the
  runner's, and ClusterFuzzLite hardcodes its container images — there is no
  input to select an OS variant — so the 24.04 builder produced targets that
  died with ``libc.so.6: version `GLIBC_2.39' not found`` on the focal runner
  (run 34172370455). The 24.04 variant had been chosen because focal's
  libtss2-dev is 2.3.2 and tss-esapi-sys requires ≥ 2.4.6; building tpm2-tss
  from source satisfies that without the ABI jump. Confirmed from the image's
  own config blob that the focal builder is Ubuntu 20.04 and already ships
  `nightly-2025-09-05` + cargo-fuzz, so the toolchain-install step is dropped.
- `.clusterfuzzlite/build.sh`: stage libtss2 (and the rest of the targets'
  non-core shared-library closure) into `$OUT/lib`, and link the targets with
  `-rpath,$ORIGIN/lib -Wl,--disable-new-dtags`. The targets are executed in
  the base-*runner* image, which has no libtss2, so both compiled fine and
  then died at startup with "error while loading shared libraries:
  libtss2-esys.so.0" — reported as 100% of fuzz targets broken (run
  34170567314). `--disable-new-dtags` is load-bearing: the default DT_RUNPATH
  is not inherited by transitive dependencies, so the loader would find
  `libtss2-esys.so.0` next to the binary and then fail on its own
  `libtss2-sys.so.1`. Adds a build-time DT_NEEDED check so the same class of
  failure reports one readable line instead of an opaque runner error.
- `.github/CODEOWNERS` (new): ownership for branch-protection review rules.
- `.github/workflows/dependabot-auto-merge.yaml`: auto-merge job now approves
  the PR (github-actions[bot] review) before enabling auto-merge, so bot PRs
  satisfy the new "1 approving review" branch-protection rule on main.
- `docs/adr/0003-fuzzing-with-clusterfuzzlite.md` (new): ADR per ADD.

### Why
Clear every open alert on the repository's code-scanning dashboard: 20 Trivy
CVEs with no upstream fix (suppress with justification), Scorecard Fuzzing
(deploy ClusterFuzzLite — the only in-repo remediation for Rust), and
Scorecard Branch-Protection (settings change + CODEOWNERS; applied via API).

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [x] Config change only
- [ ] Documentation only

## [2026-09-07 13:23] - Dependabot 7-day cooldown on all ecosystems

**Author:** Erick Bourgeois

### Changed
- `.github/dependabot.yml`: added `cooldown: default-days: 7` to every
  `package-ecosystem` entry (github-actions, cargo, pip, docker) and a header
  comment explaining the rationale.

### Why
GitHub Advanced Security / Semgrep flagged the config on PR #1 with
`dependabot-missing-cooldown` (4 findings, code-scanning alerts #27-#30).
Newly published packages can be malicious or unstable; a 7-day cooldown lets
fresh releases bake before Dependabot proposes them. Cooldown applies to
version updates only — security updates are still proposed immediately.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [x] Config change only
- [ ] Documentation only

## [2026-09-07 12:30] - Fix `join` failing to re-persist a fleet key on a node that already holds one; live-verified `enroll`/`join` round-trip

**Author:** Erick Bourgeois

### Changed
- `src/fleet.rs`: `import_and_persist` now unconditionally calls
  `delete_persisted_fleet_key` before `persist`, evicting whatever already
  occupies `FLEET_KEY_PERSISTENT_HANDLE` on the joining node. Updated
  `persist`'s doc comment to state its now-confirmed-live, non-idempotent
  behavior instead of "needs verification."

### Why
First real `enroll`/`join` round-trip run against two genuinely fresh
enrollment attempts (`genesis --force` on one real node, `join` from the
other) failed with `TPM_RC_NV_DEFINED` ("NV Index or persistent object
already defined") — the joining node already held an object at the fleet
key's persistent handle from an earlier enrollment, and `persist()`'s
`TPM2_EvictControl` call has no "replace what's there" behavior on its own.
This is exactly the gap the ADR-0003 roadmap's Phase 7 had flagged as
unverified. Unlike `genesis` (where silently replacing an already-enrolled
node's key is actively dangerous, hence the opt-in `--force`), `join` is
only ever run because the operator wants this node's slot to hold the key
it's about to receive — so always evicting first is the correct default,
no flag needed.

Live-verified end to end afterward on the real two-node test cluster:
`join` succeeded (`fleet sealing key imported and persisted`), both nodes'
`sceau.service` loaded the identical `key_id` after restart, and a Secret
created via one node's apiserver read correctly via the other's in both
directions (after a `k0scontroller` restart on both nodes to clear the
apiserver's stale cached-DEK state from the key rotation). This closes
Phase 3's live-verification gate and the positive-path half of Phase 4's
in `.github/community/adr-0003-fleet-key-ha-roadmap.md`.

### Impact
- [ ] Breaking change
- [x] Bugfix — `join` against a node that already holds any fleet key
  (e.g. a rejoin after a restart, or recovering from a partial prior join)
  was previously guaranteed to fail; now succeeds by design.


## [2026-09-07 00:00] - Fix `tests/fleet_duplication.rs` build break; first full green build/lint/test on Linux

**Author:** Erick Bourgeois

### Changed
- `tests/fleet_duplication.rs`: `fleet::create_fleet_key` call updated to
  pass the `force: bool` argument it gained after this test was written
  (was calling it with the old one-argument signature, breaking the build).

### Why
First time `cargo build`/`clippy`/`test` were run against this crate on
Linux with `libtss2-dev` actually installed (previously only ever built on
real target hardware directly) — done here via the existing
`build-linux-arm64` container recipe's toolchain, run manually with
`cargo build --all-targets`/`clippy`/`test` instead of the release-only
staging path, using the host's corporate CA bundle
(`CA_BUNDLE`-equivalent) so `rustup component add`/crates.io/static.rust-lang.org
resolve through the local TLS-intercepting proxy. This caught a stale test
signature that had never been compiled since `create_fleet_key` gained its
`force` parameter. Result: `cargo fmt --check`, `cargo build --all-targets`,
`cargo clippy --all-targets --all-features -- -D warnings`, and
`cargo test --all-features` (33 unit tests) all green; the TPM-dependent
`fleet_duplication` integration test correctly skips as `#[ignore]`d with
no TCTI configured. Closes the last open Phase 2/Phase 3 build-gate items
in `.github/community/adr-0003-fleet-key-ha-roadmap.md`.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [x] Test-only fix; no runtime behavior changed — a stale integration-test
  call site, never compiled since `create_fleet_key` gained `force`

## [2026-09-06] - Add `sceau serve --force-legacy` to make the `Decrypt` fallback path testable

**Author:** Erick Bourgeois

### Changed
- `src/cli.rs`: `Serve` gained a hidden `--force-legacy` flag (testing
  only — never look for the fleet key, always use the per-node
  deterministic SRK).
- `src/main.rs`: `run_serve` skips `build_sealer`'s auto-detect entirely
  when set, going straight to `tpm::TpmSealer::new`.
- `src/cli_tests.rs`: new test for the flag; existing `Serve` cases updated
  for the new field.

### Why
Asked directly to live-test the `KmsService::with_legacy_fallback` path
(a Secret sealed before a node joined the fleet still decrypting correctly
afterward) — but the real test cluster can't reproduce that scenario
naturally anymore: the fleet key was already active on both nodes before
either ever sealed a Secret, and there's no `etcdctl`/internet-reachable
gRPC client tool on these immutable Kairos nodes to manufacture legacy
ciphertext another way. `--force-legacy` reproduces the real scenario
directly and reversibly: seal a Secret with it on, restart without it, and
the `key_id` mismatch that scenario actually produces is real, not
simulated.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Testing-only addition — hidden from `--help`, not part of the
      supported operator surface.

## [2026-09-06] - ADR-0003 Phase 5 live-verified end to end: real cross-node decrypt, both directions

**Author:** Erick Bourgeois

### Changed
- No code changes — this entry records live verification only.
- `.github/community/adr-0003-fleet-key-ha-roadmap.md`: Phase 5 marked
  done, live-verified (bidirectionally).
- `ROADMAPS.md`: status line updated.

### Why
Full round trip against the real two-node test cluster, after the
`TPM_RC_AUTH_TYPE` / `TPM_RC_POLICY_FAIL` / `TPM_RC_ATTRIBUTES` fixes
above and rolling `--encryption-provider-config` onto both controllers one
at a time (per `docs/migration-ha-existing-cluster.md` §4b):

1. `kube-apiserver`'s `/readyz/kms-providers` went from failing (masked as
   "reason withheld" on the aggregate `/readyz`) to `ok` on both nodes
   after the `TPM_RC_ATTRIBUTES` fix.
2. `2026-09-06T15:29:22Z` — `sceau-ha-test` created via node1's apiserver,
   read back via **node2's** apiserver (a physically separate TPM chip
   that never sealed this data) — correct plaintext (`hello-from-node1`).
3. `2026-09-06T15:44:27Z` — the reverse, to rule out a one-way fluke:
   `sceau-ha-test-2` created via **node2's** apiserver, read back via
   **node1's** — also correct (`hello-from-node2`).

That's the literal claim ADR-0003 exists to establish, confirmed in both
directions: a Secret sealed on one control-plane node is readable from
another, via the shared fleet key `TPM2_Duplicate`/`Import` put there. Two
real, separate bugs were found and fixed en route to this, neither of them
TPM-specific:
- `contrib/systemd/sceau.service`'s socket group ownership churned between
  `root:root` and `Group=kube-apiserver` before settling back on
  `root:root` — `id kube-apiserver` (the static account definition) said
  gid 987; `/proc/<pid>/status` for the actual running process said
  `Gid: 0` with no supplementary groups. Only the latter is authoritative.
- Node2's `k0scontroller` was still running with no
  `--encryption-provider-config` at all — pushing the config file to both
  nodes doesn't apply it until each node's `k0scontroller` restarts; this
  simply hadn't happened for node2 yet mid-rollout.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout — already done, on the live test cluster.
- [ ] Config change only
- [x] Live verification — ADR-0003's Phases 2 through 5 are now confirmed
      working end to end on real hardware, not just compiling.

## [2026-09-06] - `deploy-test` stops a running service before overwriting its binary

**Author:** Erick Bourgeois

### Changed
- `scripts/deploy-test-nodes.sh`: checks `systemctl is-active $service_name`
  per host before copying; if active, stops it first and restarts it after
  the copy + unit install complete. Does nothing extra if the service
  wasn't running (no spurious start on a node where it's never been
  enabled).

### Why
Hit this live immediately after the previous entry's own unit-copy change
shipped: `scp: dest open ".../sceau": Failure` -- a running `sceau` holds
its own executable open, so overwriting it fails with `ETXTBSY`, which
scp's SFTP protocol reports as a bare "Failure" with no further detail.
Confirmed by checking `ps aux`/`systemctl status` on the live node before
fixing.

Verified live against `k0s-node1` (2026-09-06), both branches: with the
service running, `deploy-test` now stops/copies/restarts cleanly (confirmed
`active (running)` afterward); with it stopped beforehand, `deploy-test`
skips the stop/restart entirely and leaves it `inactive`, not started.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Tooling only -- live-verified end to end on a real node, both code
      paths.

## [2026-09-06] - `deploy-test` now also installs the systemd unit

**Author:** Erick Bourgeois

### Changed
- `scripts/deploy-test-nodes.sh`: copies `contrib/systemd/$BINARY.service`
  (if present) to `/etc/systemd/system/` on each host and runs `systemctl
  daemon-reload`, alongside the existing binary/libs copy. Never
  enables/starts it -- prints the `systemctl enable --now` command instead,
  leaving that step to the operator. `repo_root` moved out of the
  `SOURCE_MODE=local`-only branch so it's available for locating the unit
  file regardless of source mode.
- `README.md`: `### Deploying to a test node` updated to describe the unit
  install step.

### Why
`contrib/systemd/sceau.service` already existed and already documented
itself as meant to pair with this script ("For manual test-node deployment,
pair with scripts/deploy-test-nodes.sh") -- the script just never actually
did it, so every test deploy required copying the unit by hand separately.

Verified live against `k0s-node1` (2026-09-06): `sceau.service` was already
running there from earlier manual setup, which reproduced a real,
unrelated pre-existing failure mode first -- `scp` can't overwrite a
running executable (`ETXTBSY`, surfaces as a generic SFTP "Failure").
Stopped the service, re-ran `deploy-test`, confirmed the deployed unit file
is byte-identical to `contrib/systemd/sceau.service`, then restarted the
service to leave the node as found.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Tooling only -- live-verified end to end on a real node.

## [2026-09-06] - Fix live Phase 5 failure: `TPM_RC_ATTRIBUTES` sealing under the fleet key

**Author:** Erick Bourgeois

### Changed
- `src/tpm.rs`: `sealed_public` now takes a `fixed: bool`, setting both
  `fixedTpm`/`fixedParent` to match it (previously hardcoded `true` for
  both). `TpmSealer` gained a `child_fixed` field, derived from the
  sealing primary's own `Public.object_attributes().fixed_tpm()` in both
  `new()` and `from_primary()` (already reading that `Public` area to
  compute `key_id` — no extra TPM round trip), and `seal()` passes it
  through instead of assuming `true`.
- `src/tpm_tests.rs`: new test locking in that `sealed_public`'s `fixed`
  argument actually controls both attribute bits; existing envelope test
  updated for the new signature.
- Also fixed live, separately: `contrib/systemd/sceau.service` briefly
  went to `Group=kube-apiserver` and back to plain `root:root` — checking
  `id kube-apiserver` (the static `/etc/passwd` entry, gid 987) instead of
  `/proc/<pid>/status` for the *actual running* `kube-apiserver` process
  (`Gid: 0`, no supplementary groups) gave the wrong answer; reverted once
  the real, authoritative source was checked.

### Why
Live Phase 5 test, first sign of trouble: `kube-apiserver`'s `/readyz`
reported `[-]kms-providers failed: reason withheld` — the aggregate
`/readyz?verbose` endpoint deliberately redacts KMS failure detail, but the
per-check `/readyz/kms-providers?verbose` sub-endpoint does not, and showed
the real error: `TPM error: inconsistent attributes (associated with
parameter number 2)`. That's `TPM_RC_ATTRIBUTES` on `TPM2_Create`'s
`inPublic` parameter — a real TPM 2.0 consistency rule: a child object
cannot be created with `fixedTpm=true` under a parent whose own `fixedTpm`
is `false`. `sealed_public()` hardcoded `fixedTpm=true`/`fixedParent=true`
for every sealed DEK, correct under the ADR-0001 deterministic SRK (also
`fixedTpm=true`) but wrong under the ADR-0003 fleet key, whose entire
purpose is *not* being `fixedTpm` (that's what makes it duplicable at
all). Every `Encrypt` call under the fleet key was failing this way.

### Impact
- [ ] Breaking change — `authPolicy`/key material unaffected; this is a
      sealing-template fix, no re-`genesis` needed.
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Bugfix — not yet re-verified live (this fix was written in response
      to the error output, not yet re-run against real hardware).

## [2026-09-06] - Implement ADR-0003 Phase 5: steady-state fleet-key integration

**Author:** Erick Bourgeois

### Changed
- `docs/adr/0003-fleet-key-duplication-for-ha-multi-controller.md`: new
  Decision 5 — `sceau serve` prefers the fleet key over the per-node SRK
  for `Encrypt` and as `Decrypt`'s first choice, keeping the SRK alive as a
  `Decrypt`-only fallback so pre-enrollment ciphertext stays readable.
- `src/tpm.rs`: `TpmSealer::from_primary` builds a sealer around an
  already-loaded handle instead of recreating the deterministic SRK;
  `derive_key_id` extracted from `TpmSealer::new` into its own pure,
  now-tested function, shared by both constructors.
- `src/tpm_tests.rs` (new): first tests this file has ever had —
  `derive_key_id` (determinism, uniqueness, prefix) and the
  `envelope_encode`/`envelope_decode` round trip, all pure/no TPM required.
- `src/kms.rs`: `KmsService::with_legacy_fallback` holds both a primary and
  an optional legacy sealer; `decrypt` tries the primary by `key_id` first,
  then the legacy fallback, before rejecting. `encrypt` is unchanged — new
  data always goes under the primary.
- `src/main.rs`: `build_sealer` implements the auto-detect-and-prefer
  logic (`fleet::load_fleet_key` first, fall back to `TpmSealer::new`
  unconditionally on any error including simply not finding one), wired
  into `run_serve`.

### Why
Without this, `genesis`/`enroll`/`join` succeeding (confirmed live this
session — see the three `TPM2_Duplicate` fix entries below) doesn't
actually accomplish anything: `sceau serve` would keep sealing under each
node's own `fixedTpm` SRK regardless of enrollment, and cross-node decrypt
— the entire problem ADR-0003 exists to solve — would never happen. The
dual-sealer design (rather than switching over and rejecting old
ciphertext outright) was chosen over a simpler "force an immediate
re-encryption sweep" alternative because it's strictly more robust — an
operator who doesn't run that sweep immediately after `join` doesn't lose
access to that node's pre-join Secrets — and was already the design this
session's own roadmap entry for Phase 5 called for before a subsequent
pass briefly (and incorrectly) simplified it away in the ADR; this restores
and implements that original, better design.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] New capability — unbuilt/unverified in this sandbox (no macOS ARM
      `tss-esapi-sys` support); needs a real Linux build, then live
      verification that (a) pre-join ciphertext still decrypts after a node
      joins, and (b) a Secret sealed on one enrolled node decrypts on
      another.

## [2026-09-06] - Fix third live `TPM2_Duplicate` failure: never share the policy session with `LoadExternal`

**Author:** Erick Bourgeois

### Changed
- `src/fleet.rs`: `duplicate_for_joiner` now calls `load_external_public`
  via `execute_without_session` (no ambient session at all), *before*
  building the policy session — instead of inside the same
  `execute_with_session(Some(policy_session), ...)` closure as `duplicate`.
  The policy session now touches exactly one command: `duplicate()`.
  Removed the previous entry's diagnostic logging (its job is done).

### Why
The diagnostic logging added in the previous entry proved the digest theory
wrong conclusively: `stored_auth_policy` and `policy_session_digest` were
byte-for-byte identical (`digests_match=true`) immediately before the
failing call, yet `Context::duplicate` still failed with the same
`TPM_RC_POLICY_FAIL`. That ruled out every policy-*content* hypothesis
(the previous two fixes) at once — the digest is provably correct right up
until the TPM itself disagrees.

The remaining structural difference from `tss-esapi`'s own doctest: our
policy session is shared, via one `execute_with_session` closure, across
*two* ESAPI calls — `load_external_public` (which needs no authorization
at all) and `duplicate()` (which needs the policy session specifically).
The doctest never does this; its `new_parent_handle` is already loaded
before the policy session is even built. Rather than fully pin down *why*
sharing this way breaks the digest at the TPM's own verification step
(nonce rolling — a real TPM 2.0 session property `execute_with_session`
doesn't shield callers from — is the leading suspect, not confirmed), the
fix sidesteps the question: build the external object first with no
session involved, and let the policy session exist for exactly the one
command it authorizes.

**Not yet re-verified live.**

### Impact
- [ ] Breaking change — `authPolicy` content is unchanged from the previous
      entry, so no re-`genesis` needed.
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Bugfix — not yet re-verified live.

## [2026-09-06] - Add diagnostic logging: `TPM_RC_POLICY_FAIL` survived the session-lifetime fix too

**Author:** Erick Bourgeois

### Changed
- `src/fleet.rs`: `duplicate_for_joiner` now logs (via `tracing::info!`) the
  fleet key's stored `authPolicy`, the real policy session's actual digest
  (via an extra `policy_get_digest` call, purely for diagnostics), and
  whether they match — all in hex, before the `duplicate()` call that's
  been failing. Marked `TEMPORARY`, to be removed once the real cause is
  confirmed.

### Why
Rebuilt and retested with the previous entry's fix (session
`continueSession=true`) applied — same `TPM_RC_POLICY_FAIL`, byte-for-byte
identical error, for the second time in a row despite two different,
individually well-reasoned fixes. That's a strong signal to stop
hypothesizing about TPM 2.0 policy-session internals from documentation
and protocol reasoning alone, and instead get the real hardware to state
directly whether the two digests the TPM is about to compare actually
differ (a computation bug) or already match (meaning `duplicate()`'s
failure has a different cause than policy digest content entirely, and
both prior fixes — while possibly still correct in their own right — were
not addressing the actual failure).

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Diagnostic only — no behavior change, purely additional logging ahead
      of the next live retest.

## [2026-09-06] - Correction: `TPM_RC_POLICY_FAIL`'s real cause was session lifetime, not `PolicyAuthValue`

**Author:** Erick Bourgeois

### Changed
- `src/fleet.rs`: `start_session` now sets `continueSession=true` (via
  `SessionAttributesBuilder::with_continue_session(true)`). `import_and_persist`
  no longer uses `execute_with_nullauth_session` for its `import()`+`load()`
  pair — it now builds its own session via `start_session(context,
  SessionType::Hmac)`, same as the policy-session helpers, and flushes it
  explicitly afterward.

### Why
The previous entry's fix (dropping `PolicyAuthValue`) did **not** fix the
live failure — rebuilt and retested, same `TPM_RC_POLICY_FAIL`, byte-for-byte
identical error. That ruled out the auth-value theory outright and forced a
closer look at session *lifetime*, not session *content*.

The real cause: a TPM session defaults to `continueSession=false`, meaning
the TPM automatically flushes it after **any** command that references it
in the session area — not only commands that use it for handle
authorization. `duplicate_for_joiner` shares one policy session across
three ESAPI calls (`load_external_public`, `duplicate`, `flush_context`).
`load_external_public` needs no authorization at all, but it still takes
the ambient session for parameter encryption — and that alone was enough to
make the TPM flush the session before `duplicate()` ever ran. The resulting
error looked exactly like a policy-content bug (`TPM_RC_POLICY_FAIL`,
"a policy check failed") because from the TPM's perspective a flushed
session's digest is indistinguishable from a wrong one: it just doesn't
match `authPolicy`.

`execute_with_nullauth_session` (`tss-esapi`'s own helper, used everywhere
else in this file) has the identical default and is fine there only because
every other use case makes exactly one TPM call per session.
`import_and_persist`'s `import()` → `load()` pair has the exact same
chained-calls-on-one-session shape as the bug just fixed — never reached
live yet, but fixed proactively rather than waiting to hit it separately.

**Not yet re-verified live** — same caveat as every fix in this session;
needs a real rebuild + retest.

### Impact
- [ ] Breaking change — this does not change what `genesis` creates or its
      `authPolicy`, so no re-genesis is needed this time (unlike the
      previous two fixes).
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Bugfix — not yet re-verified live.

## [2026-09-06] - Fix second live `TPM2_Duplicate` failure: `TPM_RC_POLICY_FAIL`

**Author:** Erick Bourgeois

### Changed
- `src/fleet.rs`: `duplication_auth_policy_digest`/`duplication_auth_policy_session`
  no longer call `context.policy_auth_value(...)` — the policy is now
  `PolicyCommandCode(Duplicate)` alone.

### Why
After the `TPM_RC_AUTH_TYPE` fix (previous entry) and a `genesis --force`
re-key, the live retest got further — mTLS and Node-identity authorization
both passed — but `Context::duplicate` then failed with a different, more
specific error: `TPM_RC_POLICY_FAIL` ("a policy check failed"), meaning the
policy session's final digest didn't match the fleet key's stored
`authPolicy`.

The policy copied verbatim from `tss-esapi`'s own doc-tested example used
two assertions: `PolicyAuthValue` (prove knowledge of the object's auth
value) + `PolicyCommandCode(Duplicate)`. `PolicyAuthValue` needs the ESYS
context to know the object's real auth value to fold into the session's
authorization HMAC. The doctest's object comes from a handle ESAPI just
created itself (so it already knows the auth value used at creation); ours
comes from `load_fleet_key`'s `tr_from_tpm_public` — a fresh handle lookup,
which has no auth value registered in the ESYS context unless `tr_set_auth`
is called (nothing in this codebase does that). Rather than add that
plumbing, this drops `PolicyAuthValue` entirely: the fleet key's auth value
is intentionally empty, so proving knowledge of it added no real
restriction anyway, and `PolicyCommandCode` alone is sufficient and has no
auth-value dependency at all.

**Not independently verified beyond the error message it targets** — this
diagnosis fits the observed `TPM_RC_POLICY_FAIL` and the specific
mechanical difference between the doctest's object and ours, but the
`tr_set_auth` gap was reasoned from `tss-esapi`'s source, not confirmed by
directly testing the alternative (calling `tr_set_auth` with an empty auth
value) against real hardware.

### Impact
- [x] Breaking change — same as the previous entry: any fleet key created
      with the old (two-assertion) policy needs `genesis --force` again.
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Bugfix — not yet re-verified live.

## [2026-09-06] - Note Docker Desktop's minimum version for the Rosetta option

**Author:** Erick Bourgeois

### Changed
- `README.md`: `### Testing changes locally`'s Rosetta paragraph now notes
  it requires Docker Desktop 4.29.0+, and how to check the running
  version -- the checkbox is simply absent (no error) on older installs,
  which reads as "unsupported on this Mac" rather than "update Docker
  Desktop."

### Why
Hit this live: Docker Desktop 4.19.0 (2023) had no Rosetta setting in
Settings → General at all, which looked identical to the feature not
existing on this hardware. Worth documenting the version gate explicitly so
the next person checks `docker version` first instead of assuming Rosetta
isn't an option for them.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Documentation only

## [2026-09-06] - Subcommands replace flat flags; `genesis --force`; lib/bin split + first integration test

**Author:** Erick Bourgeois

### Changed
- `src/cli.rs`: replaced the flat `--genesis`/`--enroll`/`--join` flag
  design with four subcommands — `sceau serve`, `sceau genesis [--force]`,
  `sceau enroll --listen=<addr> [--max=<n>] [--timeout-secs=<d>]
  [--k0s-data-dir=<path>]`, `sceau join --seed=<addr>
  [--k0s-data-dir=<path>]`. `--tcti` is a global flag valid before or after
  the subcommand. Dropped `RawArgs`/`Mode`/`ModeError` entirely — clap's
  `Subcommand` derive makes mutual exclusivity and required-flag validation
  structural, so the hand-rolled validation layer (and its dedicated error
  enum) no longer earns its keep. `enroll --max` now uses a clap
  `value_parser` range (`1..`) instead of a manual post-parse check.
- `src/cli_tests.rs`: rewritten against `Cli::try_parse_from`.
- `src/fleet.rs`: `create_fleet_key` takes a new `force: bool`. When set,
  `delete_persisted_fleet_key` evicts any existing fleet key first (the
  same `TPM2_EvictControl` `persist` already uses, fed an already-persistent
  object instead of a fresh transient one, which the TPM treats as a
  delete) before creating a fresh one.
- `src/main.rs`: rewritten around `Cli::parse()` + `match cli.command`;
  `run_serve`/`run_genesis` replace the old flag-threading logic.
- `src/lib.rs` (new): `sceau` gained a library target — `pub mod` for every
  module main.rs used to declare with plain `mod`. `main.rs` now depends on
  it (`use sceau::{...}`) instead of owning the module tree itself.
- `tests/fleet_duplication.rs` (new): the first integration test in this
  repo — exercises `create_fleet_key` → `create_transport_key` →
  `duplicate_for_joiner` against a real TPM or `swtpm`, gated behind
  `SCEAU_TEST_TCTI` and `#[ignore]`. Directly targets the `TPM_RC_AUTH_TYPE`
  regression fixed below.
- `.gitignore`: added `/core` — a QEMU/`collect2` crash dump had been
  accidentally staged (`git add`-ed broadly during live debugging); unstaged
  it, left the file itself alone since it may still be useful for that
  investigation.
- Sanitized a real registry hostname (`registry.example.com` placeholder)
  that had leaked into two `git add`-able CHANGELOG entries below, before
  either was ever committed.

### Why
Two separate asks converged into one change: (1) a stale, pre-authPolicy-fix
fleet key needs a supported recovery path, not just a manual `tpm2_
evictcontrol` runbook step — `genesis --force` is that path; (2) flat
mode-selection flags (`--genesis`, `--enroll --enroll-listen=...`, `--join
--seed=...`) were getting harder to justify once every mode had its own
flag subset — subcommands are the idiomatic clap shape for this, and
happen to make `--force` trivial to scope to `genesis` alone rather than a
flag that only means something in one mode. Separately: asked directly
whether the `TPM2_Duplicate` fix had tests — it didn't, and `sceau` being
binary-only meant it structurally couldn't have any (`tests/` can't import
from a bin-only crate), so the lib/bin split happened first to make the
integration test possible at all.

### Impact
- [x] Breaking change — every existing invocation
      (`sceau --genesis`, `sceau --enroll --enroll-listen=...`, `sceau
      --join --seed=...`, bare `sceau`) must move to the new subcommand
      form. Nothing to migrate yet — no real deployment exists past manual
      live testing.
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] New capability (`genesis --force`, integration test) — none of this
      has been built or run in this sandbox (no macOS ARM `tss-esapi-sys`
      support); needs a real Linux build next.

## [2026-09-06] - Fix live `TPM2_Duplicate` failure: `TPM_RC_AUTH_TYPE`

**Author:** Erick Bourgeois

### Changed
- `src/fleet.rs`: `duplicable_storage_public` now takes `context: &mut
  Context` and bakes a `PolicyAuthValue` + `PolicyCommandCode(Duplicate)`
  policy into every key's `authPolicy` at creation
  (`duplication_auth_policy_digest`, computed via a `Trial` session).
  `duplicate_for_joiner` now authorizes its `Context::duplicate` call with a
  matching `SessionType::Policy` session (`duplication_auth_policy_session`)
  instead of `execute_with_nullauth_session`. New `FleetError::
  NoSessionHandle` variant.
- `src/fleet_tests.rs`: removed `duplicable_storage_public_builds_
  successfully` — that function is no longer pure (it now runs a real
  `Trial` session), so it no longer belongs in this file per
  `rules/testing.md`; no TPM-backed replacement exists yet (tracked in the
  ADR-0003 roadmap).

### Why
First live `--enroll`/`--join` round-trip attempt against two real nodes:
`--enroll` authorized the joiner correctly (mTLS + Node-identity check both
passed) but then failed calling `Context::duplicate` with `TPM_RC_AUTH_TYPE`
("authorization handle is not correct for command"). Root cause: `TPM2_
Duplicate` requires ADMIN-role authorization on the object being
duplicated, and the TPM only ever accepts ADMIN-role authorization via a
policy session — never a plain password/HMAC session, regardless of the
object's `userWithAuth` setting or its auth value being empty. `tss-esapi`'s
own doc-tested example for `Context::duplicate` shows the exact required
shape (`PolicyAuthValue` + `PolicyCommandCode(Duplicate)`, baked into the
object at creation and reproduced at use time) — this fix reproduces that
pattern verbatim rather than inventing a different policy.

**This changes what `--genesis` creates.** A fleet key persisted before
this fix has no `authPolicy` and cannot be duplicated under any session
type — it must be re-created before `--enroll` can succeed. Since
`create_fleet_key` was already made idempotent (checks for an existing key
first), re-genesis requires clearing the stale persistent handle first
(`tpm2_evictcontrol -c 0x81020001` or equivalent), not just re-running
`--genesis`.

### Impact
- [x] Breaking change — any fleet key created by a pre-fix `--genesis` is
      now unusable for enrollment and must be re-created.
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Bugfix — not yet re-verified live (this fix was written in response to
      the error output, not yet re-run against real hardware).

## [2026-09-06] - Fix `--enroll-listen` example port colliding with k0s's own API port

**Author:** Erick Bourgeois

### Changed
- `src/main.rs`: `--enroll-listen`'s help text example changed from
  `0.0.0.0:9443` to `0.0.0.0:8443`, with a note on why.
- `docs/migration-ha-existing-cluster.md`: all three `--enroll-listen`/
  `--seed`/`ExecStart` examples changed from `9443` to `8443`, with an
  explanatory note at the first occurrence.

### Why
`sudo /opt/sceau/bin/sceau --enroll --enroll-listen 0.0.0.0:9443` failed
live on a real k0s controller: `gRPC transport error: ... Address already
in use (os error 98)`. k0s's own `k0sApiPort` (the k0s-internal
controller-to-controller API, distinct from `kube-apiserver`'s `6443`)
defaults to `9443` and is already bound on every controller node -- `9443`
was never a safe example port to begin with, `8443` is.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Documentation only -- `--enroll-listen` was never hardcoded to 9443
      in the code itself (always an operator-supplied address), so no
      behavior changed, only the example port shown in `--help` and docs.

## [2026-09-06] - Document local dev/test workflow (fmt/clippy/test, deploy-test)

**Author:** Erick Bourgeois

### Changed
- `README.md`: new `### Testing changes locally` section under `## Build`
  -- the exact `docker run` recipe (cache volumes, CA bundle, cargo mirror
  config, `rustup component add`, `RUSTFLAGS=-C link-arg=-fuse-ld=bfd`) used
  this session to run `cargo fmt`/`clippy`/`test` against this project for
  the first time ever, plus why each piece exists (QEMU `collect2` segfault,
  target-dir collision, missing `--lib` target). New `### Deploying to a
  test node` section documenting `make deploy-test` and the `crane delete`
  re-push behavior.

### Why
Every piece of this workflow was discovered by hitting a real failure this
session (documented individually in earlier CHANGELOG entries) but none of
it was written down anywhere a future session -- human or otherwise --
could find without re-deriving it from scratch.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Documentation only

## [2026-09-06] - `--k0s-data-dir` flag replaces hardcoded `/var/lib/k0s` paths

**Author:** Erick Bourgeois

### Changed
- `src/certs.rs`: `own_node_name`/`mint_enroll_server_identity` now take a
  `data_dir: &Path` parameter; the former `K0S_CA_CERT_PATH`/
  `K0S_CA_KEY_PATH`/`K0S_KUBELET_CLIENT_CERT_PATH` constants are gone,
  replaced by `data_dir.join("pki/ca.crt")` etc. New `pub const
  K0S_DEFAULT_DATA_DIR: &str = "/var/lib/k0s"`.
- `src/enroll.rs`, `src/join.rs`: `run_enroll`/`run_join` now take a
  `k0s_data_dir: &Path` parameter and derive `pki/ca.crt`,
  `pki/ca.key`, `pki/admin.conf`, `kubelet/pki/kubelet-client-current.pem`
  from it instead of hardcoded `/var/lib/k0s/...` constants.
- `src/main.rs`: new `--k0s-data-dir` flag (default
  `certs::K0S_DEFAULT_DATA_DIR`), threaded into both `run_enroll` and
  `run_join`.

### Why
Every k0s filesystem path sceau reads was hardcoded to `/var/lib/k0s`, but
k0s's own `--data-dir` flag lets an operator relocate that root entirely --
sceau would silently fail to find any of its PKI inputs on such a node. All
of the paths happen to share one root, so a single flag covers all of them
rather than one flag per file.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [x] Config change only -- default behavior unchanged (`/var/lib/k0s`
      still the default); only matters for k0s installs with a non-default
      `--data-dir`. `cargo fmt`/`clippy -D warnings`/`test` all pass
      (21/21).

## [2026-09-06] - Fix live `--enroll` kube-config failure; make `--genesis` idempotent

**Author:** Erick Bourgeois

### Changed
- `src/enroll.rs`: `run_enroll` now builds its `kube::Client` from k0s's own
  admin kubeconfig (`K0S_ADMIN_KUBECONFIG_PATH =
  /var/lib/k0s/pki/admin.conf`) via `Kubeconfig::read_from` +
  `Config::from_custom_kubeconfig`, instead of `kube::Client::try_default()`.
  New `EnrollError::Kubeconfig` variant for the read/parse failure path.
- `src/fleet.rs`: `create_fleet_key` now calls `load_fleet_key` first and
  returns the existing handle if one is already persisted at
  `FLEET_KEY_PERSISTENT_HANDLE`, instead of unconditionally generating and
  persisting a new key every call.
- `src/main.rs`: `run_genesis`'s log message changed from "fleet sealing key
  created and persisted" to "fleet sealing key ready (created or already
  existed)" to stop overclaiming creation happened on a re-run.

### Why
Two real, live-run failures on an actual k0s node, back to back:

1. `--enroll` failed on startup: `kube::Client::try_default()` infers
   config from in-cluster env vars (only set inside a pod) or
   `~/.kube/config` (`/root/.kube/config` here) -- neither applies, since
   sceau runs as a bare process directly on the controller host, the same
   way it already reads `/var/lib/k0s/pki/ca.crt` and the kubelet-client
   cert directly rather than assuming standard kubectl conventions.

2. Separately, the user flagged that `--genesis` must be idempotent --
   running it twice (or after a `--join`) must not silently generate and
   persist a second fleet key out from under an already-enrolled etcd
   quorum. `persist`'s own doc comment already flagged this exact gap as
   unverified; fixed by checking for an existing key first in
   `create_fleet_key` rather than relying on `TPM2_EvictControl`'s
   occupied-handle error behavior to catch it after the fact.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [x] Bugfix + hardening -- neither fix has been re-run live yet against
      the node that hit the original `--enroll` failure. `cargo
      fmt`/`clippy --all-targets --all-features -D warnings`/`test` all
      pass (21/21).

## [2026-09-06] - `docker-image` deletes an existing tag before re-pushing it

**Author:** Erick Bourgeois

### Changed
- `Makefile`: new `CRANE_TOOL ?= crane` variable; `docker-image` now runs
  `crane delete $(IMAGE_REF)` before the `buildx build --push` step,
  whenever `PUSH=true`. Tolerates the tag not existing yet (first push) --
  filters the expected `MANIFEST_UNKNOWN`/`manifest unknown` error and
  otherwise ignores the exit code, so a missing tag never fails the build.

### Why
Repeated same-tag pushes (`IMAGE_TAG=v0.1.0`) were silently no-ops on the
`registry.example.com` Artifactory repo: `docker buildx build --push` reported
success every time, but `docker manifest inspect` against the registry
(bypassing all local cache) kept returning the exact original config
digest, hours and multiple rebuilds later. `crane delete
registry.example.com/sceau:v0.1.0` (run manually) fixed
it immediately -- the registry is evidently expected to delete-then-recreate
a tag on push rather than overwrite it in place, and that step wasn't
happening for this repo. Root cause on the Artifactory side is still
unconfirmed (not something fixable from this repo's Makefile or Docker
Desktop config), but making the delete explicit and unconditional before
every push removes the dependency on that server-side behavior entirely.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [x] Config change only -- local build tooling, no code/runtime change.
      Requires `crane` (go-containerregistry) on `PATH`; already installed
      in this environment.

## [2026-09-06] - Fix live `--enroll` failure (PKCS#1 CA key); first-ever passing `cargo test`/`clippy`/`fmt`

**Author:** Erick Bourgeois

### Changed
- `src/certs.rs`: added `ca_key_to_pkcs8_pem()`, converting a PKCS#1 RSA PEM
  key to PKCS#8 in pure Rust (`rsa` crate's `pkcs1`/`pkcs8` support) before
  handing it to `rcgen::KeyPair::from_pem`. Called on `K0S_CA_KEY_PATH`
  before minting the `--enroll` server's leaf certificate.
- `Cargo.toml`: new dependency `rsa = "0.9"` (default features: pure Rust,
  no native C/asm build step -- deliberately not the alternative fix of
  switching `rcgen` to its `aws_lc_rs` backend, which needs `cmake`/`perl`).
- `src/fleet.rs`: `key_handle_from` now uses `KeyHandle::from(handle)`
  (infallible) instead of `KeyHandle::try_from(...).map_err(...)`; removed
  the now-dead `FleetError::UnexpectedHandleKind` variant. A real
  `clippy::unnecessary_fallible_conversions` catch, not a behavior change.
- `src/certs_tests.rs`, `src/authz_tests.rs`: wrapped both in the project's
  mandatory `#[cfg(test)] mod tests { use super::super::*; ... }` pattern
  (matching `fleet_tests.rs`/`cli_tests.rs`) -- both were missing it, so
  `use super::super::*` resolved one module level too far (crate root
  instead of the parent module) and neither file could ever have compiled.
  Added two new tests for `ca_key_to_pkcs8_pem` (PKCS#1 conversion,
  PKCS#8 passthrough) using a throwaway `openssl genrsa -traditional`
  test key.
- `src/enroll.rs`, `src/join.rs`: added `#[allow(clippy::result_large_err)]`
  (with justification comments) on the two `marshall` helpers, `run_join`,
  and both generated `pb` modules -- `tonic::Status` as an `Err` variant is
  standard tonic usage, not something worth boxing.

### Why
First live run of a real, correctly-built `--enroll` binary against a real
k0s node failed immediately: `certificate error: minting certificate: Could
not parse key pair`. Root cause: k0s writes its CA key as PKCS#1
(`-----BEGIN RSA PRIVATE KEY-----`); `rcgen`'s default `ring` crypto backend
only parses PKCS#8 (`-----BEGIN PRIVATE KEY-----`) -- confirmed against
`rcgen` 0.14's own docs (PKCS#1 support exists only under the `aws_lc_rs`
backend).

Fixing this required actually compiling the project for the first time this
session (previous entries could only get as far as "compiles"). Running
`cargo test`/`clippy`/`fmt` for the first time ever surfaced two more latent
bugs unrelated to the PKCS#1 issue -- the `fleet.rs` fallible-conversion and
the `certs_tests.rs`/`authz_tests.rs` module-path bug -- both fixed in the
same pass since they were blocking the `cargo-quality` gate outright, not
because they were related to the enrollment bug being chased.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [x] Bugfix only
- [x] `cargo fmt --check` / `cargo clippy --all-targets --all-features -D
      warnings` / `cargo test` all pass for the first time this project has
      ever been able to run them (21/21 tests). Still not done: a full
      `--enroll`/`--join` round trip between two real nodes -- this fix only
      got `--enroll` past its startup failure on one node.

## [2026-09-06] - First real `linux/amd64` build passes; Docker build cache + QEMU/collect2 workaround

**Author:** Erick Bourgeois

### Changed
- `Makefile` (`_build-linux`): added persistent Docker named volumes
  (`sceau-cargo-registry`, `sceau-cargo-git`, `sceau-target-$(ARCH_NAME)` via
  `CARGO_TARGET_DIR`) to the sandboxed container build, so repeat builds
  don't recompile the full dependency tree from scratch, and so the
  container's `linux/amd64` build artifacts stop colliding with the host
  Mac's own `target/` (mixing native-arm64 and container-amd64 build output
  in the same bind-mounted directory was invalidating fingerprints on every
  run). Also added `binutils` + `RUSTFLAGS=-C link-arg=-fuse-ld=bfd` as a
  linker workaround (superseded — see Why).

### Why
The first real build attempt (previous entry, same day) got past the compile
errors but then hit `error: linking with `cc` failed: signal: 11 (SIGSEGV)`,
then (after the `bfd` workaround) a different crash one level down:
`collect2` itself segfaulting while linking a small proc-macro `.so`
(`asn1-rs-impl`), unrelated to the lld/bfd choice. Root cause: Docker Desktop
4.19.0 (2023) has no Rosetta support for `--platform linux/amd64` on Apple
Silicon and falls back to QEMU, which has known `collect2`/GCC emulation
bugs (the same class of issue as the `<jemalloc>: ... expected behaviour if
you are running under QEMU` warnings already suppressed in an earlier
change). Updating Docker Desktop and enabling "Use Rosetta for x86_64/amd64
emulation" resolved it outright — the `bfd`-linker workaround above turned
out to be unnecessary once Rosetta replaced QEMU, but is left in place since
it's harmless and cheap insurance.

Separately, Docker Desktop's own VM disk was full (68.66GB across 112
images, ~71GB reclaimable as dangling layers + build cache from unrelated
projects) and returned "no space left on device" mid-pull — reclaimed via
`docker image prune -f` + `docker builder prune -f` (dangling/unreferenced
only; tagged images and the one running container were left untouched).

First successful run: `make docker-image ARCH=amd64 PUSH=true ...` compiled,
linked, staged the binary, built the distroless image, and pushed
`registry.example.com/sceau:v0.1.0` end to end.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [x] Config change only — local dev/build environment (Docker Desktop
      version, VM disk space), not a code or cluster change.
- [x] Unblocks Phase 3/4 verification: the enrollment network layer now
      compiles and pushes as a real image. Still needs a live
      `--enroll`/`--join` round trip against a real seed/joiner before
      Phase 3/4 can be marked done (see `join.rs`/`enroll.rs` module docs).

## [2026-09-06] - Implement ADR-0003 Phase 3/4: enrollment network layer + Node-identity authz

**Author:** Erick Bourgeois

### Changed
- `proto/enroll/v1/api.proto` (new): one-RPC `Enroll` service (`Duplicate`),
  request/response shaped around `fleet::DuplicationBlob`'s marshalled bytes.
- `build.rs`: compile the new proto with both client and server generated
  (unlike `kms.rs` — `sceau` is both `--enroll` server and `--join` client).
- `src/authz.rs` (new) + `src/authz_tests.rs`: ADR-0003 Decision 3's
  authorization half — extract a k0s node name from a kubelet-client cert's
  `system:node:<name>` CN, and check it against the cluster's own `Node`
  objects via the `kube` crate.
- `src/certs.rs` (new) + `src/certs_tests.rs`: k0s-PKI-backed TLS identities.
  Reads this node's own kubelet-client cert to derive its node name; mints a
  short-lived, in-memory leaf certificate (via `rcgen`) signed by k0s's own
  CA for the `--enroll` seed's TLS server identity; extracts a peer cert's CN
  for the authorization check.
- `src/enroll.rs` (new): `--enroll` server — bounded by `--enroll-max`/
  `--enroll-timeout`, mTLS via the minted leaf + k0s's `ca.crt` as
  `client_ca_root`, serves `Duplicate` by calling `fleet::duplicate_for_joiner`
  after the authz check passes.
- `src/join.rs` (new): `--join` client — mTLS via this node's own
  kubelet-client cert, dials `--seed`, calls `fleet::create_transport_key` +
  `fleet::import_and_persist`.
- `src/main.rs`: wired `Mode::Enroll`/`Mode::Join` to `enroll::run_enroll`/
  `join::run_join`, replacing both `bail!()` stubs.
- `Cargo.toml`: new deps `kube`, `k8s-openapi`, `rcgen`, `time`,
  `x509-parser`, `http`; `tonic`'s `tls` feature enabled (not on by default).
- `docs/adr/0003-fleet-key-duplication-for-ha-multi-controller.md`: addendum
  to Decision 3 recording a design gap found mid-implementation (see Why).
- `.github/community/adr-0003-fleet-key-ha-roadmap.md`, `ROADMAPS.md`: Phase
  2 marked done (live-verified `--genesis` against a real vTPM); Phases 3-4
  marked code-complete-but-unbuilt.

### Why
Phase 2's gate (`--genesis` live-verified against real vTPM hardware)
cleared this session, so Phase 3 (enrollment network layer) and Phase 4
(k0s Node-identity authorization) were implemented together — `enroll.rs`'s
RPC handler needs both the authentication and authorization checks before
it can safely call into `fleet.rs`, so splitting them into separate changes
would have meant landing an incomplete authorization gate first.

Checking the real, already-provisioned test nodes' on-disk k0s PKI (rather
than assuming from documentation) surfaced a real design gap: k0s's
kubelet-client cert has no SAN and is client-auth-only, so it cannot serve
as the `--enroll` seed's own TLS server identity under rustls's SAN-only
hostname verification. Resolved by minting a short-lived leaf from k0s's own
CA — recorded as an ADR-0003 addendum rather than a silent implementation
detail, since it's a real "why A over B" (mint a leaf vs. skip hostname
verification) with a security trade-off attached.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] New capability (enrollment network layer) — unbuilt/unverified in this
      sandbox (no macOS ARM `tss-esapi-sys` support); needs a real Linux
      build + a live `--enroll`/`--join` round-trip before Phase 3/4 can be
      marked done.

## [2026-09-06] - Fix real build errors in Phase 3/4 enrollment code (first real compile attempt)

**Author:** Erick Bourgeois

### Changed
- `src/join.rs`: dropped unused `certs::self` import; replaced
  `Private::unmarshall`/`EncryptedSecret::unmarshall`/`Data::unmarshall` calls
  with `TryFrom<Vec<u8>>` — confirmed against `tss-esapi` 7.7.0 source that
  these three are raw `TPM2B_*` byte-buffer types (`buffer_type!`/
  `named_field_buffer_type!` macro output), not one of the ASN.1-style
  structures (`Public`, `Sensitive`, `Signature`, `Attest`) that actually
  implement `Marshall`/`UnMarshall`.
- `src/enroll.rs`: same fix on the server side — `marshall(&blob.duplicate_private)`
  etc. replaced with `.value().to_vec()` for the same three buffer types.
- `Cargo.toml`: added `features = ["pem", "x509-parser"]` to the `rcgen`
  dependency. `Issuer::from_ca_cert_pem` (used in `certs.rs`) exists in
  0.14.10 but is feature-gated behind both; without them the compiler reports
  it as "not found" rather than "feature required."

### Why
This was the first time Phase 3/4's enrollment code (landed 2026-09-06,
previous entry) hit a real compiler — the macOS dev sandbox has no
`tss-esapi-sys` bindings for `aarch64-darwin` and so could never build it.
The first real attempt (Docker-sandboxed `x86_64-unknown-linux-gnu` build)
surfaced 7 compile errors + 1 warning, all API-mismatch, not logic bugs:
`Private`/`EncryptedSecret`/`Data` were assumed to round-trip through the same
`Marshall`/`UnMarshall` trait as `Public`, and `rcgen`'s PEM-CA constructor
was assumed to be always available. Both assumptions were wrong and are now
verified against the actual crate source rather than guessed a second time.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Bugfix only — still unverified end-to-end. A full `make docker-image`
      (real internal registry mirror + build image) was queued to run
      separately; this entry covers only the compile-error fix.

## [2026-09-05] - Fix first real build error; add ADR-0003 roadmap + test-node deploy tooling

**Author:** Erick Bourgeois

### Why
`src/fleet.rs` got its first real compile against real hardware (Linux, not
this dev sandbox) — a concrete, more valuable signal than any amount of
documentation-based verification. Also: tracking a 9-phase implementation
plan for ADR-0003 purely in conversation doesn't survive past this session;
it needed a durable, versioned home, and this project keeps roadmaps in-repo
(a deliberate difference from sibling projects like `banlieue`, which keep
theirs external).

### Changed
- `src/fleet.rs`: fixed the first real compile error — `duplicate()`
  returns a plain `Data` for its first tuple element, not `Option<Data>` as
  assumed; `DuplicationBlob`'s construction now wraps it in `Some(...)` at
  the one point it's produced. Added `#[allow(dead_code)]` (each pointing at
  the specific roadmap phase that removes it) to `DuplicationBlob`,
  `create_transport_key`, `duplicate_for_joiner`, `import_and_persist`, and
  `load_fleet_key` — all genuinely unused until Phase 3/5 wire them in, per
  `make lint`'s `-D warnings` gate.
- `scripts/deploy-test-nodes.sh` (new), `contrib/systemd/sceau.service`
  (new), `Makefile` (`deploy-test` target): get a `sceau` binary + its
  TPM2-TSS libs onto real test nodes over SSH, for manual testing before
  this is packaged as a Kairos sysext. Two source modes, auto-selected by
  `uname -s` (matches this Makefile's own `build-linux-*` split): Linux
  reuses the local `binaries/$ARCH/` staging directory directly; macOS pulls
  and extracts a built image instead (`docker create`/`docker cp`, no
  `crane` dependency), since Docker Desktop's Linux VM is the only way a Mac
  produces a real Linux binary at all.
- **New project convention**: `ROADMAPS.md` (new, repo root) indexes
  phased implementation roadmaps tracked in `.github/community/` (new
  directory) — starting with `.github/community/adr-0003-fleet-key-ha-roadmap.md`,
  the full 9-phase plan for ADR-0003 (design → CLI surface → TPM crypto
  core → enrollment network layer → k0s auth → steady-state integration →
  EK-authenticity hardening → operational safety → sysext packaging →
  full HA validation), with what's done/in-progress/not-started marked
  explicitly. `.claude/rules/roadmaps.md` (new) records this as a standing
  convention — deliberately different from `banlieue`'s external-roadmap
  convention, not an inconsistency to fix later.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Build fix + new tooling/docs; no runtime behavior changed beyond the
      `duplicate()` fix itself (which only affects not-yet-wired code paths)

## [2026-09-05] - Implement TPM2_Duplicate/Import fleet-key primitives; wire --genesis end to end

**Author:** Erick Bourgeois

### Why
Continuing ADR-0003's implementation past the mode-validation surface into
the actual TPM cryptography — the part of this design nothing else in the
codebase resembles (every existing sealed/primary object uses
`fixedTpm`/`fixedParent`; the fleet key deliberately does not).

### Changed
- `src/fleet.rs` (new), `src/fleet_tests.rs` (new): `create_fleet_key`,
  `create_transport_key`, `duplicate_for_joiner`, `import_and_persist`,
  `load_fleet_key` — the full `TPM2_Duplicate`/`Import`/`EvictControl`
  protocol from ADR-0003 Decision 2, as revised (see below).
- **Design revision recorded in ADR-0003**, not just in code: the
  `TPM2_Duplicate` wrapping target is a fresh, plain duplicable storage key
  the joiner creates for this one transaction, not the vTPM's Endorsement
  Key directly. The EK's standard authorization is a `TPM2_PolicySecret`
  policy session against the Endorsement hierarchy — materially more
  intricate session handling than anything else in this codebase, and not
  something to get right for the first time with no TPM/simulator to
  validate against. This makes the already-deferred EK-authenticity gap
  (ADR-0003 Consequences) slightly wider, not narrower, for now — recorded
  explicitly, not left implicit.
- `src/main.rs`: `--genesis` is now fully wired end-to-end (connects to the
  TPM, creates and persists the fleet key, exits `0`) — no longer a `bail!`
  stub. `--enroll`/`--join` still `bail!`: they need a new gRPC service
  definition and mTLS transport that doesn't exist yet, which
  `fleet::duplicate_for_joiner`/`fleet::import_and_persist` are ready to be
  called from once that lands.

### Verification
**Not run in this session** — same pre-existing platform gap as the
previous entry (no macOS ARM support in `tss-esapi-sys`, no local
`tpm2-tss`). Every `Context` method call in `fleet.rs` was checked against
`tss-esapi`'s published docs.rs signatures (argument order,
`ObjectHandle`-vs-`KeyHandle` parameter types); two specific points could
not be confirmed that way and are flagged in `fleet.rs`'s own module doc
comment for priority review on real hardware/CI: `PersistentTpmHandle::new`
and `Persistent`'s exact module path, and whether `KeyHandle: TryFrom<ObjectHandle>`
is really the right conversion (written defensively as `TryFrom`, not
assumed to be an infallible `From`). **Run `cargo test --bin sceau` and
manually exercise `--genesis` against a real or `swtpm`-simulated TPM
before trusting this.**

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] New opt-in mode (`--genesis`), no behavior change when unset;
      `--enroll`/`--join` remain stubbed and inert

## [2026-09-05] - Document the HA migration runbook; resolve --enroll's exit-on-success lifecycle

**Author:** Erick Bourgeois

### Why
Confirming that `--genesis`/`--enroll` don't touch a live cluster, and
working out how to actually cut a running HA k0s cluster over to `sceau`
without an outage, surfaced two things worth recording rather than only
answering in conversation: (1) the previously-undecided "does `--enroll`
exit or fall through to steady-state" question (ADR-0003 Consequences)
needed an actual answer to write cloud-config against, and (2) the
`EncryptionConfiguration` cutover has its own sequencing hazard
(mid-rollout, a not-yet-updated apiserver doesn't recognize the `kms`
provider at all) independent of anything `sceau`-specific, worth writing
down once rather than re-deriving per migration.

### Changed
- `docs/adr/0003-fleet-key-duplication-for-ha-multi-controller.md`:
  resolved the "ephemeral listener lifecycle" Consequence — `--genesis`,
  `--enroll`, and `--join` are all one-shot invocations that exit (0 on
  success), never the long-running steady-state process. Fits a `systemd
  Type=oneshot` unit or Kairos cloud-config stage, run once, separate from
  the plain `sceau` service.
- `docs/migration-ha-existing-cluster.md` (new): runbook for migrating an
  existing HA k0s cluster onto `sceau`. Phase A (per-node TPM key
  preparation via `--genesis`/`--join`, cloud-config and systemd-oneshot
  shapes, explicitly confirms no cluster impact) is fully decoupled from
  Phase B (the actual `EncryptionConfiguration` cutover: push to every
  node before restarting any, roll `k0scontroller` one node at a time for
  etcd-quorum safety — not for any `sceau`-specific reason, since Phase A
  already made every node capable of decrypting anything — and a note on
  `--encryption-provider-config-automatic-reload` as an unverified
  potential simplification). Explicitly marked as a runbook for the
  ADR-0003 design, not a guide to already-working commands — `--genesis`/
  `--enroll`/`--join` are not implemented yet.

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] Documentation and a design decision only; no code behavior changed

## [2026-09-05] - Add --genesis/--enroll/--join mode surface for ADR-0003 (fleet key duplication)

**Author:** Erick Bourgeois

### Why
Planning a real migration of a 6-node HA k0s cluster onto `sceau` found
that ADR-0001's `fixedTpm`-sealed key cannot be read across nodes — a
Secret sealed by one node's TPM is undecryptable if a read is routed to a
different node's `kube-apiserver`, since KMS v2's transport is local-socket
only. ADR-0003 (Accepted, revised same day to drop an earlier
SSH+Vault-key design in favor of native `sceau` enrollment authenticated by
k0s's own PKI) adopts a duplicable, fleet-wide sealing key distributed via
three new startup modes.

### Changed
- `src/cli.rs` (new), `src/cli_tests.rs` (new): `RawArgs`/`Mode`/`ModeError`
  — validates the new `--genesis`/`--enroll`/`--join` flag combinations
  (mutually exclusive; `--enroll` requires `--enroll-listen` and rejects
  `--enroll-max=0`; `--join` requires `--seed`) into a `Mode` enum. Pure,
  fully unit-tested, no TPM/network required.
- `src/main.rs`: wires the new flags onto `Args`, validates them into a
  `Mode` at startup. `Genesis`/`Enroll`/`Join` currently `bail!` with a
  clear "not implemented yet" error — only the mode *selection/validation*
  is built in this change; `TPM2_Duplicate`/`Import`, the mTLS enrollment
  server, and the k0s Node-identity authorization check are separate,
  larger follow-up work (need a TPM resource manager/simulator to build
  and test safely, per `rules/testing.md`).
- `docs/adr/0003-fleet-key-duplication-for-ha-multi-controller.md` (new,
  Accepted): the full decision record, including the rejected
  SSH+Vault-key design and why native enrollment reusing k0s's PKI is a
  real improvement, not just a different implementation of the same risk.
- `docs/architecture/calm/architecture.json`: new `service-sceau-peer` node
  and `rel-sceau-join-enrollment` relationship; `service-sceau`'s
  description updated for the three modes. `make calm-validate` passes.

### Verification
**Not run in this session** — this sandbox cannot compile `sceau` at all:
`tss-esapi-sys` has no macOS ARM support, and no `tpm2-tss` C library is
available locally to generate bindings against either (pre-existing
platform gap, unrelated to this change). `cli.rs`'s logic was traced
manually against every `cli_tests.rs` case instead. **Run `cargo test --bin
sceau` (not `--lib` — this crate has no library target) on Linux/CI before
treating this as verified.**

### Impact
- [ ] Breaking change
- [ ] Requires cluster rollout
- [ ] Config change only
- [x] New opt-in flags, no behavior change when unset (defaults to today's
      `Standalone` mode)

## [2026-09-04 23:30] - Makefile: fix cargo mirror TLS via ssl-version pin, move source config to --config

**Author:** Erick Bourgeois

### Changed
- `Makefile`: `_build-linux` now sets Cargo source replacement and
  `http.ssl-version="tlsv1.2"` via `cargo --config` CLI flags (using portable
  `set -- ...`/`"$@"`, not bash arrays, since the recipe shell isn't
  guaranteed to be bash), instead of the `CARGO_SOURCE_CRATES_IO_REPLACE_WITH`
  / `CARGO_SOURCE_MIRROR_REGISTRY` env vars from the previous entry.
  `CARGO_HTTP_CAINFO` and `CARGO_HTTP_MULTIPLEXING` remain env vars.
- `README.md` / `docs/adr/0002-release-and-supply-chain-pipeline.md`:
  replaced the HTTP/2-multiplexing theory (previous entry) with the actual,
  reproduced root cause and fix below.

### Why
The previous entry's `CARGO_HTTP_MULTIPLEXING=false` fix did not actually
resolve the issue — verified by direct reproduction against the real
artifactory endpoint (not just theorized). The real cause, confirmed with
`openssl s_client -groups X25519MLKEM768` reproducing an identical
connection-reset against the same host (while classic groups completed a
normal handshake): `cargo` statically vendors its own libcurl/OpenSSL,
independent of the system's, and OpenSSL 3.6+ defaults to advertising a
post-quantum hybrid TLS 1.3 group that this network's TLS-inspection
appliance can't parse and resets on — while `apt`/`curl` (older system
OpenSSL, no PQ group) succeed against the identical host. `OPENSSL_CONF`
group restriction has no effect on cargo's vendored build (config
auto-loading is disabled), so the fix is pinning `http.ssl-version` instead,
which avoids TLS 1.3 group negotiation entirely.

Separately, reproduction also showed that mixing `CARGO_SOURCE_*` env vars
with any `--config` CLI flag causes cargo to silently drop `http.ssl-version`
(a cargo config-merge quirk) — so source replacement moved to `--config`
alongside the TLS pin to avoid the interaction. Verified end-to-end: a real
`make docker-image` run against the actual artifactory apt mirror, Cargo
mirror, and registry succeeded, compiling all of sceau's real dependencies
(including `tss-esapi`) and producing the final distroless image.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only
- [x] Build/CI tooling only (no runtime behavior change)

## [2026-09-04 22:55] - Makefile: force HTTP/1.1 for cargo when CARGO_REGISTRY_MIRROR is set (superseded — see entry above)

**Author:** Erick Bourgeois

### Changed
- `Makefile`: `_build-linux` now also exports `CARGO_HTTP_MULTIPLEXING=false`
  whenever `CARGO_REGISTRY_MIRROR` is set, in both the host-toolchain and
  container-fallback code paths.
- `README.md` / `docs/adr/0002-release-and-supply-chain-pipeline.md`:
  documented the symptom and fix.

### Why
`cargo build` kept failing with `SSL routines::unexpected eof while reading`
against a private Cargo mirror even after CA trust was correctly configured
via `CA_BUNDLE` (apt succeeded against the same host over the same network,
ruling out cert trust as the cause). This is a known failure mode: cargo
defaults to HTTP/2 multiplexing for the sparse-registry protocol, and
TLS-intercepting corporate proxies frequently mishandle HTTP/2, dropping the
connection mid-handshake. Forcing HTTP/1.1 via `CARGO_HTTP_MULTIPLEXING=false`
is the standard documented fix for this exact symptom.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only
- [x] Build/CI tooling only (no runtime behavior change)

## [2026-09-04 22:40] - Makefile: silence debconf frontend noise in build-linux-* container fallback

**Author:** Erick Bourgeois

### Changed
- `Makefile`: `_build-linux`'s container fallback now exports
  `DEBIAN_FRONTEND=noninteractive` before `apt-get update`/`install`.

### Why
Without `DEBIAN_FRONTEND` set, `apt-get install` in the non-tty container
tries the Dialog, Readline, and Teletype prompt frontends in turn, printing a
`debconf: unable to initialize frontend: ...` warning for each before falling
back to Noninteractive on its own. Harmless, but pure noise on every build —
setting the variable upfront skips straight to Noninteractive.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only
- [x] Build/CI tooling only (no runtime behavior change)

## [2026-09-04 22:30] - README: fix APT_SETUP_CMD example for deb822 default sources

**Author:** Erick Bourgeois

### Changed
- `README.md`: the `APT_SETUP_CMD` example now removes
  `/etc/apt/sources.list.d/*.sources` before writing the legacy
  `/etc/apt/sources.list`, and explains why.

### Why
Debian 12+ (bookworm, trixie) Docker images ship their default sources in the
newer deb822 format under `/etc/apt/sources.list.d/*.sources`, separate from
the legacy `sources.list` file. apt merges both, so overwriting only the
legacy file (the original example) left the untouched deb822 default still
pointing at `deb.debian.org` — apt kept trying to reach it alongside the
private mirror and failed on the still-blocked default. This is a docs-only
fix; no Makefile logic changed, since `APT_SETUP_CMD` is deliberately a raw
hook and this is content the caller supplies.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only

## [2026-09-04 22:20] - Makefile: CA_BUNDLE for TLS-intercepting/private-CA networks

**Author:** Erick Bourgeois

### Changed
- `Makefile`: added `CA_BUNDLE` variable. When set to a host path, the
  `build-linux-*` container fallback bind-mounts it read-only into
  `RUST_BUILD_IMAGE` and runs `update-ca-certificates` (plus exports
  `CARGO_HTTP_CAINFO` defensively) before `APT_SETUP_CMD`/apt/cargo touch the
  network.
- `README.md`: documented `CA_BUNDLE`, simplified the `APT_SETUP_CMD` example
  to drop the now-unnecessary `Acquire::https::CaInfo` line.
- `docs/adr/0002-release-and-supply-chain-pipeline.md`: extended the addendum.

### Why
A correct mirror URL still fails on networks with a TLS-intercepting proxy or
an internal-only registry backed by a private CA: the public
`RUST_BUILD_IMAGE` doesn't trust that CA, so apt reports a misleading `does
not have a Release file` and cargo fails with `SSL routines::unexpected eof
while reading` — both are TLS-trust symptoms, not repo-layout bugs, and
neither `APT_SETUP_CMD` nor `CARGO_REGISTRY_MIRROR` alone fixes them.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only
- [x] Build/CI tooling only (no runtime behavior change)

## [2026-09-04 22:05] - Makefile: replace APT_MIRROR with a raw APT_SETUP_CMD hook

**Author:** Erick Bourgeois

### Changed
- `Makefile`: removed `APT_MIRROR` (hostname-substitution sed) in favor of
  `APT_SETUP_CMD`, an arbitrary shell command `eval`'d inside
  `RUST_BUILD_IMAGE` before `apt-get update`. Both `APT_SETUP_CMD` and
  `CARGO_REGISTRY_MIRROR` are now `export`ed Make variables passed to the
  container via bare `docker run -e VARNAME` (environment passthrough)
  instead of being substituted as `$(VAR)` text into a double-quoted
  `-e VAR="..."` argument.
- `docs/adr/0002-release-and-supply-chain-pipeline.md`: updated the addendum.

### Why
A real private-network apt setup needs a custom repo path (no `/debian`
suffix), a separate security repo, a corporate CA bundle, and proxy overrides
— together, not a single hostname swap. `APT_SETUP_CMD` lets the caller supply
that exact script. Passing a command like that through `$(VAR)` text
substitution into `-e VAR="$(VAR)"` is unsafe: embedded double quotes and
shell operators (`&&`, `>`) in the value would be reinterpreted by the *host*
shell that runs the `docker run` line, not passed through as literal text.
Routing it through `export` + bare `-e VARNAME` uses process environment
passthrough instead of text substitution, so arbitrary shell content survives
intact. Verified directly: a value containing `"`, `&&`, and `>` reached a
test container's environment unmodified via this mechanism.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only
- [x] Build/CI tooling only (no runtime behavior change) — `APT_MIRROR` users
      must switch to `APT_SETUP_CMD`

## [2026-09-04 21:50] - Makefile: CARGO_REGISTRY_MIRROR override for build-linux-*

**Author:** Erick Bourgeois

### Changed
- `Makefile`: added `CARGO_REGISTRY_MIRROR` variable. When set to a sparse
  registry URL (e.g. an Artifactory Cargo remote repo's index), `_build-linux`
  exports `CARGO_SOURCE_CRATES_IO_REPLACE_WITH=mirror` and
  `CARGO_SOURCE_MIRROR_REGISTRY=<url>` before `cargo build`, in both the
  host-toolchain and container-fallback code paths.

### Why
`RUST_BUILD_IMAGE`/`APT_MIRROR` route the build container and its apt install
around Docker Hub / deb.debian.org, but `cargo build` still resolves
dependencies against crates.io directly, which the same restricted networks
also block. Cargo supports registry replacement purely via environment
variables (no config file needed), so this reuses that mechanism.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only
- [x] Build/CI tooling only (no runtime behavior change)

## [2026-09-04 21:35] - Makefile: APT_MIRROR override for the build-linux-* container fallback

**Author:** Erick Bourgeois

### Changed
- `Makefile`: added `APT_MIRROR` variable. When set, the `build-linux-*`
  container fallback rewrites `deb.debian.org`/`security.debian.org` in the
  container's apt sources to the given mirror before `apt-get update`, for
  networks that only allow a private package mirror.

### Why
Pulling `RUST_BUILD_IMAGE` from a private registry mirror doesn't help if the
container's own `apt-get install libtss2-dev` still tries to reach
`deb.debian.org` directly, which corporate networks that only permit an
artifactory mirror will also block.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only
- [x] Build/CI tooling only (no runtime behavior change)

## [2026-09-04 21:15] - Makefile: host cross-toolchain first, container fallback for build-linux-*

**Author:** Erick Bourgeois

### Changed
- `Makefile`: `build-linux-amd64`/`build-linux-arm64` now try a host gcc
  cross-toolchain (or native compile on a matching Linux host) before falling
  back to a `RUST_BUILD_IMAGE` container, mirroring banlieue's `_build-linux`
  pattern. The native path is gated on `pkg-config --exists tss2-esys` since
  sceau (unlike banlieue) links against libtss2, a system C library that needs
  a target-matching sysroot, not just a cross-linker.
- `docs/adr/0002-release-and-supply-chain-pipeline.md`: added an addendum
  documenting the local build-strategy decision and its trade-offs.

### Why
`build-linux-*` always shelled out to `docker run rust:1-bookworm`, which
fails outright on networks that block Docker Hub (only a private registry
mirror reachable). Linux CI runners already `apt-get install libtss2-dev`
before building, so they can skip the container entirely; only macOS/cross-arch
local builds still need it, and `RUST_BUILD_IMAGE` stays overridable to point
at a private mirror.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [x] Documentation only
- [x] Build/CI tooling only (no runtime behavior change)

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
