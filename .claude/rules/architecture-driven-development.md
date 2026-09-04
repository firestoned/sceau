# Architecture Driven Development (ADD)

> **ADD is the governing methodology for sceau.** Architecture is designed,
> recorded, and visualized **before** code is written. ADRs and CALM diagrams
> are first-class deliverables — equal in importance to the code and the tests.

ADD layers *on top of* the existing TDD discipline (`rules/testing.md`); it does
not replace it. The order is fixed:

```
ADR  →  CALM  →  TDD  →  implement  →  docs
```

## The ADD cycle

For any **architecturally significant** change, complete each step before
starting the next:

### 1. ADR — decide and record (FIRST)

Write or update an Architecture Decision Record in
`docs/adr/NNNN-title.md` (lowercase-hyphen, zero-padded sequential number)
with the standard sections:

- **Status** — Proposed → Accepted (→ Superseded by NNNN)
- **Context** — the forces, constraints, and the problem being solved
- **Decision** — what we will do, stated plainly
- **Consequences** — trade-offs, follow-ups, what this rules out

Keep ADRs in the repo (unlike roadmaps, which live outside it). One decision per
ADR. If a change reverses an earlier ADR, mark the old one *Superseded* and link
forward.

### 2. CALM — model and visualize

Update the FINOS CALM architecture model
(`docs/architecture/calm/architecture.json`) to reflect the decision: nodes,
relationships, interfaces, controls, and flows. Then:

```sh
make calm-validate     # architecture conforms to the meta-schema (hard gate)
make calm-diagrams     # regenerate Mermaid diagrams into docs/architecture/
```

The architecture must be modeled and the diagrams must render cleanly **before**
implementation begins. A change that isn't reflected in CALM isn't designed yet.

### 3. TDD — red / green / refactor

Only now write code, tests first, per `rules/testing.md` and the `tdd-workflow`
skill: failing test → minimum implementation → refactor. After any `.rs` change,
run the `cargo-quality` skill.

### 4. Docs

Update `.claude/CHANGELOG.md` (with `**Author:**`), the `README.md`, and any
affected `docs/` pages / examples, per `rules/documentation.md`.

## When does ADD apply?

**Full ADR + CALM** (architecturally significant):

- New TPM object types or key hierarchies (e.g. ECC SRK, child keys, PCR policies)
- Changes to the KMS contract (`proto/kms/v2/api.proto`, envelope format, key_id derivation)
- New deployment topology (static pod vs systemd unit, socket placement, Kairos bundling)
- Cross-cutting concerns: security boundaries, failure domains (TPM loss, socket exposure)
- Any decision where "why A over B" is worth recording

**TDD only** (no ADR/CALM needed):

- Typos, comment/doc tweaks, formatting
- Isolated bug fixes with no architectural impact
- Mechanical refactors that preserve behavior and structure

> When unsure whether a change is "architectural," **write the ADR.** A short,
> slightly-redundant ADR costs little; an undocumented architectural decision
> costs the next person a re-derivation.

## Checklist (paste into the work)

- [ ] ADR written/updated in `docs/adr/NNNN-*.md` (Status/Context/Decision/Consequences)
- [ ] CALM model updated; `make calm-validate` passes; `make calm-diagrams` renders
- [ ] Tests written **first**, then implementation (TDD)
- [ ] `cargo-quality` passes (fmt + clippy + test)
- [ ] CHANGELOG + docs updated
