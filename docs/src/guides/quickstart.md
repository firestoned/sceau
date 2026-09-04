# Quickstart

Build sceau and exercise seal/unseal against a software TPM
([swtpm](https://github.com/stefanberger/swtpm)) — no hardware TPM and no
Kubernetes cluster required. For deploying to a real Kairos/k0s host, see
[k0s Setup](k0s-setup.md) and [Kairos Deployment](kairos-deployment.md).

## Prerequisites

- Rust toolchain (**1.88+**).
- The TPM2 TSS development libraries and `protoc`:

    === "Debian / Ubuntu"

        ```sh
        sudo apt-get update && sudo apt-get install -y libtss2-dev protobuf-compiler
        ```

    === "Fedora"

        ```sh
        sudo dnf install -y tpm2-tss-devel protobuf-compiler
        ```

    === "macOS (Homebrew)"

        ```sh
        brew install tpm2-tss protobuf
        ```

- `swtpm` for the simulator (`apt-get install swtpm swtpm-tools`, `dnf
  install swtpm`, or `brew install swtpm`).

```sh
git clone https://github.com/firestoned/sceau
cd sceau
```

## Build

```sh
make build        # = cargo build --release
```

The result is a single binary at `target/release/sceau`.

## Start a software TPM

swtpm exposes a TPM 2.0 over a TCP socket pair (command channel on `2321`,
control channel on `2322`):

```sh
mkdir -p /tmp/swtpm-state
swtpm socket --tpm2 \
  --tpmstate dir=/tmp/swtpm-state \
  --server port=2321 \
  --ctrl type=tcp,port=2322 \
  --flags not-need-init
```

Leave it running in its own terminal. To reset the simulated TPM to a clean
state later, delete `/tmp/swtpm-state` and restart swtpm — remember that a
fresh TPM state means a fresh seed, so anything sealed before the reset is
gone (see the [threat model](../concepts/threat-model.md)).

## Run sceau against it

Point the TCTI at the simulator instead of the default `device:/dev/tpmrm0`:

```sh
./target/release/sceau \
  --socket /tmp/sceau.sock \
  --tcti "swtpm:host=127.0.0.1,port=2321"
```

You should see the SRK being recreated and the stable key identity reported:

```text
INFO TPM primary key ready key_id="sceau-9f2c41a7b3e80d1c" tcti="swtpm:host=127.0.0.1,port=2321"
INFO serving KMS v2 socket="/tmp/sceau.sock"
```

!!! tip "Same simulator, same key_id"
    Restart sceau against the *same* swtpm state directory and you get the
    same `key_id` — the deterministic SRK recreation working as designed.
    Wipe the state directory and the `key_id` changes.

## Exercise the KMS API

Any gRPC client that speaks the KMS v2 proto works. With
[`grpcurl`](https://github.com/fullstorydev/grpcurl):

```sh
# Status — the apiserver's health/key check
grpcurl -plaintext -unix /tmp/sceau.sock \
  -import-path proto -proto kms/v2/api.proto \
  v2.KeyManagementService/Status
```

```json
{
  "version": "v2",
  "healthz": "ok",
  "keyId": "sceau-9f2c41a7b3e80d1c"
}
```

```sh
# Encrypt — seal a 32-byte DEK (base64 in JSON)
DEK_B64=$(head -c 32 /dev/urandom | base64)
grpcurl -plaintext -unix /tmp/sceau.sock \
  -import-path proto -proto kms/v2/api.proto \
  -d "{\"plaintext\": \"$DEK_B64\", \"uid\": \"demo-1\"}" \
  v2.KeyManagementService/Encrypt
```

Take the `ciphertext` and `keyId` from the response and decrypt:

```sh
grpcurl -plaintext -unix /tmp/sceau.sock \
  -import-path proto -proto kms/v2/api.proto \
  -d "{\"ciphertext\": \"<ciphertext>\", \"uid\": \"demo-1\", \"keyId\": \"<keyId>\"}" \
  v2.KeyManagementService/Decrypt
```

The returned `plaintext` matches the original DEK. Decrypting with a wrong
`keyId` fails with `INVALID_ARGUMENT` — the key-identity check described in
[KMS v2 Protocol](../concepts/kms-v2.md).

## Run the test suite

```sh
make test         # cargo test --all-features
make lint         # cargo fmt --check + clippy -D warnings
```

Unit tests cover the envelope codec and the KMS service logic; TPM-dependent
paths run against whatever TCTI is configured. End-to-end tests against a
swtpm harness are on the roadmap (see [Status](../index.md#project-status)).

## Next steps

- [k0s Setup](k0s-setup.md) — point a real apiserver at sceau.
- [TPM Sealing](../concepts/tpm-sealing.md) — what just happened inside the
  TPM.
- [Local Development](../developer/local-development.md) — the full `make`
  surface and the ADD workflow.
