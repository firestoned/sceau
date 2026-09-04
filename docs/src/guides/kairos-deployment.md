# Kairos Deployment

sceau targets [Kairos](https://kairos.io) hosts running k0s. Kairos's
immutable, image-based model fits sceau well: the plugin is a single static
binary plus the TPM TSS runtime libraries, and its statelessness (the SRK is
recreated deterministically at every boot) means **Kairos A/B upgrades are
transparent** — the new image boots, sceau recreates the same SRK, and every
previously sealed DEK unseals.

## The systemd unit

The reference unit — also shown in the README — runs sceau before k0s and
keeps it running:

```ini
[Unit]
Description=sceau KMS plugin (TPM)
Before=k0scontroller.service

[Service]
RuntimeDirectory=sceau
RuntimeDirectoryMode=0700
ExecStart=/usr/local/bin/sceau --socket /run/sceau/sceau.sock
Restart=always
RestartSec=2

[Install]
WantedBy=multi-user.target
```

Notes:

- **`Before=k0scontroller.service`** — the apiserver's KMS health check runs
  at startup; sceau must already be listening or the apiserver logs KMS
  errors until it retries.
- **`RuntimeDirectory=sceau`** — systemd creates `/run/sceau` (mode `0700`)
  for the socket. sceau also creates the parent directory itself, so the unit
  works without this, but the declarative form is preferred.
- **No `--tcti` flag** — the default `device:/dev/tpmrm0` is the right choice
  on real hardware (the kernel resource manager). Override it only for
  simulator-based testing.
- A restarted sceau unseals everything its predecessor sealed — the SRK is
  deterministic, so `Restart=always` is safe, not stateful.

Install and enable:

```sh
install -m 0755 sceau /usr/local/bin/sceau
install -m 0644 sceau.service /etc/systemd/system/sceau.service
systemctl daemon-reload
systemctl enable --now sceau
```

## Verifying on the host

```sh
systemctl status sceau
journalctl -u sceau -f
ls -l /run/sceau/sceau.sock     # srw------- root root
```

The startup log line reports the TPM-derived identity:

```text
INFO TPM primary key ready key_id="sceau-9f2c41a7b3e80d1c" tcti="device:/dev/tpmrm0"
```

Record this `key_id` per host. It is stable across reboots and image
upgrades; if it ever *changes* on the same hardware, the TPM was cleared or
the board was replaced — and everything sealed under the old identity is
gone (see the [threat model](../concepts/threat-model.md)).

## Bundling into a Kairos image

The Kairos-native install path is baking sceau into the OS image itself, so
every host provisioned from the image comes up with the KMS plugin present
and enabled. Two inputs are needed in the image:

1. the binary at `/usr/local/bin/sceau` (with the TSS runtime libraries —
   `libtss2-esys`, `libtss2-sys`, `libtss2-mu`, `libtss2-tctildr`, and the
   TCTI device module — on the library path), and
2. the enabled systemd unit above.

A Kairos Dockerfile stage looks like:

```dockerfile
FROM ghcr.io/firestoned/sceau:v0.1.0 AS sceau

FROM quay.io/kairos/kairos-init AS kairos-init

# ... your base image / kairos-init stages ...

# sceau binary + staged TSS runtime libraries
COPY --from=sceau /usr/local/bin/sceau /usr/local/bin/sceau
COPY --from=sceau /usr/lib/ /usr/lib/
COPY deploy/kairos/sceau.service /etc/systemd/system/sceau.service
RUN systemctl enable sceau.service
```

The released `ghcr.io/firestoned/sceau` image is distroless but contains
exactly the binary and the staged TSS libraries (see the repo `Dockerfile`),
which makes it a convenient `COPY --from` source. Verify the image
signature before pinning it into your build — see
[supply chain](../concepts/threat-model.md#supply-chain).

!!! note "Cloud-init alternative"
    If you prefer not to build a custom image, a Kairos cloud-config stage
    can fetch the release tarball, drop the unit, and `systemctl enable
    --now sceau`. Image bundling is recommended: the artifact is verified
    once at build time instead of downloaded on every host at provision
    time.

## What survives what

| Event | Sealed DEKs survive? |
| --- | --- |
| Reboot | ✅ — SRK recreated deterministically |
| Kairos A/B upgrade / OS reinstall | ✅ — same TPM, same template, same primary |
| sceau daemon restart | ✅ — stateless by design |
| `tpm2_clear`, TPM failure, motherboard swap | ❌ — permanent data loss; restore from etcd snapshots (see the [threat model](../concepts/threat-model.md)) |

## Next steps

- [k0s Setup](k0s-setup.md) — point the apiserver at the socket.
- [Internal Registry](internal-registry.md) — mirror the image through your
  own registry for air-gapped or bandwidth-constrained sites.
