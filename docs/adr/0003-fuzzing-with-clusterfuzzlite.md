<!--
Copyright (c) 2026 Erick Bourgeois, sceau
SPDX-License-Identifier: Apache-2.0
-->
# 0003 — Fuzzing with ClusterFuzzLite and cargo-fuzz

- **Status:** Accepted
- **Date:** 2026-09-07
- **Deciders:** Erick Bourgeois
- **Related:** ADR-0001 (KMS wire contract — the envelope being fuzzed); OpenSSF Scorecard `Fuzzing` check (code-scanning alert #6); `rules/testing.md`.

## Context

OpenSSF Scorecard flags sceau with `Fuzzing` score 0 ("project is not
fuzzed"). For a Rust repository the check recognises exactly two remediations:
listing in OSS-Fuzz (an external application process, unrealistic for a
pre-1.0 single-maintainer plugin) or a ClusterFuzzLite deployment in the repo.
Meanwhile the code has exactly one parser of attacker-influenced bytes that is
not yet exercised by property testing: the KMS ciphertext envelope decoder
(`version || public_len || public || private` in `src/tpm.rs`), whose input
arrives from kube-apiserver over the gRPC socket, and the prost-generated KMS
v2 request decoders.

## Decision

**Fuzz the untrusted-input parsers with cargo-fuzz (libFuzzer), driven in CI
by ClusterFuzzLite.**

1. **Targets** — `fuzz/fuzz_targets/`:
   - `envelope_decode.rs` — arbitrary bytes into the envelope parser
     (`sceau::tpm::envelope_decode`), which must never panic and must reject
     malformed input with `TpmError::MalformedEnvelope`;
   - `kms_proto_decode.rs` — arbitrary bytes into the prost decoders for
     `EncryptRequest` / `DecryptRequest` / `StatusRequest`.
2. **Crate layout** — the binary gains a thin library (`src/lib.rs`) so the
   fuzz crate can link the parsers; `main.rs` becomes a shim over it. The
   `fuzz/` crate is a standalone cargo-fuzz workspace member and is never
   built by the normal `make build` / `make test`.
3. **CI** — `.clusterfuzzlite/` (project.yaml + Dockerfile + build.sh) plus a
   `fuzz.yaml` workflow running ClusterFuzzLite `code-change` mode on PRs that
   touch Rust sources: build the fuzzers in the
   `gcr.io/oss-fuzz-base/base-builder-rust:ubuntu-24-04` image (digest-pinned;
   the default focal variant ships libtss2 2.3.2, too old for tss-esapi-sys)
   and run them for a bounded time. Batch/corpus-pruning modes that need a
   storage repo are deferred until the corpus is worth keeping.
4. **TPM-backed code is out of scope** for fuzzing (needs a resource manager);
   only pure byte parsers are targeted, per `rules/testing.md`.

## Consequences

- Scorecard `Fuzzing` detects the ClusterFuzzLite deployment and the alert
  clears on the next scheduled scan.
- `envelope_decode` / `envelope_encode` become `pub` (the fuzz crate is a
  separate crate) — acceptable: they are pure parsers with no TPM state, and
  `src/tpm_tests.rs` now pins the wire contract (TDD per `rules/testing.md`).
- Fuzz builds need `libtss2-dev` + `protobuf-compiler` inside the builder
  image; installed in `.clusterfuzzlite/build.sh`.
- No CALM model change: this is CI/test infrastructure, not a runtime node,
  relationship, or control in the system architecture.
