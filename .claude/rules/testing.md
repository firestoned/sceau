# Testing Standards

## CRITICAL: Test-Driven Development (TDD) Workflow

**MANDATORY: ALWAYS write tests FIRST before implementing functionality.**

This project follows strict Test-Driven Development practices. You MUST follow the Red-Green-Refactor cycle for ALL code changes.

> **How:** Follow the `tdd-workflow` skill (RED → GREEN → REFACTOR).

### When to Write Tests First

- ✅ **New Features**: Write tests defining the feature behavior, then implement
- ✅ **Bug Fixes**: Write a failing test that reproduces the bug, then fix it
- ✅ **Refactoring**: Ensure existing tests pass, add new tests for edge cases
- ✅ **Envelope/format changes**: Write tests that pin the byte layout, then change it

### Exceptions to TDD

TDD is MANDATORY except for:
- Exploratory/prototype code (must be marked as such and removed before merging)
- Simple refactoring that doesn't change behavior (existing tests verify correctness)

**REMEMBER**: If you're writing implementation code before tests, STOP and write tests first!

---

## After Modifying Any `.rs` File

**CRITICAL: At the end of EVERY task that modifies Rust files, run the `cargo-quality` skill.**

> **How:** Run the `cargo-quality` skill. Fix ALL clippy warnings. Task is NOT complete until all three commands pass.

**CRITICAL: After ANY Rust code modification, you MUST also verify:**

1. **Function documentation is accurate**:
   - Check rustdoc comments match what the function actually does
   - Verify all `# Arguments` match the actual parameters
   - Verify `# Errors` describes all error cases

2. **Unit tests are accurate and passing**:
   - Check test assertions match the new behavior
   - Add new tests for new behavior/edge cases

3. **End-user documentation is updated**:
   - Update `README.md` if flags, behavior, or config examples changed
   - Ensure `.claude/CHANGELOG.md` reflects the changes

---

## Unit Testing Requirements

**MANDATORY: Every public function MUST have corresponding unit tests.**

### Test Quality Standards

- Use descriptive test names (e.g., `test_seal_rejects_oversized_plaintext`)
- Follow the Arrange-Act-Assert pattern
- Test error conditions, not just happy paths
- Ensure tests are deterministic (no flaky tests)

### Test File Organization

**CRITICAL: ALWAYS place tests in separate `_tests.rs` files, NOT embedded in the source file.**

This is the **required pattern** for this codebase. Do NOT embed tests directly in source files.

**Correct Pattern:** `src/foo.rs` → declare `#[cfg(test)] mod foo_tests;` at the bottom; `src/foo_tests.rs` → `#[cfg(test)] mod tests { use super::super::*; ... }`.

> **See:** `tdd-workflow` skill for the full file pattern and Arrange-Act-Assert examples.

**Examples in This Codebase:**
- `src/tpm.rs` → `src/tpm_tests.rs`
- `src/kms.rs` → `src/kms_tests.rs`
- `src/main.rs` → `src/main_tests.rs`

### What Can Be Unit-Tested Without a TPM

Pure functions are fully unit-testable on any machine:
- `envelope_encode` / `envelope_decode` round-trips, version rejection, truncated-input rejection
- `PlaintextTooLarge` boundary checks
- key_id formatting

TPM-backed methods (`seal`, `unseal`, `TpmSealer::new`) need a TPM resource
manager or simulator — those belong in integration tests gated behind a TCTI
environment variable, never in unit tests that CI runs without hardware.

---

## Integration Tests

Place in `/tests/` directory:
- Use a **swtpm** software TPM (`swtpm socket`) or skip cleanly when unavailable
- Take the TCTI from the environment (`SCEAU_TCTI`, e.g. `swtpm:host=127.0.0.1,port=2321`) and `#[ignore]` by default — see `rules/no-real-infrastructure.md`
- Test seal→unseal round-trips, wrong-key_id rejection, restart persistence (same SRK recreated → same ciphertext decrypts)
- Never require real hardware in CI

---

## Test Execution

> **How:** Run the `cargo-quality` skill. For a specific module: `cargo test --lib <module_path>`. For verbose output: `cargo test -- --nocapture`.

**ALL tests MUST pass before code is considered complete.**
