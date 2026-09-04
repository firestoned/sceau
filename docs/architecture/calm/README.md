# sceau CALM Architecture

This folder contains the [FINOS Common Architecture Language Model
(CALM)](https://calm.finos.org/) description of sceau.

| File | Purpose |
| --- | --- |
| `architecture.json` | Single architecture document: nodes, relationships, flows, controls, metadata. Targets CALM schema **1.2**. |
| `templates/mermaid/system.md.hbs` | Handlebars template that renders every node and relationship as a single Mermaid `flowchart LR`. Output → `docs/src/architecture/system.md` (MkDocs site). |
| `templates/mermaid/flows.md.hbs` | Handlebars template that renders each `flows[]` entry as its own Mermaid `flowchart TD`. Output → `docs/src/architecture/flows.md` (MkDocs site). |

## What it models

- **Actor** — the cluster operator who deploys sceau and wires the k0s
  apiserver's `EncryptionConfiguration` to it.
- **Ecosystem** — a single Kairos OS host running the k0s control plane.
  Everything sceau touches is on this one host.
- **Services** — the k0s kube-apiserver and the `sceau` KMS v2 plugin,
  connected over a mode-0600 unix socket.
- **Database** — etcd, which persists only KMS envelopes (sealed DEKs +
  AES-GCM ciphertext), never plaintext.
- **System** — the TPM 2.0, reached via `/dev/tpmrm0`, holding the
  deterministic SRK primary under which every DEK is sealed.
- **Data assets** — the sealed DEK envelope (the ciphertext format) and the
  apiserver `EncryptionConfiguration`.
- **Flows** — *startup SRK recreation*, *encrypt on write*, *decrypt on read*.
- **Controls** — TPM root of trust (fixedTpm+fixedParent), no persistent
  plaintext, and the supply-chain pipeline. Each links to NIST SP 800-53
  Rev. 5 / SP 800-218 (SSDF) and to in-repo evidence files.

## Validating

```bash
make calm-validate   # hard gate: architecture conforms to the meta-schema
make calm-diagrams   # render docs/src/architecture/{system,flows}.md (MkDocs site)
```
