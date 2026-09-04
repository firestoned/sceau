<!--
Copyright (c) 2026 Erick Bourgeois, sceau
SPDX-License-Identifier: Apache-2.0
-->
# Security Policy

## Supported Versions

sceau is pre-1.0 and under active development. Only the latest commit on
`main` and the most recent release receive security fixes.

| Version | Supported |
| ------- | --------- |
| latest release / `main` | ✅ |
| anything older | ❌ |

## Reporting a Vulnerability

**Do not open a public issue for a suspected vulnerability.**

Report it privately via GitHub's
[private vulnerability reporting](https://github.com/firestoned/sceau/security/advisories/new)
("Report a vulnerability" on the repository's Security tab).

Include, where possible:

- the affected component (KMS service, TPM sealing, deployment unit) and version/commit;
- steps to reproduce or a proof of concept;
- the impact you believe it has.

You can expect an acknowledgement within **3 business days** and a triage
decision (accepted / needs-more-info / not-a-vulnerability) within **7**.
Accepted reports get a fix or mitigation plan, and credit in the release notes
unless you ask otherwise.

## Scope Notes

- The intended deployment trust model is documented in `README.md` and
  [ADR-0001](docs/adr/0001-tpm-sealed-kms-v2-plugin.md) — issues that require
  already-root access on the host are generally out of scope (the KMS socket
  is mode 0600 precisely because root is the trust boundary).
- Supply-chain controls (SBOM, Cosign signing, Trivy scanning) are described
  in [ADR-0002](docs/adr/0002-release-and-supply-chain-pipeline.md); the
  dependency policy lives in `deny.toml`.
