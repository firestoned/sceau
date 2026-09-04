# Concepts

The three ideas sceau is built from, each with its own page:

<div class="grid cards" markdown>

- :material-api: **[KMS v2 Protocol](kms-v2.md)**

    The gRPC contract between kube-apiserver and sceau: `Status`, `Encrypt`,
    `Decrypt`, the DEK lifecycle, and how `key_id` drives rotation and
    migration.

- :material-chip: **[TPM Sealing](tpm-sealing.md)**

    The TPM 2.0 object model: the deterministic RSA-2048 SRK primary,
    `fixedTpm` + `fixedParent` sealed-data objects, and the envelope byte
    format that becomes the KMS ciphertext.

- :material-shield-alert: **[Threat Model](threat-model.md)**

    What TPM sealing actually protects against, the explicit "TPM loss = data
    loss" trade-off, and the PCR-binding roadmap.

</div>

If you want the 10,000-foot view of how these compose, read the
[Overview](../overview.md) first. For the formal architecture model, see
[Architecture](../architecture/index.md).
