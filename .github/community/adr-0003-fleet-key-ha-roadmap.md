# sceau ADR-0003 — Fleet Key Duplication for HA Multi-Controller — Roadmap

**Author:** Erick Bourgeois
**Source decision:** `docs/adr/0003-fleet-key-duplication-for-ha-multi-controller.md`
in `firestoned/sceau` (Accepted, revised 2026-09-05).
**Repo in scope:** `firestoned/sceau` only.
[`banlieue`](https://github.com/firestoned/banlieue) (the Kubernetes-native
VM lifecycle API that provisions these nodes; `add_tpm_device`, ADR-0039)
and the vm-build stack's
`docs/architecture/banlieue-vtpm-kairos-k0s-encryption.md` are upstream/
downstream context, not touched by this roadmap directly — Phase 8 below is
where they connect back in.

> Status legend: ☐ not started · ◐ in progress · ☑ done.
> Effort: S ≤ half day · M ≤ 2 days · L > 2 days.
> Every code change is TDD where the code is TPM-independent; TPM-touching
> code is verified live (no simulator available in the dev sandbox — see
> `rules/testing.md`'s `swtpm`-gated integration-test convention) and against
> the real two-node test fleet (`k0s-node1`/`k0s-node2`, provisioned outside
> this repo).

---

## Ordering rationale

Land the pure-crypto core first (it's the part nothing else in the codebase
resembles, and the highest-risk-to-get-wrong), verify it on real hardware
before building the network layer on top of it, then the network layer,
then the one k8s-integration point (Node-identity auth), then operational
safety nets, then the deferred security hardening (EK-backed authenticity),
then packaging. Each phase's own real-hardware verification gates the next
phase starting — this design has already needed two live-compiler-driven
fixes ([`fleet.rs`](../../src/fleet.rs) is the first TPM-duplication
code in this codebase; nothing here should be assumed correct from
documentation alone until it's actually run).

---

## Phase 0 — Design ☑ done

- ☑ ADR-0003 written, accepted, revised same day (dropped the original
  external `sceauctl`+SSH+Vault-key design for native `sceau` enrollment
  authenticated by [k0s](https://k0sproject.io)'s own PKI — a real security
  improvement, not just a different implementation of the same risk).
- ☑ ADR-0003 revised again: `TPM2_Duplicate`'s wrapping target is a fresh
  joiner-created duplicable key, not the vTPM's Endorsement Key directly —
  the EK's `TPM2_PolicySecret` session handling was judged too high-risk to
  implement correctly with no TPM/simulator to validate against as part of
  the *first* cut. **This is what Phase 6 below comes back to close.**
- ☑ [FINOS CALM](https://calm.finos.org) (Common Architecture Language
  Model — an open-source, JSON-based architecture-description standard
  from the [FINOS](https://www.finos.org) foundation) model updated
  (`service-sceau-peer` node, `rel-sceau-join-enrollment` relationship,
  `service-sceau` description) — `make calm-validate` passes.
- ☑ Migration runbook: `docs/migration-ha-existing-cluster.md` (Phase A/B
  cloud-config shapes, `EncryptionConfiguration` cutover sequencing,
  explicit confirmation that `--genesis`/`--enroll` never touch a live
  cluster).

## Phase 1 — CLI mode surface ☑ done

- ☑ `src/cli.rs` + `src/cli_tests.rs`: `RawArgs`/`Mode`/`ModeError` —
  `--genesis`/`--enroll`/`--join` flag validation, mutual exclusivity,
  required-sub-flag checks. Pure logic, fully unit-tested.
- ☑ Wired onto `Args` in `src/main.rs`.

## Phase 2 — TPM2_Duplicate/Import primitives ☑ done, live-verified

- ☑ `src/fleet.rs` + `src/fleet_tests.rs`: `create_fleet_key`,
  `create_transport_key`, `duplicate_for_joiner`, `import_and_persist`,
  `load_fleet_key`, `persist`. Checked against `tss-esapi`'s published
  `Context` method signatures (not guessed from memory).
- ☑ `--genesis` wired end-to-end in `main.rs` (`run_genesis`) — no longer a
  stub.
- ◐ **First real build error found and fixed** (2026-09-05): `duplicate()`
  returns a plain `Data` for its first tuple element, not `Option<Data>` —
  `DuplicationBlob.encryption_key` construction needed `Some(...)`. Fixed.
  **Expect more of these** — two specific points were flagged as unverified
  in `fleet.rs`'s own module doc comment before this build even started:
  - ☐ `PersistentTpmHandle::new`'s exact constructor/error type.
  - ☐ `Persistent`'s exact module path (used in `evict_control`).
  - ☐ Whether `KeyHandle: TryFrom<ObjectHandle>` is really the right
    conversion (written defensively, not assumed infallible).
- ☑ `cargo build --bin sceau` green, on real Linux hardware.
- ☑ **Live verification on a real test node** (2026-09-06): `sceau --genesis
  --tcti device:/dev/tpmrm0` created and persisted the fleet sealing key
  against a real vTPM — `tss_esapi::context` logs confirmed a clean
  create-primary → evict_control → flush → close sequence, no errors. This
  also retroactively confirms the three points flagged unverified above:
  `PersistentTpmHandle::new`'s constructor, `Persistent`'s module path, and
  `KeyHandle: TryFrom<ObjectHandle>` all worked as written.
- ☑ `cargo test --bin sceau` green end to end (2026-09-07, via a Linux
  container with `libtss2-dev` — see `.claude/CHANGELOG.md`). Fixed one
  stale call site in `tests/fleet_duplication.rs` that had never compiled
  since `create_fleet_key` gained its `force: bool` parameter. All 33 unit
  tests pass; the TPM-dependent `fleet_duplication` integration test
  correctly skips as `#[ignore]`d with no TCTI configured.
- ☑ `make lint`/`cargo clippy` green (2026-09-07, same session).

## Phase 3 — Enrollment network layer (`enroll`/`join`) ☑ done, live-verified

`src/enroll.rs` (server), `src/join.rs` (client), `proto/enroll/v1/api.proto`
now exist. Every tonic/rustls call was checked directly against tonic
0.12.3's own source + `examples/src/tls_client_auth` (fetched, not
guessed) — but none of it has been compiled or run yet.

- ☑ **New `.proto`** (`proto/enroll/v1/api.proto`): one RPC (`Duplicate`),
  request = joiner's transport-key `Public` (marshalled bytes), response =
  `DuplicationBlob`'s four fields, plus a `has_encryption_key` bool (proto3
  `bytes` can't distinguish empty from absent on its own).
- ☑ `build.rs` wiring — `tonic_build` configured with both
  `build_server(true)` and `build_client(true)` for this proto (unlike
  `kms.rs`, `sceau` is both enroll server *and* client, never a KMS v2
  client).
- ☑ `Cargo.toml`: `tonic`'s `tls` feature enabled (confirmed via tonic's own
  `Cargo.toml` — default features are `["transport", "codegen", "prost"]`,
  `tls` is not on by default). New deps: `kube`, `k8s-openapi`, `rcgen`,
  `time`, `x509-parser`, `http`.
- ☑ **Design gap found and closed** (addendum to ADR-0003 Decision 3): the
  kubelet-client cert used for the *joiner's* client identity has no SAN and
  `Extended Key Usage: TLS Web Client Authentication` only (confirmed live
  against a real k0s node) — it cannot double as the *seed's* TLS server
  identity, since rustls requires SAN-based hostname verification with no CN
  fallback. Resolved by minting a short-lived, in-memory leaf cert at
  `enroll` startup, signed by k0s's own CA (`src/certs.rs`,
  `mint_enroll_server_identity`).
- ☑ **`enroll` server** (`src/enroll.rs`): bounded by `--max`
  (optimistic-decrement `AtomicUsize`, permit returned if already exhausted)
  and/or `--timeout-secs` (`tokio::select!` race against
  `serve_with_shutdown`), mTLS via `ServerTlsConfig` (minted leaf identity +
  `client_ca_root` = k0s's `ca.crt`), calls `fleet::duplicate_for_joiner`.
- ☑ **`join` client** (`src/join.rs`): mTLS client (`ClientTlsConfig`)
  presenting this node's own k0s-issued kubelet-client cert, dials
  `https://<--seed>`, calls `fleet::create_transport_key` +
  `fleet::import_and_persist`.
- ☑ k0s-issued cert/key file paths confirmed live on a real node:
  `/var/lib/k0s/pki/ca.crt` + `ca.key` (CA), `/var/lib/k0s/kubelet/pki/
  kubelet-client-current.pem` (per-node kubelet client cert+key,
  concatenated PEM, `CN=system:node:<hostname>`, `O=system:nodes`).
- ☑ **`cargo build`/`cargo clippy`/`cargo test` all green** (2026-09-07,
  Linux container build — see Phase 2's build-gate entry above for the fix
  this required). This validates the code compiles and unit-tests
  correctly; it does **not** substitute for the live two-node round-trip
  gate below.
- ☑ **Live verification, the actual `enroll`/`join` gate this phase
  exists for** — confirmed on the real two-node test cluster, 2026-09-07
  (UTC):
  - `genesis --force` on `k0s-node1` created a fresh fleet key (discarding
    whatever key was shared before), then `enroll --listen 0.0.0.0:8443
    --max 1 --timeout-secs 300` served it.
  - `join --seed=k0s-node1:8443` from `k0s-node2` dialed over mTLS,
    completed `TPM2_Duplicate`/`Import`, and logged `fleet sealing key
    imported and persisted` — exit `0`.
  - **A real bug was found and fixed by this run**: `import_and_persist`'s
    final `persist()` step failed with `TPM_RC_NV_DEFINED` ("NV Index or
    persistent object already defined") because `k0s-node2` already held an
    object at `FLEET_KEY_PERSISTENT_HANDLE` from a prior enrollment —
    exactly the gap Phase 7 named as unverified. Fixed in `src/fleet.rs`:
    `import_and_persist` now unconditionally calls
    `delete_persisted_fleet_key` before `persist` (unlike `genesis`, `join`
    has no "protect what's there" case — it's only ever invoked to receive
    and store a new key). See `.claude/CHANGELOG.md`.
  - After restarting `sceau.service` on both nodes, both logged the
    identical `key_id=sceau-8bfca9a52b4f9897` — the fleet key genuinely
    transferred, not just independently matching.
  - Cross-node decrypt re-confirmed bidirectionally through the real
    `enroll`/`join` path (previous Phase 5 proof used whatever mechanism
    put the earlier shared key in place, which predates this round-trip):
    a Secret created via `k0s-node1`'s apiserver read correctly via
    `k0s-node2`'s, and vice versa, after a `k0scontroller` restart on both
    nodes to clear the apiserver's stale cached-DEK state left over from
    the key rotation.

**Gate cleared:** two real nodes have completed a live `enroll`/`join`
round-trip.

## Phase 4 — k0s Node-identity authorization ☑ done, live-verified (both paths)

ADR-0003 Decision 3's second half — mTLS proves *which machine*, this
proves *that machine is entitled*. Implemented alongside Phase 3 rather than
as a separate pass, since `enroll.rs`'s `Duplicate` handler needs both
checks before it can safely call into `fleet.rs`.

- ☑ **Dependency decision recorded**: `kube` + `k8s-openapi` (rustls-tls,
  matching the TLS stack `tonic` already uses — no openssl mixed in), not
  raw HTTPS + hand-rolled service-account-token auth. `kube::Client::
  try_default()` picks up the in-cluster service account automatically.
- ☑ Seed-side check (`src/authz.rs`): peer cert (from `request.peer_certs()`)
  → CN via `x509-parser` → `node_name_from_cn` strips the `system:node:`
  prefix → `authorize_node` looks up a matching `Node` object via `kube` →
  reject if absent.
- ☑ Positive path live-verified as a side effect of Phase 3's round-trip
  (2026-09-07): `k0s-node2`'s real kubelet-client cert CN matched an actual
  `Node` object on `k0s-node1`'s apiserver, and `authorize_node` let the
  `Duplicate` RPC through — the authz check is genuinely being exercised,
  not bypassed.
- ☑ **Negative path live-verified** (2026-09-07): rather than standing up a
  third node, tested the rejection branch directly against a real,
  otherwise-valid k0s identity — deleted `k0s-node2`'s own `Node` object
  (mTLS still succeeds, its kubelet-client cert is untouched and still
  signed by k0s's real CA) and immediately ran `join` from it. Result:
  clean `PermissionDenied` / `"... is not a current member of this
  cluster's Node objects"`, and `k0s-node2`'s TPM state was provably
  untouched (the error happens before `import_and_persist` is ever
  reached). This is a stronger test of `authorize_node` specifically than
  a no-cert/wrong-CA client would have been — it isolates the Node-existence
  check from the mTLS layer, rather than conflating the two.
  - **A second real bug was found and fixed by this test**: with
    `--max 1` (the realistic default), this one rejected attempt
    permanently consumed the sole enrollment permit — a subsequent
    *legitimate* `join` (after restoring the Node object) failed with
    `--max already reached`, even though nothing had ever actually
    enrolled. Fixed in `src/enroll.rs`: `duplicate`'s permit is now
    restored on any `Err` from the request-handling logic, not just the
    explicit "already at max" branch. Re-verified live: reject → restore
    Node membership → legitimate `join` now succeeds in the same session.
    See `.claude/CHANGELOG.md`.
  - `k0s-node2`'s cluster membership and `node-role.kubernetes.io/
    control-plane` label were restored afterward (kubelet's own
    self-registration doesn't reliably recreate the role label; a
    `k0scontroller` restart was needed to force re-registration promptly).

## Phase 5 — Steady-state fleet-key integration ◐ live-verified for the main claim; legacy-fallback path untested

- ☑ **Design recorded** as ADR-0003 Decision 5: `sceau serve` auto-detects
  a persisted fleet key (`fleet::load_fleet_key` succeeding) and prefers it
  for `Encrypt` and as `Decrypt`'s first choice, checked once at startup —
  no explicit flag. When found, the per-node deterministic SRK is *also*
  kept alive as a `Decrypt`-only fallback (never `Encrypt`), so ciphertext
  sealed before a node joined the fleet stays readable indefinitely.
- ☑ `src/tpm.rs`: `TpmSealer::from_primary` builds a sealer around an
  already-loaded handle (the fleet key) instead of recreating the
  deterministic SRK; `derive_key_id` extracted as a pure, unit-tested
  function shared by both paths (`src/tpm_tests.rs`, new — this file had no
  tests at all before this change).
- ☑ `src/kms.rs`: `KmsService::with_legacy_fallback` holds both sealers;
  `decrypt` tries the primary by `key_id`, then the legacy fallback if
  present and its `key_id` matches, before rejecting.
- ☑ `src/main.rs`: `build_sealer` implements the auto-detect-and-prefer
  logic, returning `(primary, Option<legacy>)`.
- ☑ `cargo build`/`cargo clippy`/`cargo test` green on real Linux hardware.
- ☑ **Found and fixed live**: `sealed_public()` hardcoded
  `fixedTpm=true`/`fixedParent=true`, which the TPM rejects
  (`TPM_RC_ATTRIBUTES`) for a child created under a `fixedTpm=false`
  (duplicable) parent — i.e. every `Encrypt` call under the fleet key was
  failing. Fixed by deriving the child's attributes from the primary's own
  `fixed_tpm()` bit (`TpmSealer.child_fixed`) instead of hardcoding them.
  Diagnosed via `/readyz/kms-providers?verbose` — the *aggregate*
  `/readyz?verbose` redacts KMS failure detail ("reason withheld"); the
  per-check sub-path does not.
- ☑ **Live verification, the actual cross-node HA claim this whole ADR
  exists for — confirmed bidirectionally**, on the real two-node test
  cluster, 2026-09-06 (UTC):
  - `15:29:22Z` — `sceau-ha-test` created via node1's apiserver, read back
    via **node2's** apiserver (a different physical TPM that never sealed
    it) — correct plaintext (`hello-from-node1`).
  - `15:44:27Z` — the reverse: `sceau-ha-test-2` created via node2's
    apiserver, read back via **node1's** — also correct
    (`hello-from-node2`).
  - Both directions ruling out a one-way fluke (e.g. one node's `sceau`
    silently falling back to its old per-node SRK for reads while the
    other genuinely used the fleet key).
- ☐ Live verification, still open: a Secret sealed *before* a node joined
  the fleet (under its old per-node SRK `key_id`) still decrypts correctly
  *after* joining, via the `KmsService::with_legacy_fallback` path — not
  yet exercised (the test cluster's two nodes both had the fleet key
  active before any real pre-join ciphertext existed to test against).

## Phase 6 — Hardening: EK-backed authenticity via TPM credential-activation attestation ◐ in progress

**Interim hardening landed 2026-09-09 (does not close the phase).** Two
independent gates were added ahead of the attestation work, because both are
cheap and neither depends on it:

- ☑ **Operator allowlist.** `enroll` now requires `--allow-node <NAME>`
  (repeatable) and rejects any joiner not named. Previously any identity
  naming a currently-existing `Node` was authorized — which every kubelet in
  the cluster satisfies, workers included. A node-role label check was
  considered and rejected: k0s controllers do not necessarily carry
  `node-role.kubernetes.io/control-plane`, and a controller without
  `--enable-worker` has no `Node` object at all.
- ☑ **Transport-key validation.** `fleet::validate_transport_key` rejects a
  joiner-supplied public area that is not a restricted RSA-2048 storage key
  with `fixedTpm`+`fixedParent`, before `LoadExternal`. The transport key now
  has its own non-duplicable template rather than reusing
  `duplicable_storage_public`. Live-verified against swtpm 0.7.1 via
  `make test-tpm`, including that `TPM2_Duplicate` still accepts a `fixedTpm`
  new parent.

**Design correction this phase must absorb before it is implemented.** As
written below, the EK/AK pair is "used only for this attestation challenge,
never for wrapping," and the transport key remains the duplication target.
Those two keys are then unbound: a joiner can pass attestation with a genuine
AK and still submit an unrelated transport key, and `TPM2_Duplicate` will wrap
the fleet key to it. `TPM2_MakeCredential`'s third parameter is an object
*Name*, so the challenge must be issued over the **transport key's** Name —
`make_credential(ek_public, secret, transport_key_name)` /
`activate_credential(transport_key_handle, ek_handle, ..)` — which is what
proves that specific key, with those specific attributes, is resident in that
specific TPM. Confirmed available in `tss-esapi` 7.7.0.

Note also that neither gate above proves TPM residency: the attributes are
fields in a submitted structure and a forged public area can set them freely.
They pin the template, which is precisely what makes a Name-based challenge
meaningful. That is why this phase is still open.

**Explicitly requested as a follow-up, not part of the initial cut** — the
gap this closes: the seed's mTLS/Node-identity check (Decision 3)
authenticates the *machine*, never proves the joiner's *TPM* is a genuine,
unmodified vTPM. **Design fully decided as an ADR-0003 Decision 3
addendum (2026-09-06)** — not `TPM2_PolicySecret` as originally scoped
here; the actual mechanism is standard TPM2 credential-activation
(`MakeCredential`/`ActivateCredential` + `Quote`), confirmed buildable
against `tss-esapi` 7.7.0's real API with no assumptions
(`Context::make_credential`, `Context::activate_credential`,
`Context::quote`, `abstraction::ek::retrieve_ek_pubcert`,
`abstraction::ak::create_ak_2`/`load_ak` all exist with the needed
signatures). Layered as a *second, independent gate* alongside the
existing mTLS/Node check — both must pass, neither replaces the other —
and does **not** change Decision 2's duplication target: the fresh
transport key stays the `TPM2_Duplicate` wrapping target; the EK/AK pair
is used only for this attestation challenge, never for wrapping.

Protocol (see ADR-0003 Decision 3's addendum for the full 7-step version):
joiner sends its EK cert + AK public/name → seed validates the EK cert
chains to vCenter's VMCA root → seed wraps a random secret to the AK via
`make_credential` → joiner recovers it via `activate_credential` (proves
AK/EK hardware binding) → joiner also sends a PCR `quote` signed by the AK
→ seed accepts only if the secret matches, the quote signature verifies,
and PCRs match a configured allowlist.

- ☐ `src/certs.rs` or a new module: EK cert retrieval + VMCA chain
  validation (**new operational input**: the seed needs vCenter's VMCA
  root cert locally, delivered via the same `VMImageSpec.cloud_configs`
  channel as k0s's CA and the seed address).
- ☐ `src/fleet.rs` or a new module: AK creation/loading
  (`create_ak_2`/`load_ak`), the seed's `make_credential` step, the
  joiner's `activate_credential` + `quote` steps.
- ☐ New request/response messages on the existing `enroll.v1` proto for
  this exchange — same mTLS channel as the duplicate RPC, not a new
  transport.
- ☐ PCR allowlist definition + delivery mechanism (**new operational
  input, ongoing cost**: every kernel/firmware/bootloader update to the
  golden image shifts PCR values; the allowlist must track that or real
  joiners start failing attestation).
- ☐ Implement + live-verify on the two-node test fleet.

**This phase is genuinely optional for the migration to proceed** — ADR-0003
already accepted shipping without it, with the gap named explicitly rather
than hidden. Sequence it opportunistically, not as a hard blocker on
Phases 7-9, unless the security posture demands otherwise before any real
production migration.

## Phase 7 — Operational safety nets ◐ in progress

Small, independent items already named as gaps across the ADR and
migration doc; each is its own short unit of work.

- ☑ **`sceau status`, live-verified** (2026-09-07): reports
  `fleet_key=<bool> key_id=<id>` to stdout, exits, no cluster/network
  interaction. Migration doc §5's first open item — closes the migration
  guide's own §2c confirmation step (no longer requires reading
  `sceau.service`'s startup log). Ran on both real test nodes:
  `fleet_key=true key_id=sceau-8bfca9a52b4f9897`, identical.
  - **A real bug was found and fixed by adding this**: `TpmSealer`'s
    `Drop` impl called `TPM2_FlushContext` unconditionally, including on
    an already-persistent (fleet key) handle — its own comment claimed
    this was harmless, but the TPM actually rejects it
    (`TPM_RC` 0x1c4). This was already happening silently on every
    `sceau serve` shutdown using the fleet key; `status` (which drops its
    `TpmSealer` immediately instead of holding it for a whole process
    lifetime) made it impossible to miss. Fixed: `TpmSealer` now tracks
    whether it owns a transient primary and only flushes in that case.
    See `.claude/CHANGELOG.md`.
- ☐ Accidental double-`genesis` guard (ADR-0003 Consequences) — a
  well-known marker path/label check, or at minimum a loud runbook
  callout, before this is used on a real migration.
- ☐ Key rotation runbook — a suspected fleet-key compromise currently has
  no designed recovery path (ADR-0003 Consequences, migration doc §5).
- ☑ `EvictControl`'s actual behavior against an already-occupied persistent
  handle — resolved as part of Phase 3's live round-trip (2026-09-07):
  fails loudly (`TPM_RC_NV_DEFINED`), does not silently overwrite. `join`
  now handles this correctly (evicts first); `fleet.rs`'s `persist()` doc
  comment documents the confirmed behavior. See Phase 3 above.

## Phase 8 — Kairos sysext packaging + banlieue integration ☐ not started

The stated end goal — "once we have this, then we will make it a kairos
sysext and will be on the nodes as part of creation." Depends on Phases
2-4 being live-verified first; this phase is about *delivery*, not new
`sceau` logic.

- ☐ Build `sceau` as a proper Kairos sysext (raw OCI artifact,
  `application/vnd.kairos.sysext.raw` — same mechanism as the unrelated
  `vm-build` sysext fix from this same engagement, different payload).
- ☐ `contrib/systemd/sceau-genesis.service` / `sceau-join.service` (oneshot
  units, sketched in `docs/migration-ha-existing-cluster.md` §3 but not
  shipped as tracked files yet — deferred until Phase 3 makes them
  meaningful to test).
- ☐ Wire sysext delivery into the Kairos cloud-config that also carries
  `sceau`'s `EncryptionConfiguration` (already-established delivery
  channel — `VMImageSpec.cloud_configs`, per ADR-0003 Decision 3).
- ☐ Retire `scripts/deploy-test-nodes.sh`'s manual SSH-copy path as the
  *primary* deployment method once the sysext lands — keep the script
  around for ad hoc debugging, but node creation should no longer depend
  on it.

## Phase 10 — `k0smotron`/CAPI hosted control planes ☐ not started, newly identified

**Not yet designed anywhere — surfaced while planning beyond the
VM-per-node topology, not carried over from any prior discussion.**
`k0smotron` runs etcd and `kube-apiserver` as **Pods** on a *management*
cluster, backing a *guest* cluster's control plane — a fundamentally
different topology from every phase above, which all assume `sceau` runs
as a host process on the same VM whose vTPM it seals against. This phase
exists to name the boundary problems explicitly, per the request that
motivated it, not to solve them all here.

**Boundary problem 1 — a naive port is a real security regression, not
just a packaging detail.** The tempting-looking approach — hostmount
`/dev/tpmrm0` straight into the etcd/apiserver pod — gives that pod raw,
unmediated TPM2 command access, bypassing every authorization `sceau`
does today entirely. A pod with the raw device can issue arbitrary TPM2
commands, including ones outside anything `sceau`'s own gRPC surface
would ever allow. This is not an acceptable default; it needs to be
named as a regression, not shipped as if it were equivalent to the
current host-process model.

- ☐ **Stronger alternative, to design and default to:** never hostmount
  the raw device into the pod at all. Run `sceau` as a **DaemonSet** —
  one instance per management-cluster node, privileged/host-network
  exactly as it runs today — and hostPath-mount only its already-narrow
  **unix socket** (`/run/sceau/sceau.sock`) into the etcd/apiserver pod.
  The pod becomes a KMS v2 client on that node, identical in shape to how
  `kube-apiserver` already talks to `sceau` today — just containerized.
  Zero raw-TPM exposure to the pod; the entire existing authorization
  model (Decision 3's mTLS + Node-identity check, for enrollment; the
  KMS v2 gRPC surface itself, for steady-state) is preserved unchanged.
- ☐ Evaluate whether a Kubernetes **device plugin** (advertising
  something like `sceau.io/kms-socket` as an extended resource, gating
  which pods can even request the mount via pod-spec resource limits +
  RBAC on who can create such pods) is worth the extra implementation
  effort over a plain hostPath volume — a stronger admission-time gate
  than relying solely on PSA/OPA policy to restrict hostPath mounts, but
  real new scope. Not decided; record the decision once made.

**Boundary problem 2 — pod rescheduling breaks the VM-per-node
assumption every phase above relies on, and makes Phases 2-4 a hard
requirement, not an HA nicety, for this topology specifically.** A
guest cluster's etcd/apiserver pod is not permanently bound to one node
the way a `banlieue`-provisioned VM is to its own vTPM — Kubernetes can
and does reschedule pods to a different node, with a different vTPM and
a different `key_id`, as a routine operation (node drain, eviction,
rebalancing), not a rare event. Without the fleet-key mechanism from
Phases 2-4, a pod rescheduled to a new node would be unable to decrypt
anything it sealed on the old one — the exact failure mode ADR-0003
exists to fix, except triggered by ordinary Kubernetes scheduling instead
of a control-plane node going down. **This phase cannot start before
Phases 2-4 are live-verified; it is not an independent feature, it is a
consumer of them.**

**Boundary problem 3 — which cluster's PKI authenticates enrollment, and
which cluster does the fleet key belong to?** The `sceau` DaemonSet lives
on the *management* cluster's nodes; the etcd Pod it serves belongs to a
*guest* cluster's control plane. ADR-0003 Decision 3's authorization check
(peer's mTLS identity → look up a matching `Node` object) was designed
assuming both live in the same cluster — that assumption doesn't hold
here. Per the per-cluster scoping decision recorded in ADR-0003 Decision
1, **each guest cluster still needs its own independent fleet key** — but
that means a single `sceau` DaemonSet instance (one per management-cluster
node) must be able to hold and serve **multiple different guest clusters'
fleet keys simultaneously**, depending on which guest clusters currently
have a pod scheduled on that node. This is a real, new multi-tenancy
requirement `sceau`'s current design (one fleet key per process) does not
support at all — not a detail to discover during implementation.

- ☐ Decide: does `genesis`/`enroll`/`join` operate per-*guest-cluster*
  (keyed by guest-cluster identity, however that's namespaced/labeled on
  the management cluster) rather than per-node? If so, `fleet.rs`'s
  `load_fleet_key`/`persist` need a keying scheme beyond "the one
  persistent handle this process owns."
  Both are real design changes, not a config flag.
- ☐ Decide which cluster's `Node` objects the seed checks against for
  authorization — the management cluster's (the actual TPM-holding
  nodes) or the guest cluster's (the identity the etcd Pod is acting on
  behalf of)? These can disagree, and the answer determines what the
  authorization check is actually vouching for.
- ☐ **This almost certainly needs to be recorded as ADR-0004, not folded
  into ADR-0003 as another Decision** — the multi-tenant-fleet-key and
  cross-cluster-authorization questions above are a materially different
  shape of problem from anything ADR-0003 decided, per the same judgment
  call already applied to Phase 6. Write it once boundary problems 1-3
  have concrete answers, not before.

**This phase is deliberately left mostly as open questions.** The point
of adding it now is to make sure the boundary problems are named and
gated correctly (behind Phases 2-4, behind a new ADR) rather than
discovered mid-implementation — not to pretend they're solved.

## Phase 9 — Full end-to-end HA migration validation ☐ not started

The actual payoff — validating the whole design against a real HA
migration, per `docs/migration-ha-existing-cluster.md`.

- ☐ Phase A (per-node TPM key prep) on both `k0s-node1`/`k0s-node2`.
- ☐ Phase B (`EncryptionConfiguration` cutover: push to all nodes, rolling
  `k0scontroller` restart, re-encrypt pre-existing Secrets).
- ☐ Confirm a Secret written through one node's apiserver reads correctly
  through the other's — the actual property this whole ADR exists to
  deliver.
- ☐ Confirm `--encryption-provider-config-automatic-reload` works against
  the real k0s build in use (migration doc §4c flagged this as unverified;
  if it works, it meaningfully simplifies Phase B for every future cutover).
- ☐ Write up the validated result (mirroring how
  `docs/architecture/banlieue-vtpm-kairos-k0s-encryption.md` §5a documents
  the single-node `sceau` validation) — this is the evidence that closes
  out this entire roadmap.
