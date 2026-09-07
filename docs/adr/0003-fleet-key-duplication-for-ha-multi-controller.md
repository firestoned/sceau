<!-- Copyright (c) 2026 Erick Bourgeois, sceau -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# 0003 — Fleet key duplication for HA multi-controller clusters, via native `sceau` enrollment

## Status

Accepted — 2026-09-05 (revised 2026-09-06 to add TPM credential-activation
attestation to Decision 3, closing the EK-authenticity gap this ADR
originally deferred — see Decision 3's new attestation step and the
Consequences this revision resolves/replaces). Amends [ADR-0001](0001-tpm-sealed-kms-v2-plugin.md)
— its "Negative / risks" section did not consider multi-controller HA at
all; this ADR closes that gap for clusters where more than one node runs
`kube-apiserver` against shared etcd. **This revision replaces this ADR's
own original design** (an external, SSH-orchestrated `sceauctl` tool with a
static Vault-held master key) with native enrollment inside `sceau` itself,
authenticated by the [k0s](https://k0sproject.io) (the single-binary
Kubernetes distribution this project targets) join trust every node
already establishes. No consumer has built against the original design —
there is nothing to
migrate away from, so it is rewritten rather than superseded-and-kept.

## Context

`sceau`'s current design (ADR-0001) seals both the recreated primary and
every child seal object with `.with_fixed_tpm(true).with_fixed_parent(true)`
(`src/tpm.rs`) — a hardware-enforced, non-duplicable binding to the exact TPM
that created the object. ADR-0001 frames this as a pure positive ("a cloned
disk is useless without the same TPM"), and it is, for the single-controller
edge topology the ADR was written against.

It breaks down for a genuinely HA k0s cluster: multiple controller+worker
nodes, each running its own `kube-apiserver` behind a load balancer (found
while planning a migration of exactly such a cluster — 6 nodes, all labeled
`control-plane`, all potential apiserver replicas). Kubernetes' KMS v2
plugin transport is a **local Unix domain socket only**, by design — there
is no supported way to point one node's `kube-apiserver` at another node's
`sceau` socket over the network, so this cannot be solved via
`EncryptionConfiguration` or any k0s/kube-apiserver flag. With today's
`fixedTpm` sealing, a Secret written through node A's apiserver is only ever
decryptable by node A: a read routed to node B fails outright, regardless of
etcd/Raft (which has already fully replicated the ciphertext bytes to every
member — storage durability and decrypt availability are independent
properties here, and only the first is HA today).

Two candidate fixes were evaluated:

1. **Cross-node decrypt relay.** Keep `fixedTpm` sealing (no crypto change).
   Each `sceau` instance tries its own TPM first; on `key_id` mismatch
   (`key_id` is already derived from the SRK name and embedded per
   ciphertext, per ADR-0001/current code), relay the decrypt call over an
   authenticated inter-node channel to whichever node's `key_id` matches.
   Preserves per-node blast radius (compromising one node's TPM/seal only
   exposes that node's own sealed Secrets) but does **not** restore true
   read HA: a Secret sealed by node A is still unreadable cluster-wide if
   node A itself is down, relay or not — the relay fixes routing, not
   availability.
2. **Shared key via `TPM2_Duplicate`/`TPM2_Import`.** Create the fleet's
   sealing key as duplicable (drop `fixedTpm`/`fixedParent` for this key
   specifically — the recreated SRK described in ADR-0001 stays exactly as
   it is; this is a *second*, distinct TPM object, not a change to the
   first), duplicate it once from a seed node into every other node's TPM
   at enrollment time. Any node can then decrypt anything any other node
   sealed — genuine read HA. Trade-off: blast radius becomes fleet-wide —
   compromising *any one* node's TPM/seal now exposes every Secret ever
   sealed under the shared key, not just that node's own. Also gives up
   ADR-0001's "stateless, nothing to rotate" property for this key
   specifically: a suspected compromise now requires re-running the
   duplication ceremony across the whole fleet, not just recreating one
   node's deterministic primary.

## Decision

1. **Adopt option 2 (shared key via TPM duplication)** for HA
   multi-controller topologies. Read availability was judged more
   operationally important than per-node blast-radius isolation for this
   use case — losing the ability to read *any* Secret whenever *any one*
   control-plane node is down is worse, in practice, than the fleet-wide
   blast-radius trade a shared key makes. Single-controller/edge
   deployments are unaffected: they keep ADR-0001's `fixedTpm` behavior
   unchanged, since there is only ever one TPM to seal against.

   **Scope, stated explicitly so "fleet" is never misread: this is
   per-cluster, never fleet-wide across every VM/cluster
   [`banlieue`](https://github.com/firestoned/banlieue) (the
   Kubernetes-native VM lifecycle API that provisions the nodes this ADR's
   `sceau` instances run on)
   manages.** Everywhere this ADR says "fleet," it means exactly one
   thing — the set of control-plane nodes sharing a single etcd quorum,
   for one k0s cluster. A node that doesn't run `kube-apiserver` against
   that specific etcd quorum has no reason to enroll at all, worker-only
   nodes included. A different cluster — even one running on the same
   hosts, the same `banlieue` instance, the same vCenter, the same
   crypto-team-attested ESXi hosts — gets its own independent
   `genesis`, its own independent duplicable key, and its own
   independent seed(s); nodes are never enrolled across cluster
   boundaries. An operator running many k0s clusters ends up with as many
   independent keys as clusters, by design — never one organization-wide
   key covering everything `banlieue` has ever provisioned.

2. **The duplication ceremony is performed by `sceau` itself, in three
   distinct startup modes — no external tool, no SSH, no static
   Vault-held master key.** `TPM2_Duplicate`/`Import` must run from inside
   each guest against its own local TPM command interface (`/dev/tpmrm0`)
   — there is no vSphere/hypervisor API that can reach into a vTPM's
   command interface, so this can never be a `banlieue` (VM provisioning)
   responsibility; `banlieue`'s `add_tpm_device` (its own ADR-0039) only
   ever attaches inert virtual TPM *hardware* before first boot. The three
   modes:

   - **Genesis** (`sceau genesis`) — the first controller node in a new
     cluster. No peer to enroll against. Creates the fleet's sealing key as
     a duplicable object (distinct from, and in addition to, the
     recreated-every-boot SRK from ADR-0001) and immediately becomes able
     to serve enrollment for the next mode. `--force` deletes and recreates
     an existing fleet key (e.g. one that predates a fix to what its
     `authPolicy` needs to be) — never the default, since it orphans every
     other node's copy on an already-enrolled fleet.
   - **Seed/server** (`sceau enroll --listen=<addr>
     --max=<n> --timeout-secs=<d>`) — any node that already holds
     the fleet key (from genesis or its own prior join). Opens a network
     listener that serves **only** the duplicate RPC, authenticated and
     authorized per Decision 3, bounded by count and/or timeout, then
     closes — reverting to the unix-socket-only steady state with no
     network code path live for the rest of the process's life. Only the
     serving side ever listens.
   - **Joiner/client** (`sceau join --seed=<seed-address>`) — a newly
     provisioned node. Dials out only (no inbound listener needed on this
     side at all); creates a fresh, plain duplicable storage key (same
     empty-auth `ObjectAttributesBuilder` pattern `src/tpm.rs` already uses
     for the SRK, just without `fixedTpm`/`fixedParent`) as the
     `TPM2_Duplicate` wrapping target, **not the vTPM's Endorsement Key
     directly** (revised from an earlier draft of this Decision — see the
     note below) — attestation (Decision 3) is a separate, additional gate
     the seed runs before authorizing that duplication; it does not change
     what gets wrapped. Also creates (or, on a subsequent run, loads) an
     Attestation Key under the Endorsement hierarchy (`abstraction::ak::
     create_ak_2`/`load_ak`) and retrieves its own EK certificate
     (`abstraction::ek::retrieve_ek_pubcert`) — both needed for Decision
     3's attestation exchange, confirmed present in `tss-esapi` 7.7.0
     (2026-09-06). Authenticates to the seed via its own k0s-issued mTLS
     client cert (see Decision 3 — this means `sceau join`
     must run **after** the node's own k0s controller-join completes, an
     explicit systemd ordering dependency, since the cert used for this
     step doesn't exist before that); receives the duplicate blob;
     `TPM2_Import`s it locally; **persists it via `TPM2_EvictControl`** so
     it survives a reboot without needing to re-enroll (a merely-loaded
     TPM object is volatile by default — skipping this would mean every
     reboot re-runs the whole join ceremony, which is fragile in a way
     that doesn't need to be); then starts serving KMS v2 normally, and is
     itself now eligible to act as a seed for the next joiner.

3. **Transport is one-directional; authentication reuses k0s's own PKI;
   authorization is a new check `sceau` must own, since k0s's PKI has no
   opinion on it.** Only the serving/seed side ever opens a port — the
   joining side is a pure outbound client, which keeps the network
   exposure to "however many existing members are currently willing to
   serve enrollment," not every node in the fleet.
   - **Authentication:** mTLS, using the joining node's k0s-issued client
     certificate — proves *which machine* is asking, reusing trust that
     already exists as a byproduct of that node completing k0s's own
     join-token-authenticated bootstrap. No new PKI, no new credential
     class, no Vault/Fusion-API/Chorus dependency for this step at all.
   - **Authorization is a separate, explicit check, because mTLS alone
     only proves identity, not entitlement.** The seed checks the
     authenticated peer's node identity against the cluster's own `Node`
     objects (the seed already has API-server access) before serving a
     duplicate request. This means the bar to request the fleet key isn't
     "holds any k0s-signed cert" — it's "has already independently passed
     k0s's own bootstrap-token-gated join and exists as a real cluster
     member," a materially higher bar, and one that requires no
     separately-maintained allowlist to stay in sync with cluster
     membership. An explicit allowlist may be layered on top for
     defense-in-depth later; it is not required for this design to be
     sound.
   - **A second, independent gate: TPM credential-activation attestation,
     proving hardware, not just identity.** mTLS/Node-identity above
     proves *the machine asking has a valid k0s identity* — it says
     nothing about whether the TPM answering on that channel is a genuine,
     unmodified vTPM rather than something else entirely. This closes that
     gap as an additional requirement layered on top of (not instead of)
     Decision 3's existing mTLS/Node check — both must pass before the
     seed serves `Duplicate`. Confirmed buildable against `tss-esapi`
     7.7.0's real API (2026-09-06, no assumptions): `Context::
     make_credential`, `Context::activate_credential`, `Context::quote`,
     `abstraction::ek::retrieve_ek_pubcert`, `abstraction::ak::create_ak_2`/
     `load_ak` all exist with the signatures this protocol needs.
     Exchanged over the same mTLS channel as the duplicate RPC (new
     request/response messages on the existing `enroll.v1` proto, not a
     new transport):
     1. Joiner → seed: its EK certificate (`retrieve_ek_pubcert`) and AK
        public + name (`create_ak_2`/`load_ak`).
     2. Seed validates the EK certificate chains to a configured trust
        anchor — vCenter's VMCA root (§3b of the encryption doc: VMCA
        signs the EK cert at vTPM-attach time). **New operational input**:
        the seed needs that VMCA root cert available locally, delivered
        the same way as k0s's own CA and the seed address (Decision 3's
        `VMImageSpec.cloud_configs` channel) — not designed further here.
     3. Seed loads the joiner's EK public as an external object on its
        *own* TPM (`load_external_public`, the same pattern
        `duplicate_for_joiner` already uses for the joiner's transport
        key), generates a random secret, and wraps it to the joiner's AK
        name via `make_credential` — standard TPM2 credential activation,
        not sceau-specific.
     4. Seed → joiner: the wrapped credential (`IdObject` +
        `EncryptedSecret`).
     5. Joiner recovers the secret via `activate_credential` against its
        own, real AK and EK handles — this only succeeds if the AK is
        genuinely resident under that specific EK, on that specific TPM.
     6. Joiner also runs `quote` over an agreed PCR selection, signed by
        the AK, and sends both the recovered secret and the
        quote+signature back to the seed.
     7. Seed accepts only if: the recovered secret matches what it
        generated (proves AK/EK hardware binding) **and** the quote
        signature verifies under the joiner's AK public **and** the PCR
        values match a configured allowlist. **New operational input**:
        that PCR allowlist, which needs maintaining across kernel/
        firmware/bootloader updates — a real, ongoing operational cost,
        not a one-time setup step (see Consequences).
     Only once both this and the existing mTLS/Node-identity check pass
     does the seed proceed to `duplicate_for_joiner` — unchanged from
     Decision 2: the wrapping target is still the joiner's separate,
     unauthenticated transport key, not the EK/AK used for attestation.
   - **The joining node needs to know the seed's address.** Delivered via
     the same `VMImageSpec.cloud_configs` channel already used to deliver
     `install.encrypted_partitions` and (per the encryption doc's §7) will
     carry `sceau`'s own `EncryptionConfiguration` — no new delivery
     mechanism invented for this.
   - **Addendum, found implementing this Decision against a real k0s node:
     the seed's own TLS server identity cannot be the same kubelet-client
     cert used above.** That cert (confirmed live:
     `/var/lib/k0s/kubelet/pki/kubelet-client-current.pem`, `CN=system:
     node:<hostname>`, `O=system:nodes`) has no Subject Alternative Name
     and `Extended Key Usage: TLS Web Client Authentication` only — correct
     for authenticating the *joiner*, but rustls (the TLS backend both
     `sceau`'s client and server use) requires SAN-based hostname
     verification with no CN fallback, so it cannot serve as the *seed's*
     TLS identity. The seed instead mints a short-lived, in-memory-only
     leaf certificate at `enroll` startup — `SAN = CN =` this node's own
     `system:node:` identity — signed by k0s's own CA
     (`/var/lib/k0s/pki/ca.crt` + `ca.key`, both already present on every
     controller, via the `rcgen` crate). This is still k0s's existing CA —
     no new trust root — just a purpose-built leaf the way
     apiserver/scheduler/ccm/etc. already each get their own. The
     alternative considered (skip hostname verification, trust-chain only)
     was rejected: it would weaken the check from "this specific host" to
     "holds any k0s-issued cert," with the operator-supplied seed address
     becoming the only thing anchoring identity.

4. **This is a deliberate, explicit, narrow exception to ADR-0001's "no
   network calls" framing — see the note added there.** The exception is
   scoped (one RPC, nothing else reachable), bounded (`--max`/
   `--timeout-secs`, then the listener closes for good), and one-sided
   (only serving nodes, never joiners, expose anything). Outside the
   enrollment window, there is nothing listening to attack, not merely
   something denying unauthorized requests.

5. **`sceau serve`'s steady state prefers the fleet key over the per-node
   SRK whenever one exists, checked once at startup, for `Encrypt` and as
   `Decrypt`'s first choice — but keeps the per-node SRK alive as a
   `Decrypt`-only fallback, never used for `Encrypt`.** This is the piece
   that makes genesis/enroll/join actually matter: without it, every node
   still seals under its own `fixedTpm` SRK regardless of enrollment, and
   cross-node decrypt (this whole ADR's motivating problem) never actually
   happens. Concretely, `sceau serve` tries `fleet::load_fleet_key` first;
   on success, new `Encrypt` calls seal under that handle instead of the
   deterministic SRK, and the reported `key_id` — derived from the sealing
   key's own `Name`, same derivation either way — is now the *fleet's*
   `key_id`, identical across every enrolled node, which is exactly what
   lets `kube-apiserver` route a `Decrypt` call to any node's `sceau` and
   get a consistent answer. `Decrypt` tries the fleet key first by
   `key_id`; on mismatch, it also tries the per-node SRK before giving
   up — so a node's own pre-enrollment ciphertext (sealed under its old
   `key_id`) stays readable indefinitely after joining, not just until an
   operator gets around to a re-encryption sweep. Any error probing for the
   fleet key (including simply not finding one) falls back to today's
   ADR-0001 per-node-SRK-only behavior unconditionally — single-
   controller/edge deployments that never run `genesis` are completely
   unaffected, exactly as promised in Decision 1.

## Consequences

- **Fleet-wide blast radius, accepted deliberately — scoped to one
  cluster's own etcd quorum, per Decision 1, not every VM/cluster
  `banlieue` manages.** A single compromised node's TPM/seal exposes
  every Secret sealed under that cluster's shared key, not just that
  node's own; it exposes nothing belonging to any other cluster, since
  each cluster's key comes from its own independent `genesis`.
- **Key rotation requires a full re-duplication ceremony across that
  cluster's own nodes**, not a per-node primary recreation and not an
  organization-wide operation — a real, new operational procedure (not
  designed here; a follow-up ADR or an extension of this one once the
  rotation story is worked out).
- **No new external credential class, and no static master key — a real
  improvement over this ADR's original design, not just a different
  implementation of the same risk.** The original version of this ADR
  introduced a Vault-held SSH key valid fleet-wide indefinitely, explicitly
  named as "the highest-value secret in this whole design." This revision
  has no equivalent: the only things a node needs are (a) its own vTPM's
  EK, which it already has, and (b) a k0s-issued client cert, which it
  already gets from joining the Kubernetes control plane it was going to
  join anyway. There is no separate, standing, high-value credential to
  protect.
- **Genesis needs its own explicit path.** Exactly one node per cluster
  runs in `genesis` mode, creating the duplicable fleet key with no peer
  to enroll against. This is new, explicit `sceau` behavior that didn't
  exist before this ADR — worth calling out since it's easy to design the
  join/seed sides and forget that the very first key has to come from
  somewhere.
- **Genesis bootstrap window: single point of total loss until the first
  join completes.** Between `genesis` creating the fleet key and the
  first successful `join`, exactly one machine holds it — losing that
  node in this window loses the fleet key outright, before it was ever
  meant to be a single-node risk. Operational guidance, not a code
  requirement: join at least one (ideally two) additional nodes promptly
  after genesis, before treating the fleet as provisioned.
- **Nothing currently prevents accidental double-genesis.** Two nodes both
  started with `genesis` (operator error, plausible specifically during
  a migration window when several nodes are being provisioned close
  together) silently creates two incompatible fleet keys with no error —
  each behaves correctly in isolation, so this fails silently rather than
  loudly, and shows up later as some subset of nodes unable to decrypt
  Secrets sealed by the other subset. Not designed here: a lightweight
  guard (e.g. a well-known marker path/label checked before allowing
  `genesis` to proceed) or, at minimum, an explicit runbook callout that
  `genesis` is a once-per-cluster operation, should land before this is
  used on a real migration.
- **Resolved (design-level; not yet implemented — see below): EK
  authenticity is now a designed part of Decision 3, not a deferred gap.**
  The seed's mTLS check alone only authenticates *the machine* (via its
  k0s-issued cert), not the TPM answering on that channel. Decision 3's new
  attestation step closes this specific gap via standard `TPM2_
  MakeCredential`/`ActivateCredential` credential activation plus a
  `TPM2_Quote`/PCR check, run as a second, independent gate alongside the
  existing mTLS/Node-identity check — both must pass. This does not change
  the `TPM2_Duplicate` wrapping target (still the joiner's fresh,
  unauthenticated transport key, per Decision 2 — attestation and
  duplication use two different TPM objects for two different purposes,
  the AK/EK pair only for proving hardware identity). **Caveat, stated
  plainly: this is a design confirmed buildable against `tss-esapi`
  7.7.0's real API (2026-09-06) — the attestation exchange itself is not
  yet implemented in `src/`,** and TPM2 session handling for
  `ActivateCredential`/`MakeCredential` is exactly the kind of "more
  intricate than anything else in this codebase" territory that produced
  three separate live-hardware bugs (`TPM_RC_AUTH_TYPE`, `TPM_RC_
  POLICY_FAIL`, a session-continuance bug) just building the simpler
  `TPM2_Duplicate` path earlier this session. Treat this Decision as the
  target, not as done.
- **New operational inputs this attestation step requires, neither
  designed further here.** (1) The seed needs vCenter's VMCA root
  certificate available locally, to validate a joiner's EK cert chain —
  a new trust anchor to deliver and keep current, on top of k0s's own CA
  and the seed address this ADR already required. (2) A PCR allowlist the
  seed checks a joiner's quote against — this is an ongoing maintenance
  cost, not a one-time setup step: every kernel/firmware/bootloader update
  to the golden image shifts PCR values, and the allowlist has to track
  that or every real joiner starts failing attestation. Both are new
  supply-chain dependencies on vCenter's own VMCA and image-build process
  that this ADR's design did not previously carry.
- **Resolved: `genesis`, `enroll`, and `join` are all one-shot
  invocations that exit, never the long-running steady-state process.**
  `enroll` blocks until it serves `--max` joiners or
  `--timeout-secs` elapses, then **exits 0** on success (or non-zero if
  the window closed with nothing enrolled) — it does not fall through into
  serving KMS v2 itself. `genesis` and `join` exit 0 immediately once
  the fleet key exists locally (created, or duplicated-in-and-persisted via
  `TPM2_EvictControl`, respectively). This makes all three a natural fit
  for a `systemd` `Type=oneshot` unit (or a Kairos cloud-config `stages:`
  command) run once, separate from and before the plain `sceau` steady-state
  service that actually serves the KMS v2 socket — matching the pattern
  already used for `sceau.service` itself (see the migration guide,
  `docs/migration-ha-existing-cluster.md`). No mode transitions in-process;
  each of the four modes (including plain steady-state) is a distinct
  process invocation with a clear start and end.
- Single-controller/edge deployments (ADR-0001's original target) are
  unaffected — `fixedTpm` sealing stays the default there; none of
  `genesis`/`enroll`/`join` are invoked, and no network code path in
  `sceau` is ever reachable on those deployments at all.
- **Resolved: `TPM2_EvictControl` against an already-occupied persistent
  handle fails loudly, it does not silently overwrite or no-op** —
  confirmed live 2026-09-07 (`TPM_RC_NV_DEFINED`, "NV Index or persistent
  object already defined"), closing the open question this ADR originally
  left for real-hardware verification. This meant `join` on a node that
  already held any object at the fleet key's slot (a retry after a prior
  attempt, or re-enrolling after a reset) was **guaranteed to fail** before
  the fix below — a real gap, not a hypothetical one; it reproduced on the
  first genuine two-fresh-node round-trip test. Fixed: `join`'s
  `import_and_persist` now unconditionally evicts whatever occupies the
  handle before persisting the newly-received key. This is deliberately
  asymmetric with `genesis`, which only replaces an existing key behind
  the opt-in `--force` — `genesis`'s default protects an already-enrolled
  fleet from an operator accidentally orphaning every other node's copy,
  a risk that does not exist for `join`: running `join` is itself the
  explicit statement of intent to receive and hold the fleet's current
  key, so there is no "protect what's there" case to guard. See
  `.claude/CHANGELOG.md` (2026-09-07) and the ADR-0003 roadmap's Phase 3
  entry for the live round-trip this fix was verified against.
- **Resolved: a rejected/failed `enroll` request no longer burns a `--max`
  permit.** Live-testing Decision 3's Node-identity check against a real,
  deliberately-unauthorized request (mTLS valid, `Node` object absent)
  surfaced a second bug alongside the one above: `enroll.rs`'s permit
  counter was decremented unconditionally before the auth/authz checks
  ran, and only the explicit "already at zero" branch ever restored it.
  Confirmed live 2026-09-07: with the realistic `--max 1` default, one
  rejected attempt permanently exhausted the enrollment window, and a
  subsequently legitimate `join` failed with `--max already reached` —
  nothing had actually enrolled. This is a real availability gap in the
  bound Decision 3's `--max`/`--timeout-secs` design exists to provide
  (a bad or misconfigured request should never be able to deny a
  legitimate one). Fixed: the permit is now restored on any failure from
  the request-handling logic, not just the "already at max" case; only a
  genuinely served `Duplicate` keeps it consumed. Re-verified live: reject
  → restore the node's cluster membership → a second, legitimate `join`
  now succeeds in the same `enroll --max 1` session. See
  `.claude/CHANGELOG.md` (2026-09-07) and the roadmap's Phase 4 entry.
- **Resolved: `sceau status` (Phase 7) exists, and its own implementation
  fixed a third live-only bug.** `TpmSealer`'s `Drop` impl (used by both
  `serve` and now `status`) called `TPM2_FlushContext` unconditionally,
  with a comment claiming this was harmless on an already-persistent
  handle (the fleet key). Confirmed live 2026-09-07 that real hardware
  rejects it (`TPM_RC` 0x1c4) — harmless in observed effect (the error was
  already discarded), but wrong, and it had been happening silently on
  every `sceau serve` shutdown using the fleet key; `status`'s short
  process lifetime (create-then-immediately-drop, on every invocation)
  made it impossible to keep ignoring. Fixed: `TpmSealer` now tracks
  whether it owns a transient primary and only flushes in that case. See
  `.claude/CHANGELOG.md` (2026-09-07).
