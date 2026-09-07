# sceau ADR-0005 (tentative) — Attestation-Gated Cluster Join — Roadmap

**Author:** Erick Bourgeois
**Source decision:** not yet written. This roadmap's Phase 0 is writing it.
**Tentative ADR number, stated explicitly:** ADR-0003's own roadmap (Phase
10, boundary problem 3) already provisionally reserves **ADR-0004** for the
`k0smotron`/CAPI multi-tenant-fleet-key topic. This work is unrelated to
that one (it doesn't touch multi-tenancy or hosted control planes at all),
so it takes the next number, **ADR-0005** — confirm this is still free when
the ADR is actually written, in case something else lands first.
**Repo(s) in scope:** `firestoned/sceau` for the new subcommand(s) and
protocol. **Crosses into `firestoned/banlieue`** (or wherever k0s join
tokens are actually minted today — `scripts/bootstrap-k0s-cluster.sh`, or
an operator's manual `k0s token create` step) for Phase 3, since gating
admission means changing *who is allowed to mint a join token*, not just
adding a new `sceau` capability in isolation. Stated explicitly per this
project's roadmap convention, not assumed.

> Status legend: ☐ not started · ◐ in progress · ☑ done.
> Effort: S ≤ half day · M ≤ 2 days · L > 2 days.

---

## Ordering rationale

This entire roadmap **consumes** ADR-0003 Phase 6's attestation primitives
(`MakeCredential`/`ActivateCredential`/`Quote`, EK cert chain validation,
PCR allowlist checking) rather than reimplementing them — the crypto is
identical; only the *reward* for a passing attestation differs (a k0s join
token here, a duplicated fleet key there). Phase 1 below is therefore a
hard dependency gate, not busywork: starting this roadmap's real
implementation before ADR-0003 Phase 6 is live-verified would mean building
against attestation code that has never been run against real hardware,
repeating exactly the kind of live-hardware-driven bug hunt ADR-0003's own
roadmap has needed twice already (`TPM_RC_AUTH_TYPE`, `TPM_RC_POLICY_FAIL`).

After that, the ordering follows the same shape as ADR-0003's own roadmap:
design the privilege/trust model first (this is the highest-risk-to-get-
wrong part — see Phase 0), then the new subcommand surface, then the actual
integration point that makes the gate *real* (Phase 3 — without it, Phase 2
is a capability nobody depends on), then live verification of both the
positive and negative paths, then operational hardening.

---

## Phase 0 — Design ☐ not started

- ☐ Write ADR-0005 (confirm the number is still free first). Record,
  explicitly, the decision already reasoned through in conversation: this
  is a **pre-join gate on token issuance** (an unattested node cannot
  obtain a join token at all), not a **post-join revoke** (let it join,
  check after, cordon on failure) — the two have materially different
  security properties and this roadmap assumes the stronger one. If a
  future design chooses the weaker one instead, that's a real reversal
  worth its own ADR status change, not a silent scope-narrowing here.
- ☐ Decide the privilege model for whatever runs the new subcommand's
  server side (tentatively called the "join broker" throughout this
  roadmap — not a final name). It needs the ability to mint a k0s join
  token (or call whatever API does) — a materially bigger trust boundary
  than ADR-0003's `enroll`, which only ever needed read access to `Node`
  objects. Record explicitly what this process is trusted to do and why
  that trust is bounded (e.g. one-shot, bounded by count/timeout, the same
  `--max`/`--timeout-secs` shape `enroll` already uses — or a different
  shape, if reasoning suggests one).
- ☐ Decide the bootstrap/chicken-and-egg answer for the very first node(s)
  in a cluster — there is no existing member to attest *to* yet, the same
  shape of gap ADR-0003 already names for `genesis`'s own bootstrap
  window. Does the first node skip attestation entirely (an explicit,
  documented exception), or does something else vouch for it?
- ☐ Decide whether this is genuinely a new ADR, or folds into ADR-0003 as
  a new Decision after all, once the privilege-model writeup above is done
  — the reasoning for "new ADR" (privilege escalation + a materially
  different trust boundary than fleet-key duplication) was sound in
  conversation, but confirm it still holds once written down formally,
  not just asserted.
- ☐ CALM model update once the above is decided — new node/relationship
  for the join-broker role and whatever it calls to mint tokens.

## Phase 1 — Dependency gate: ADR-0003 Phase 6 ☐ blocked

- ☐ Do not start Phase 2 until ADR-0003's `adr-0003-fleet-key-ha-roadmap.md`
  Phase 6 (`MakeCredential`/`ActivateCredential`/`Quote`, EK cert chain
  validation, PCR allowlist) is implemented **and live-verified** on real
  hardware. This phase exists purely to make that dependency explicit and
  checkable, not to duplicate Phase 6's own tracking.

## Phase 2 — New `sceau` subcommand(s) ☐ not started

- ☐ Name and shape the subcommand(s) — tentatively a server side (e.g.
  `sceau attest --listen=<addr>`, run by an already-trusted, already-a-
  member node) and a client side (run by the node trying to join, before
  it ever invokes `k0s install controller`). Naming, flags, and whether
  this reuses `enroll`'s `--max`/`--timeout-secs` shape are open — decide
  here, not by silent convention-copying from `enroll`.
- ☐ New proto/service, or new messages on the existing `enroll.v1` proto —
  decide during design (Phase 0 should have an opinion by the time this
  starts; if not, decide here). The attestation *sub-protocol itself*
  (EK cert + AK public/name → wrapped credential → recovered secret +
  quote) can very likely be shared verbatim with ADR-0003 Phase 6's
  messages; only the final response (a join token here, vs. nothing extra
  there — ADR-0003's reward is the separate `Duplicate` RPC) differs.
- ☐ Implement the server side: on a passing attestation, mint (or request)
  a k0s join token and return it. On failure, return nothing — the
  attestation exchange itself already proves nothing about cluster
  identity, so there's no `Node`-existence check to run here the way
  `enroll` runs one; the token grant *is* the entitlement decision.
- ☐ Implement the client side: run the attestation exchange, receive a
  token, **then** invoke k0s's own join (`k0s install controller
  --token-file=...`) using it — this is a new orchestration step, not
  something `sceau` has ever driven before (today `sceau join` runs
  *after* k0s's own join completes; this reverses that ordering for the
  new subcommand specifically).

## Phase 3 — Token-issuance integration (crosses into `banlieue`/join
tooling) ☐ not started

**This phase is what makes the gate real — without it, Phase 2 is a
capability nobody depends on, and an operator can still mint a token the
old way and skip attestation entirely.**

- ☐ Identify every place a k0s join token is minted today for a cluster
  meant to use this gate (`banlieue`'s `scripts/bootstrap-k0s-cluster.sh`,
  and/or an operator's manual `k0s token create`) — not assumed, actually
  checked, since this roadmap doesn't yet know if there's more than one.
- ☐ Decide how those call sites stop minting tokens directly and instead
  go through the new join-broker subcommand — this is a real workflow
  change to whatever automation exists there, tracked as its own item
  in that repo's own roadmap/issue tracker once scoped, not silently
  patched in from here.
- ☐ Decide what happens to the *old* direct-mint path — disabled outright
  for gated clusters, or left available as an explicit escape hatch
  (weakens the guarantee if so; record the trade-off if this is the
  choice).

## Phase 4 — Live verification ☐ not started

Same discipline as ADR-0003's own Phase 3/4 — a positive-path pass alone
is not enough; a roadmap item is not done because the code compiles.

- ☐ Positive path: a node that passes attestation receives a token and
  successfully joins as a real cluster member.
- ☐ **Negative path, explicitly** (matching ADR-0003 Phase 4's own
  discipline of testing rejection directly, not just assuming it works
  because the positive path did): a node that fails attestation (wrong/
  missing EK cert chain, failed credential activation, or a PCR mismatch)
  is provably refused a token and never becomes a `Node` at all — confirm
  by checking the cluster's `Node` list, not just the client's own exit
  code.
- ☐ Confirm the bootstrap/chicken-and-egg answer from Phase 0 actually
  works against a real first-node scenario, not just on paper.

## Phase 5 — Operational hardening ☐ not started

- ☐ PCR allowlist delivery/maintenance — **shared operational concern with
  ADR-0003 Phase 6**, do not re-design a second allowlist mechanism here;
  reference/reuse whatever Phase 6 ships.
- ☐ Runbook for a legitimate node failing attestation (e.g. after a
  routine kernel/firmware update shifted its PCR values) — an operator
  unlock/override path, not designed anywhere yet.
- ☐ Decide whether the join-broker role itself needs its own audit trail
  (who attested, when, pass/fail) beyond whatever the underlying token-
  minting API already logs — not assumed sufficient, checked.
