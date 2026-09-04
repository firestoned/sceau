# Documentation Standards

## Before Marking Any Task Complete

ALWAYS ask: "Does documentation need to be updated?"

Applies to: code changes, KMS proto changes, TPM behavior changes, configuration changes, architecture changes.

---

## Documentation Update Workflow

1. **Analyze the change**: user-facing impact? architectural implications? new APIs/config?
2. **Update in this order:**
   - `.claude/CHANGELOG.md` (see `update-changelog` skill — `**Author:**` is MANDATORY)
   - `README.md` — getting-started, build/run instructions, k0s / Kairos configuration snippets
   - `docs/` — ADRs and CALM model for architectural changes
   - Architecture diagrams (regenerate with `make calm-diagrams`) if structure changed
3. **Verify:** read docs as a new user, validate all YAML/config examples against the real flags (`--socket`, `--tcti`)

---

## What to Update by Change Type

**KMS service changes** (`src/kms.rs`, `proto/kms/v2/api.proto`):
- Document behavior changes in `README.md` (status/encrypt/decrypt semantics)
- Update the key_id / rotation notes if `key_id` derivation changes

**TPM logic changes** (`src/tpm.rs`):
- Update the "How it works" and "Security notes" sections of `README.md`
- Record the decision in an ADR if the TPM object model changes (per `rules/architecture-driven-development.md`)

**Deployment changes** (systemd unit, k0s `encryption.conf`, Kairos notes):
- Keep `README.md` examples in sync — they are the only deployment docs today
- Verify every flag in an example exists in `src/main.rs`'s `Args`

**Bug fixes:**
- Update troubleshooting notes in `README.md` with the failure mode and fix

---

## Configuration Examples Must Match the Code

ALWAYS verify flags, paths, and field names against `src/main.rs` and
`proto/kms/v2/api.proto` before writing examples. NEVER guess.

```yaml
# ❌ WRONG - guessed KMS API version
providers:
  - kms:
      apiVersion: v1

# ✅ CORRECT - matches proto/kms/v2/api.proto
providers:
  - kms:
      apiVersion: v2
```

---

## Changelog Requirements

Every entry in `.claude/CHANGELOG.md` MUST have `**Author:**` — no exceptions.

Format:
```markdown
## [YYYY-MM-DD HH:MM] - Brief Title

**Author:** <Name of requester or approver>

### Changed
- `path/to/file.rs`: Description of the change

### Why
Brief explanation.

### Impact
- [ ] Breaking change
- [ ] Requires daemon restart / re-encryption migration
- [ ] Config change only
- [ ] Documentation only
```

---

## Code Comments

All public functions and types MUST have rustdoc comments:

```rust
/// Seals a data encryption key under the TPM storage root key.
///
/// # Arguments
/// * `plaintext` - The DEK bytes to seal (max [`MAX_SEAL_DATA`] bytes)
///
/// # Errors
/// Returns `TpmError::PlaintextTooLarge` if the DEK exceeds the TPM's
/// sealed-data capacity, or `TpmError::Tss` on any TPM command failure.
pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, TpmError> {
```

---

## Validation Checklist

- [ ] `.claude/CHANGELOG.md` updated with `**Author:**`
- [ ] `README.md` examples still match the actual CLI flags and KMS proto
- [ ] Architecture diagrams regenerated (`make calm-diagrams`) if structure changed
- [ ] `make calm-validate` passes
