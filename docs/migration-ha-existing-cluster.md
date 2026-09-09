<!-- Copyright (c) 2026 Erick Bourgeois, sceau -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Migrating an existing HA k0s cluster onto `sceau` (ADR-0003)

**Status:** Runbook for the design in ADR-0003. `genesis`/`enroll`/`join`
are implemented and live-verified end to end, including the actual
cross-node `Secret` decrypt this design exists for (ADR-0003 §8a-equivalent
proof, `.claude/CHANGELOG.md`, 2026-09-06). Phase 5 (steady-state fleet-key
preference, described in Phase A/B below) is implemented too.

**2026-09-07 update:** the first *genuine* `enroll`/`join` round-trip run
against two freshly-enrolling nodes (as opposed to whatever put the
2026-09-06 shared key in place) surfaced a real bug: `join` failed with
`TPM_RC_NV_DEFINED` on a node that already held any object at the fleet
key's persistent handle (e.g. re-running `join` after a prior enrollment
attempt). Fixed — `join` now always replaces whatever's there, since
that's the only thing running `join` ever means. **§2b below is safe to
retry** as a result: re-running `join` on a node that already attempted it
(successfully or not) now succeeds rather than erroring. See
`.claude/CHANGELOG.md` and ADR-0003's Consequences for detail.

**2026-09-07 update #2:** live-testing Decision 3's rejection path (a
real, otherwise-valid node whose `Node` object was temporarily absent)
surfaced a second bug: a rejected `enroll` request permanently consumed
one of `--max`'s permits, so with `--max=1` a single failed/rejected
attempt could lock out the next legitimate joiner until the operator
restarted `enroll`. Fixed — only a genuinely-served enrollment consumes
a permit now. See `.claude/CHANGELOG.md` and ADR-0003's Consequences.

**2026-09-07 update #3:** `sceau status` now exists (§2c). Building it
surfaced a third bug — `TpmSealer`'s `Drop` impl issued an invalid
`TPM2_FlushContext` against the fleet key's already-persistent handle,
rejected by real hardware (harmless in effect, since the error was
already discarded, but noisy). This was already happening silently on
every `sceau serve` shutdown; fixed now. See `.claude/CHANGELOG.md`.

**Audience:** operators migrating a real, already-running HA
[k0s](https://k0sproject.io) (the single-binary Kubernetes distribution
this project targets) cluster (multiple controller+worker nodes,
unencrypted etcd today) onto TPM-sealed Secrets encryption, one node at a
time, without an outage.

## 1. `genesis` and `enroll` never touch the running cluster — say this explicitly, because it's easy to assume otherwise

Every command in this guide's **Phase A** only ever talks to a node's own
local TPM (`/dev/tpmrm0`) and, for `enroll`/`join`, to each other over
the bounded enrollment channel. None of them read or write anything in
etcd, call `kube-apiserver`, or touch `k0s`'s `ClusterConfig`. A node can
run `genesis`, sit there fully enrolled and TPM-key-ready, indefinitely,
while its `kube-apiserver` keeps running exactly as it does today —
Secrets stay unencrypted (`identity` provider, or whatever the cluster
already uses) until **Phase B** explicitly turns encryption on. This
decoupling is deliberate: it lets you fully prepare every node's TPM key
material first, verify it, and only then flip the switch that actually
changes what `kube-apiserver` does — rather than coupling TPM enrollment
to a live control-plane change.

## 2. Phase A — prepare every node's TPM key material (no cluster impact)

### 2a. Pick one existing node as genesis

Any one controller. It becomes the fleet's first sealing-key holder. On
that node:

```console
$ sudo /opt/sceau/bin/sceau genesis
```

