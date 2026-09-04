# Guides

Task-oriented, step-by-step guides for running sceau — from a first local
build against a TPM simulator to a production Kairos host.

<div class="grid cards" markdown>

- :material-rocket-launch: **[Quickstart](quickstart.md)**

    Build sceau, start a local [swtpm](https://github.com/stefanberger/swtpm)
    simulator, and seal/unseal your first DEK — no hardware TPM required.

- :material-kubernetes: **[k0s Setup](k0s-setup.md)**

    Wire kube-apiserver to sceau with an `EncryptionConfiguration`, then run
    the migration procedure that re-encrypts existing Secrets.

- :material-harddisk: **[Kairos Deployment](kairos-deployment.md)**

    Run sceau as a systemd unit on a Kairos host, and bundle the binary into
    a custom Kairos image.

- :material-docker: **[Internal Registry](internal-registry.md)**

    Build and push the distroless image to an internal registry mirror with a
    single `make docker-image` invocation — including the Docker Hub gotcha
    and the `IMAGE_REF` escape hatch.

</div>

!!! info "Looking to hack on sceau itself?"
    Building from source, the `make` targets, and the ADD workflow live under
    **[Developer → Local Development](../developer/local-development.md)**.

## Conventions used in these guides

- The released image is **`ghcr.io/firestoned/sceau`**, Cosign-signed; see
  the [threat model](../concepts/threat-model.md#supply-chain) for
  verification.
- Placeholder hostnames (`bar.foo.io`, `k0s-node1.example.com`) and RFC 5737
  IPs are used throughout — substitute your own values.
- The KMS socket is **`/run/sceau/sceau.sock`** (mode `0600`) and the default
  TCTI is **`device:/dev/tpmrm0`**; both are overridable via
  [CLI flags](../reference/cli.md).
