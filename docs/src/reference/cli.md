# CLI Reference

sceau is a single binary with two flags, defined in
[`src/main.rs`](https://github.com/firestoned/sceau/blob/main/src/main.rs).
Every example in this documentation uses only these flags.

```text
Kubernetes KMS v2 plugin that seals data encryption keys with a TPM 2.0.

Usage: sceau [OPTIONS]

Options:
      --socket <SOCKET>  Unix socket kube-apiserver connects to
                         [default: /run/sceau/sceau.sock]
      --tcti <TCTI>      TCTI configuration string for the TPM
                         [default: device:/dev/tpmrm0]
  -h, --help             Print help
  -V, --version          Print version
```

## `--socket`

Path of the unix socket the KMS v2 gRPC server binds. Behaviour:

- The parent directory is created if missing.
- A stale socket file at the path is removed before binding.
- The socket is created with mode **`0600`** — only the owner (root, in the
  reference deployment) can connect. This is the access-control boundary; see
  the [threat model](../concepts/threat-model.md).
- The socket file is removed again on graceful shutdown.

The matching apiserver-side value is the `endpoint:` in the
`EncryptionConfiguration` — `unix:///run/sceau/sceau.sock` for the default
(see [k0s Setup](../guides/k0s-setup.md)).

## `--tcti`

TCTI (TPM Command Transmission Interface) configuration string, parsed by
`tss-esapi`'s `TctiNameConf`. Common values:

| TCTI string | Use |
| --- | --- |
| `device:/dev/tpmrm0` | **Default.** Kernel TPM resource manager — the right choice on real hardware. |
| `device:/dev/tpm0` | Raw TPM device, bypassing the resource manager. Rarely what you want. |
| `swtpm:host=127.0.0.1,port=2321` | Local [swtpm](https://github.com/stefanberger/swtpm) simulator — the dev loop (see [Quickstart](../guides/quickstart.md)). |
| `swtpm:host=bar.foo.io,port=2321` | Remote simulator/TPM over TCP. |

An unparseable TCTI string fails fast at startup (`invalid TCTI
configuration`); a TCTI that parses but cannot connect fails at SRK creation
with a TSS error.

## Environment

| Variable | Effect |
| --- | --- |
| `RUST_LOG` | Standard `tracing-subscriber` env filter, e.g. `RUST_LOG=debug`. Defaults to `info`. |

There are no other environment variables and no configuration file — the two
flags are the whole interface, by design.