(A `sceau-genesis.service` oneshot unit, or a [Kairos](https://kairos.io)
(the immutable Linux OS distribution these nodes boot) cloud-config
`stages:` entry — see §3 — is the real way this runs; shown bare here for
clarity.) This exits `0` once the fleet's duplicable sealing key exists in
that node's TPM. **Nothing else changes on this node** — its existing
steady-state `sceau.service` (ADR-0001's per-node SRK, serving KMS v2 on
`/run/sceau/sceau.sock`) is unaffected; the fleet key is a second,
separate TPM object sceau will only start using once Phase B tells it to
via `EncryptionConfiguration`.

### 2b. Enroll every other existing node, one at a time

On the genesis node (or any node that has already joined), open a bounded
enrollment window:

```console
$ sudo /opt/sceau/bin/sceau enroll --listen=0.0.0.0:8443 --max=1 --timeout-secs=600 \
    --allow-node=<joiner-node-name>
```

`--allow-node` is **required** and names the node you are enrolling right now
(as it appears in `kubectl get nodes` — the same name as the `system:node:<name>`
CN in that node's kubelet certificate). Repeat the flag to enrol several nodes
in one window.

Holding a valid k0s node certificate is deliberately *not* sufficient on its
own: every kubelet in the cluster — workers included — has one, and the fleet
key unseals every DEK in the cluster. `enroll` refuses to start without at
least one `--allow-node`, so a forgotten flag fails loudly rather than opening
the window to any current cluster member.

(Port `8443`, not `9443` — k0s's own `k0sApiPort` already listens on `9443`
by default; `--listen=0.0.0.0:9443` fails live with `Address already
in use (os error 98)` on any real controller node. Confirmed 2026-09-06,
see CHANGELOG.)

Blocks until it serves exactly one joiner or 10 minutes elapse, then exits
(`0` on success). A rejected/failed attempt against this listener (wrong
node, not-yet-joined, transient error) no longer consumes the `--max=1`
slot — it's safe to have `join` fail once and simply retry against the
same still-running `enroll` (fixed 2026-09-07, see below). While that's
running, on the node joining:

```console
$ sudo /opt/sceau/bin/sceau join --seed=<seed-node-address>:8443
```

Must run **after** that node's own `k0scontroller` join has completed —
`join` authenticates with the k0s-issued client certificate that
bootstrap produces, so it doesn't exist before that (ADR-0003 Decision 2).
Exits `0` once the fleet key is duplicated in and `TPM2_EvictControl`-
persisted locally. Safe to re-run on the same node (e.g. after a failed
attempt, or to pick up a rotated fleet key) — `join` always replaces
whatever it previously held at that slot; it never errors on an
already-occupied handle (fixed 2026-09-07, see above).

Repeat 2b for every remaining node. Order doesn't matter beyond "the seed
you point at has already completed its own join" — a freshly-joined node
is immediately eligible to serve as the next joiner's seed.

### 2c. Confirm before proceeding

Every node should now be running steady-state `sceau` with the fleet key
present. Run, on every node:

```console
$ sudo /opt/sceau/bin/sceau status
fleet_key=true key_id=sceau-8bfca9a52b4f9897
```

Confirm `key_id` is **identical** across every node before proceeding —
that's the actual proof the fleet key is shared, not just present locally
(added 2026-09-07; previously this required reading `sceau serve`'s own
startup log line, `TPM primary key ready key_id=...`, which still works
identically if you'd rather not touch a running node's process). Do not
proceed to Phase B until you've confirmed this for every node — Phase B
assumes it.

## 3. Cloud-config shape for Phase A

Kairos cloud-config `stages:` entries run on boot; gate anything meant to
run exactly once with a marker file, matching this stack's existing
`after-install-chroot` identity-wipe pattern (see
`docs/architecture/banlieue-vtpm-kairos-k0s-encryption.md` §4). Two
different node roles need two different stages:

**Genesis node** (one node, once — e.g. baked into that specific node's
own userData, not the shared image):

```yaml
stages:
  boot:
    - name: "sceau: fleet genesis (once)"
      if: '[ ! -f /etc/sceau/.genesis-done ]'
      commands:
        - /opt/sceau/bin/sceau genesis
        - mkdir -p /etc/sceau && touch /etc/sceau/.genesis-done
```

**Joining node** (every other node — this can live in the shared
`VMImageSpec.cloud_configs` layer, since the seed address is the only
per-cluster value):

```yaml
stages:
  boot:
    - name: "sceau: join fleet (once, after k0s controller-join)"
      if: '[ ! -f /etc/sceau/.joined ] && systemctl is-active --quiet k0scontroller'
      commands:
        - /opt/sceau/bin/sceau join --seed=${SCEAU_SEED_ADDRESS}
        - mkdir -p /etc/sceau && touch /etc/sceau/.joined
```

The `systemctl is-active k0scontroller` check is a coarse stand-in for
"the k0s-issued client cert this node needs now exists" — a boot-stage
`if:` can't easily wait/retry, so in practice this is better expressed as
a small oneshot systemd unit with `After=k0scontroller.service` plus a
retry loop waiting for the actual cert file, not a cloud-config `stages:`
one-liner. Sketch:

```ini
[Unit]
Description=sceau fleet join (ADR-0003, one-shot)
After=k0scontroller.service
ConditionPathExists=!/etc/sceau/.joined

[Service]
Type=oneshot
ExecStart=/opt/sceau/bin/sceau join --seed=%H:8443
ExecStartPost=/bin/sh -c 'mkdir -p /etc/sceau && touch /etc/sceau/.joined'
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
```

`ConditionPathExists=!/etc/sceau/.joined` makes re-running the unit (e.g.
on every boot via `WantedBy`) a safe no-op once joined — systemd skips
`ExecStart` entirely rather than re-invoking `--join` against a TPM that
already holds the fleet key.

## 4. Phase B — turn encryption on cluster-wide

This is the only phase that touches the running cluster. By this point
every node's `sceau` already holds the identical fleet key (Phase A) — the
question is purely how to safely change what `kube-apiserver` does with
it, not whether every node is capable of decrypting anything.

### 4a. Push the same `EncryptionConfiguration` to every node first, then restart — never restart before every node has it

```yaml
# /etc/k0s/encryption-config.yaml — identical content on all controllers
apiVersion: apiserver.config.k8s.io/v1
kind: EncryptionConfiguration
resources:
  - resources:
      - secrets
    providers:
      - kms:
          apiVersion: v2
          name: sceau
          endpoint: unix:///run/sceau/sceau.sock
          timeout: 3s
      - identity: {}
```

and `spec.api.extraArgs.encryption-provider-config: /etc/k0s/encryption-config.yaml`
in `k0s.yaml` on every node (exact keys validated live — see the
encryption doc's Appendix A.3/A.4). Distribute this file to **all**
controllers before restarting **any** of them. If even one node restarts
into the new config while others still don't know about the `kms`
provider at all, a Secret written on the already-restarted node and read
on a not-yet-restarted one will fail outright — that node's
`EncryptionConfiguration` doesn't declare `kms` yet, so it isn't a matter
of preference between providers, it's an unrecognized one. Getting the
file everywhere first, unread by any apiserver until you choose to
restart, avoids this being possible at all.

### 4b. Restart `k0scontroller` one node at a time — never all at once, and not because of `sceau`

`k0scontroller` supervises `etcd` *and* `kube-apiserver` together on each
node — restarting it drops that node's etcd member too. Restarting all
controllers simultaneously briefly drops the whole cluster's etcd quorum,
a real availability risk with nothing to do with encryption specifically;
it's the same reason you'd roll any control-plane config change one node
at a time. Since Phase A already guaranteed every node can decrypt
anything sealed by any other, there is no `sceau`-specific reason to
throttle this further (no need to redirect all traffic to one controller,
the way a *fresh, differently-provisioned* node join would need — see
`docs/architecture/banlieue-vtpm-kairos-k0s-encryption.md` §8's rejection
of that plan for a different scenario). Plain rolling restart, one
controller at a time, confirming each is healthy (`/readyz`, its `sceau`
socket answering) before moving to the next, is sufficient here.

### 4c. Worth verifying before Phase B, not assumed here: automatic reload

Kubernetes' `--encryption-provider-config-automatic-reload` flag lets
`kube-apiserver` pick up an updated `EncryptionConfiguration` file from
disk without a restart at all — if k0s's build exposes it as a pass-through
`extraArgs` value (worth confirming against your k0s version; this cluster
runs `v1.35.1+k0s`, well past where this landed upstream), enabling it
turns 4a+4b into a single step: push the file to all six nodes and let
each `kube-apiserver` reload it on its own short poll interval, with no
restart and therefore no etcd-quorum risk at all. Not relied on as the
primary plan above since it hasn't been confirmed working against this
k0s build yet — confirm it, and this whole phase gets simpler.

### 4d. Re-encrypt Secrets that predate encryption being turned on

Turning on `EncryptionConfiguration` only encrypts objects written *after*
that point — existing Secrets stay whatever they were (plaintext, if this
cluster never had encryption before) until they're next written. Force a
rewrite of everything once Phase B is confirmed healthy on all nodes:

```console
$ kubectl get secrets --all-namespaces -o json | kubectl replace -f -
```

## 5. Open items this guide surfaces, not solved here

- ~~A `sceau --status`/equivalent way to confirm a node actually holds the
  fleet key, for §2c~~ — resolved 2026-09-07, see §2c above.
- §3's `ConditionPathExists`-gated oneshot unit is a sketch, not something
  run end-to-end yet — build and validate it once `--join` itself exists.
- Key rotation (ADR-0003 Consequences) isn't covered by this guide at all
  — a suspected fleet-key compromise needs its own re-duplication runbook,
  not written yet.
