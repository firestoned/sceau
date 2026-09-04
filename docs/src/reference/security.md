# Security

This page mirrors the repository's
[`SECURITY.md`](https://github.com/firestoned/sceau/blob/main/SECURITY.md).

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

- the affected component (KMS service, TPM sealing, deployment unit) and
  version/commit;
- steps to reproduce or a proof of concept;
- the impact you believe it has.

You can expect an acknowledgement within **3 business days** and a triage
decision (accepted / needs-more-info / not-a-vulnerability) within **7**.
Accepted reports get a fix or mitigation plan, and credit in the release
notes unless you ask otherwise.

## Scope Notes

- The intended deployment trust model is documented in the
  [Threat Model](../concepts/threat-model.md) and
  [ADR-0001](https://github.com/firestoned/sceau/blob/main/docs/adr/0001-tpm-sealed-kms-v2-plugin.md)
  — issues that require already-root access on the host are generally out of
  scope (the KMS socket is mode `0600` precisely because root is the trust
  boundary).
- Supply-chain controls (SBOM, Cosign signing, Trivy scanning) are described
  in [ADR-0002](https://github.com/firestoned/sceau/blob/main/docs/adr/0002-release-and-supply-chain-pipeline.md);
  the dependency policy lives in
  [`deny.toml`](https://github.com/firestoned/sceau/blob/main/deny.toml).
