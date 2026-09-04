# Architecture

sceau's architecture is maintained **as code** using the
[FINOS Common Architecture Language Model (CALM)](https://calm.finos.org/),
per the project's Architecture Driven Development (ADD) methodology:

```text
ADR → CALM → TDD → implement → docs
```

Architecturally significant changes are decided in an
[ADR](https://github.com/firestoned/sceau/tree/main/docs/adr) first, then
modelled in the CALM document, and only then implemented. `make
calm-validate` is a hard CI gate: code that isn't reflected in the model
isn't considered designed.

## The CALM model

The single architecture document lives at
[`docs/architecture/calm/architecture.json`](https://github.com/firestoned/sceau/blob/main/docs/architecture/calm/architecture.json)
(CALM schema 1.2). It models:

- **Actor** — the cluster operator who deploys sceau and wires the k0s
  apiserver's `EncryptionConfiguration` to it.
- **Ecosystem** — one Kairos OS host running the k0s control plane;
  everything sceau touches is on this host.
- **Services** — the k0s kube-apiserver and the sceau KMS v2 plugin,
  connected over a mode-`0600` unix socket.
- **Database** — etcd, which persists only KMS envelopes (sealed DEKs +
  AES-GCM ciphertext), never plaintext.
- **System** — the TPM 2.0, reached via `/dev/tpmrm0`, holding the
  deterministic SRK primary under which every DEK is sealed.
- **Data assets** — the sealed DEK envelope (the ciphertext format) and the
  apiserver `EncryptionConfiguration`.
- **Flows** — *startup SRK recreation*, *encrypt on write*, *decrypt on
  read*.
- **Controls** — TPM root of trust (`fixedTpm` + `fixedParent`), no
  persistent plaintext, and the supply-chain pipeline, each linked to NIST
  SP 800-53 Rev. 5 / SP 800-218 (SSDF) and to in-repo evidence files.

## Generated diagrams

The Mermaid diagrams in this section are rendered from the CALM model by
`make calm-diagrams` — **do not edit them by hand**:

- [System Diagram](system.md) — every node and relationship in one flowchart.
- [Architecture Flows](flows.md) — one flowchart per modelled flow.

To change a diagram, edit `architecture.json` or the Handlebars templates in
`docs/architecture/calm/templates/mermaid/`, then regenerate:

```sh
make calm-validate   # hard gate: architecture conforms to the meta-schema
make calm-diagrams   # re-render the pages in this section
```

## Decision records

| ADR | Decision |
| --- | --- |
| [0001](https://github.com/firestoned/sceau/blob/main/docs/adr/0001-tpm-sealed-kms-v2-plugin.md) | TPM-sealed KMS v2 plugin — deterministic SRK, sealed DEKs, `key_id` from the SRK name, k0s host-socket integration. |
| [0002](https://github.com/firestoned/sceau/blob/main/docs/adr/0002-release-and-supply-chain-pipeline.md) | Release and supply-chain pipeline — distroless image, SBOM, Cosign, Trivy, Makefile-driven workflows. |
