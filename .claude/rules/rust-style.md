# Rust Style Guide

## Core Principles

- Use `thiserror` for error types, not string errors
- Prefer `anyhow::Result` in the binary (`main.rs`), typed errors in library-style modules (`tpm.rs`, `kms.rs`)
- Use `tracing` for logging, not `println!` or `log`
- Async functions should use `tokio`
- **No magic numbers**: Any numeric literal other than `0` or `1` MUST be declared as a named constant
- **Use early returns/guard clauses**: Minimize nesting by handling edge cases early and returning

---

## Early Return / Guard Clause Pattern

**CRITICAL: Prefer early returns over nested if-else statements.**

The "early return" or "guard clause" coding style emphasizes minimizing nested if-else statements and promoting clearer, more linear code flow. This is achieved by handling error conditions or special cases at the beginning of a function and exiting early if those conditions are met.

### Key Principles

1. **Handle preconditions first**: Validate input parameters and other preconditions at the start of a function. If a condition is not met, return immediately (e.g., `return Err(...)`, `return None`, or `return Ok(())`).

   ```rust
   // ✅ GOOD - Early return for validation
   pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, TpmError> {
       // Guard clause: reject oversized input before touching the TPM
       if plaintext.len() > MAX_SEAL_DATA {
           return Err(TpmError::PlaintextTooLarge(plaintext.len()));
       }

       // Main logic continues here (happy path)
       let sensitive = SensitiveData::try_from(plaintext.to_vec())?;
       // ...
   }

   // ❌ BAD - Nested if-else
   pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, TpmError> {
       if plaintext.len() <= MAX_SEAL_DATA {
           let sensitive = SensitiveData::try_from(plaintext.to_vec())?;
           // ...
       } else {
           Err(TpmError::PlaintextTooLarge(plaintext.len()))
       }
   }
   ```

2. **Minimize else statements**: Instead of using if-else for mutually exclusive conditions, use early returns within if blocks.

3. **Use `?` for error propagation**: Rust's `?` operator is a form of early return for errors. Use it liberally to keep the happy path unindented.

   ```rust
   // ✅ GOOD - Early error returns with ?
   pub fn unseal(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, TpmError> {
       let (public, private) = envelope_decode(ciphertext)?;
       let data = self.context.execute_with_nullauth_session(|ctx| {
           let handle = ctx.load(self.srk, private, public)?;
           let data = ctx.unseal(handle.into())?;
           ctx.flush_context(handle.into())?;
           Ok::<_, TpmError>(data)
       })?;
       Ok(data.value().to_vec())
   }
   ```

### When to Use

- Input validation at function start (size limits, envelope version checks)
- Checking preconditions before expensive TPM operations
- Handling special cases before the general case
- Error handling in async gRPC handlers

---

## Magic Numbers Rule

**CRITICAL: All numeric literals (except 0 and 1) MUST be named constants.**

A "magic number" is any numeric literal (other than `0` or `1`) that appears directly in code without explanation.

### Rules

- **`0` and `1` are allowed** - These are ubiquitous and self-explanatory
- **All other numbers MUST be named constants** - No exceptions
- Use descriptive names that explain the *purpose*, not just the value

### Examples

```rust
// ✅ GOOD - Named constants
const MAX_SEAL_DATA: usize = 128;      // TPM2B_SENSITIVE_DATA cap (MAX_SYM_DATA)
const ENVELOPE_VERSION: u8 = 1;        // ciphertext envelope format version
const KEY_ID_HEX_LEN: usize = 16;      // hex chars of the SRK-name hash in key_id
const SOCKET_MODE: u32 = 0o600;        // owner-only unix socket permissions

// ❌ BAD - Magic numbers
if plaintext.len() > 128 { /* why 128? */ }
out.push(1);                            // what is 1?
let id = hex::encode(digest)[..16].to_string();  // why 16?
```

### Special Cases

**Buffer sizes**: Always use named constants.

**Byte offsets in the envelope format**: Named constants or derived from
`ENVELOPE_HEADER_LEN`-style constants — the envelope format is a wire contract.

### Where to Define Constants

- **Module-level**: For constants used only within one file (e.g. `MAX_SEAL_DATA` in `tpm.rs`)
- **Crate-level**: For constants shared across modules — group them with documentation

### Verification

```bash
# Find numeric literals other than 0 and 1 in Rust files (excludes test files)
grep -Ern '\b[2-9][0-9]*\b' src/ --include="*.rs" --exclude="*_tests.rs" | grep -v '^[^:]*:[^:]*://.*$'
```

---

## Code Quality: Use Global Constants for Repeated Strings

When a string literal appears in multiple places across the codebase, it MUST be defined as a global constant and referenced consistently (socket paths, TCTI strings, KMS version strings, key_id prefixes).

```rust
// ✅ GOOD - Use constants
const DEFAULT_SOCKET_PATH: &str = "/run/sceau/sceau.sock";
const DEFAULT_TCTI: &str = "device:/dev/tpmrm0";
const KEY_ID_PREFIX: &str = "sceau-";

// ❌ BAD - Hardcoded strings repeated across main.rs / kms.rs / docs
```

---

## Dependency Management

Before adding a new dependency:
1. Check if existing deps solve the problem
2. Verify the crate is actively maintained (commits in last 6 months)
3. Prefer crates from well-known authors or the Rust ecosystem
4. Document why the dependency was added in `.claude/CHANGELOG.md`

---

## Code Comments

All public functions and types **must** have rustdoc comments with `# Arguments` / `# Errors` sections where applicable.

---

## Things to Never Do

- **Never** use `unwrap()` in production code - use `?` or explicit error handling
- **Never** log DEK plaintext, sealed key material, or TPM auth values — not even at `trace` level
- **Never** leak TPM error internals to the gRPC client beyond an error class (`Status::internal` with a generic message)
- **Never** block the tokio runtime on TPM I/O without serializing through the sealer `Mutex` — the TPM is a single-threaded resource
- **Never** store unsealed plaintext anywhere persistent — unsealed DEKs live in memory only, for the duration of one Decrypt call
