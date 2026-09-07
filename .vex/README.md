<!--
Copyright (c) 2026 Erick Bourgeois, sceau
SPDX-License-Identifier: Apache-2.0
-->
# `.vex/` — OpenVEX statements

This directory holds **curated, hand-authored** [OpenVEX](https://openvex.dev/)
statements that suppress (or annotate) container CVEs with a documented
justification. The release pipeline merges every `.vex/*.json` here — together
with the machine-derived document from `auto-vex-presence` — into one OpenVEX
document, attests it to the image digest with Cosign, and feeds it to the Grype
scan (`grype --vex`) so justified findings do not re-alarm. See
[ADR-0006](../docs/adr/0006-vex-slsa-and-attestation-parity.md).

## Files

- **`*.json`** — one curated OpenVEX document per advisory. Merged by
  `vexctl merge` in CI (the `build-vex` job) and locally via `make vex-assemble`.
  Validate with `make vex-validate`.
- **`.gitkeep`** — keeps the directory tracked when no curated statements exist.

When there are no curated statements, CI emits a valid **empty** OpenVEX
document, so the pipeline works from day one.

> Dot-prefixed `*.json` files are treated as sidecar config, not statements —
> both the shell glob and `load_triaged_from_vex_dir` skip them. banlieue uses
> that slot for `.affected-functions.json` (the curated CVE → symbol map for
> its `auto-vex-reachability` tool). sceau does not port that tool yet; see
> ADR-0006 for why.

## What is automated vs. what you write by hand

`auto-vex-presence` (`crates/sceau-vex`) already emits
`not_affected + component_not_present` for any finding whose package URL is in
**no** image SBOM. Do not hand-write those — they are derived on every run.

Hand-write a statement when the component *is* present but the vulnerability
still does not apply. For sceau that is mostly the TPM TSS shared libraries and
their glibc dependencies, which `make build-linux-*` stages into the image
rootfs: those packages are genuinely in the SBOM, so only a human can say
whether the vulnerable path is reachable from the plugin.

## Statement shape

```json
{
  "@context": "https://openvex.dev/ns/v0.2.0",
  "@id": "https://sceau/vex/CVE-XXXX-NNNN",
  "author": "Erick Bourgeois",
  "timestamp": "2026-01-01T00:00:00Z",
  "version": 1,
  "statements": [
    {
      "vulnerability": { "name": "CVE-XXXX-NNNN" },
      "products": [{ "@id": "pkg:oci/sceau" }],
      "status": "not_affected",
      "justification": "vulnerable_code_not_in_execute_path",
      "impact_statement": "Why sceau is not affected.",
      "timestamp": "2026-01-01T00:00:00Z"
    }
  ]
}
```

The document-level `@id` / `author` / `timestamp` are replaced by CI at merge
time; statement-level fields ship as-authored.

- **`status`**: `not_affected` | `affected` | `fixed` | `under_investigation`.
- **`justification`** (required when `not_affected`): `component_not_present`,
  `vulnerable_code_not_present`, `vulnerable_code_not_in_execute_path`,
  `vulnerable_code_cannot_be_controlled_by_adversary`,
  `inline_mitigations_already_exist`.
- **`products[].@id`**: the product purl — `pkg:oci/sceau` (`PRODUCT_PURL` in
  the Makefile).
- Accepted vulnerability id shapes: `CVE-…`, `GHSA-…`, `RUSTSEC-…`.

## Adding a suppression

1. Create `.vex/<ADVISORY>.json` following the shape above with an honest
   `justification` and `impact_statement`.
2. Run `make vex-validate` to confirm it parses and merges.
3. On merge to `main` / release, CI assembles, attests, and applies it.

An `impact_statement` that only restates the justification is not worth
writing — say *which* code path is absent and what would have to change for the
statement to stop being true (e.g. "drop this once the base image ships
zlib 1.3.2+").
