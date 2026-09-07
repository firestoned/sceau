# Roadmaps Live In This Repo

Unlike `banlieue` (whose roadmaps are deliberately kept **outside** its
repo, in a private directory the maintainer keeps elsewhere), `sceau`'s
roadmaps are tracked **inside this repository**, under
`.github/community/`. This is a deliberate difference between the two
projects — do not "fix" it to match `banlieue`'s convention, and do not
create a roadmap directory outside this repo for `sceau` work.

## Where things go

- **`ROADMAPS.md`** (repo root) — the index. One row per roadmap doc: what
  it tracks, current status. Update this whenever a roadmap doc is added or
  its status changes materially (a phase completes, a new phase is added).
- **`.github/community/NNNN-title-roadmap.md`** — one file per ADR (or
  tightly related group of ADRs) being implemented. Name it after the ADR
  it tracks, e.g. `adr-0003-fleet-key-ha-roadmap.md` for
  `docs/adr/0003-fleet-key-duplication-for-ha-multi-controller.md`.

## What a roadmap doc contains

Every roadmap follows the same shape (see `adr-0003-fleet-key-ha-roadmap.md`
for a full worked example):

- **Header**: author, the ADR(s) it tracks, which repo(s) are in scope (most
  work here is single-repo, but note explicitly when a phase reaches into
  `banlieue`/`vm-build`/another sibling project instead of assuming it).
- **Status legend and effort scale**, stated once at the top:
  `☐ not started · ◐ in progress · ☑ done`, effort `S/M/L`.
- **An "Ordering rationale" section** explaining *why* the phases are
  sequenced the way they are — not just a flat checklist. If a later phase
  is gated on an earlier one's live verification, say so explicitly as a
  **Gate:** line at the end of the earlier phase.
- **Numbered phases**, each phase a `##` heading with its own status marker
  in the heading itself (e.g. `## Phase 2 — TPM2_Duplicate/Import
  primitives ◐ in progress`), followed by a checkbox list of concrete,
  independently-completable items.
- **Newly-identified gaps get called out as such**, not silently folded in
  as if they were always planned — e.g. "not yet designed anywhere —
  surfaced while writing this roadmap, not carried over from any prior
  discussion." This is what makes a roadmap trustworthy as a status report,
  not just an aspirational plan.
- **Live-verification items are their own checkbox items**, distinct from
  "code written" items — this project's own TPM/crypto work has already
  needed real-hardware-driven fixes that no amount of documentation
  research caught in advance (see Phase 2's own history in the ADR-0003
  roadmap). A roadmap item is not "done" because the code compiles; TPM-
  touching code is "done" only once it's been run against real hardware or
  a `swtpm` simulator (per `rules/testing.md`).

## When to create a new roadmap vs. update an existing one

- A new ADR that's architecturally significant enough to need its own
  phased implementation plan (per `rules/architecture-driven-development.md`'s
  own "full ADR + CALM" criteria) gets its own roadmap doc.
- A revision to an existing ADR (like ADR-0003's same-day revisions) updates
  the *existing* roadmap's relevant phase(s) — do not fork a second roadmap
  for the same ADR.
- A small, isolated bug fix or refactor with no architectural impact does
  not need a roadmap entry at all, same as it doesn't need an ADR.

## The no-real-infrastructure rule still applies here, fully

`sceau` is a public OSS repository — roadmap docs are exactly as subject to
`rules/no-real-infrastructure.md` as any other tracked file. Real test-node
hostnames, IPs, or account identifiers must never appear in a roadmap doc,
including short/bare hostnames without a domain suffix — use the
placeholder patterns from that rule (e.g. `k0s-node1`/`k0s-node2` for a
generic two-node test fleet, never the maintainer's actual machine names).
