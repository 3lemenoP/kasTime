# KTCS-CLI Test Plan & Coverage Report

## Test Status

**Total tests: 130**, split into:
- Unit tests (`main.rs`): 36
- CLI basic tests: 29
- Config tests: 17
- Error handling tests: 23
- Network mock tests (wiremock): 17
- Live network tests: 8, each marked `#[ignore]`

**The hermetic suites (unit, basic, config, error, and wiremock mock tests)
pass** under `cargo test -p ktcs-cli`; they run with no network access.

**The 8 live-network tests are `#[ignore]` by default** and are only run
explicitly with `-- --ignored`. They require network connectivity to a real
Kaspa node (and the direct-stamp test can consume KAS), so they are not part of
the default or CI run.

> The live suite previously invoked flags that did not exist (e.g. a
> `--format json` flag on `wallet balance`); those argument errors have been
> corrected, so the live tests now match the actual CLI surface. The remaining
> gate on them is network/funds, not argument parsing.

### Running Tests

```bash
# Run all hermetic tests (excludes the #[ignore] live-network tests)
cargo test -p ktcs-cli

# Run the live-network tests explicitly (requires network; may consume KAS)
cargo test -p ktcs-cli --test cli_live_network -- --ignored
```

---

# Original Test Plan

## Summary
- **Modules analyzed**: 1 (main.rs, ~1,600 lines)
- **Existing test functions**: 0
- **Behaviors identified**: 58
- **Behaviors covered**: 0 (0%)
- **Gaps to fill**: 58

## Test Infrastructure Needed

```toml
# Add to ktcs-cli/Cargo.toml
[dev-dependencies]
assert_cmd = "2"        # CLI integration testing
predicates = "3"        # Assertion helpers
tempfile = "3"          # Temporary file fixtures
pretty_assertions = "1" # Better diff output
```

Create test structure:
```
ktcs-cli/
├── src/
│   └── main.rs          # Add #[cfg(test)] module
└── tests/
    ├── common/
    │   └── mod.rs       # Shared fixtures
    ├── cli_basic.rs     # Basic CLI invocation tests
    ├── cli_stamp.rs     # Stamp command tests
    ├── cli_verify.rs    # Verify command tests
    └── cli_config.rs    # Config command tests
```

---

## Priority 1: Pure Unit Tests (No I/O, No Network)

These can be tested immediately with in-module `#[cfg(test)]` blocks.

| Function | Test Cases | Rationale |
|----------|------------|-----------|
| `determine_output_path()` | 4 cases | Path derivation logic |
| `parse_batch_mode()` | 6 cases | Input validation |
| `is_leap_year()` | 6 cases | Date calculation |
| `days_in_year()` | 3 cases | Date calculation |
| `days_in_months()` | 3 cases | Date calculation |
| `format_timestamp_utc()` | 5 cases | Timestamp formatting |

### Test Cases Detail

#### `determine_output_path()`
- `(Some("out.kts"), "in.txt")` → `"out.kts"`
- `(None, "file.txt")` → `"file.kts"`
- `(None, "file.tar.gz")` → `"file.tar.kts"`
- `(None, "README")` → `"README.kts"`

#### `parse_batch_mode()`
- `"instant"` → `Ok(BatchMode::Instant)`
- `"STANDARD"` → `Ok(BatchMode::Standard)` (case-insensitive)
- `"economic"` → `Ok(BatchMode::Economic)`
- `"fast"` → `Err`
- `""` → `Err`
- `"instant "` (trailing space) → `Err`

#### `is_leap_year()`
- `2000` → `true` (div by 400)
- `1900` → `false` (div by 100, not 400)
- `2004` → `true` (div by 4, not 100)
- `2001` → `false` (not div by 4)
- `2100` → `false` (div by 100, not 400)
- `1972` → `true`

#### `format_timestamp_utc()`
- `0` → `"1970-01-01 00:00:00 UTC"`
- `86400000` → `"1970-01-02 00:00:00 UTC"`
- `951782400000` → `"2000-02-29 00:00:00 UTC"` (leap year)
- `1609459200000` → `"2021-01-01 00:00:00 UTC"`

---

## Priority 2: Config System Unit Tests

Config struct methods can be unit tested with mock data.

| Method | Test Cases | Rationale |
|--------|------------|-----------|
| `Config::default()` | 1 case | Default values |
| `Config::network_config()` | 4 cases | Network selection |
| `Config::calendar()` | 4 cases | URL resolution |
| `Config::rpc_url()` | 4 cases | URL resolution |
| `Config::wallet_file()` | 4 cases | Path expansion |

### Test Cases Detail

#### `Config::network_config()`
- `network = "testnet"` → returns testnet config
- `network = "mainnet"` → returns mainnet config
- `network = "TESTNET"` → returns mainnet (case matters)
- `network = "invalid"` → returns mainnet (default)

#### `Config::calendar()`
- Configured calendar → returns configured value
- `network="testnet"`, calendar=None → testnet default URL
- `network="mainnet"`, calendar=None → mainnet default URL

#### `Config::wallet_file()`
- wallet_file=None → returns None
- wallet_file="~/key.txt" → expands tilde
- wallet_file="/abs/path" → unchanged
- wallet_file="rel/path" → unchanged

---

## Priority 3: Config File I/O Tests (with tempfile)

| Test | Behavior | Notes |
|------|----------|-------|
| `test_config_save_load_roundtrip` | Save then load produces same config | Uses tempfile |
| `test_config_load_missing_file` | Returns defaults | No file needed |
| `test_config_load_invalid_toml` | Returns defaults, prints warning | Writes invalid file |
| `test_config_save_creates_dir` | Creates parent directory | Uses tempdir |
| `test_config_partial_toml` | Missing fields get defaults | Partial file |

---

## Priority 4: CLI Integration Tests (with assert_cmd)

| Command | Test Cases | Notes |
|---------|------------|-------|
| `ktcs --help` | Shows usage | Basic invocation |
| `ktcs --version` | Shows version | Basic invocation |
| `ktcs hash <file>` | Outputs SHA256 hex | Uses tempfile |
| `ktcs info <proof>` | Shows proof info | Needs fixture |
| `ktcs info --json <proof>` | Valid JSON output | Needs fixture |
| `ktcs status <proof>` | Shows status | Needs fixture |
| `ktcs config path` | Prints path | No setup |
| `ktcs config init` | Creates file | Uses tempdir |
| `ktcs config show` | Displays config | After init |
| `ktcs completions bash` | Outputs script | No setup |

### Integration Test Details

#### Basic CLI Tests
```rust
#[test]
fn test_help_flag() {
    Command::cargo_bin("ktcs").unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Kaspa Thermodynamic Clock Service"));
}

#[test]
fn test_version_flag() {
    Command::cargo_bin("ktcs").unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("ktcs"));
}
```

#### Hash Command
```rust
#[test]
fn test_hash_command() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test content").unwrap();

    Command::cargo_bin("ktcs").unwrap()
        .args(["hash", file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::is_match("[0-9a-f]{64}\n").unwrap());
}
```

#### Config Commands
```rust
#[test]
fn test_config_path() {
    Command::cargo_bin("ktcs").unwrap()
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("config.toml"));
}
```

---

## Priority 5: Error Handling Tests

| Scenario | Expected Behavior | Test Type |
|----------|-------------------|-----------|
| Missing file for `hash` | Exit code 1, error message | Integration |
| Missing file for `stamp` | Exit code 1, error message | Integration |
| Invalid batch mode | Exit code 1, "Invalid batch mode" | Integration |
| Invalid config key | Exit code 1, lists valid keys | Integration |
| Invalid network value | Exit code 1, "mainnet or testnet" | Integration |

---

## Priority 6: Command-Specific Tests (Deferred)

These require network mocking or complex setup:

| Command | Complexity | Notes |
|---------|------------|-------|
| `stamp` (calendar mode) | High | Needs HTTP mock |
| `stamp --direct` | High | Needs Kaspa RPC mock |
| `verify --chain` | High | Needs Kaspa RPC mock |
| `complete` | Medium | Needs HTTP mock |
| `wallet balance` | High | Needs Kaspa RPC mock |

---

## Implementation Order

### Phase 1: Infrastructure Setup
1. Add dev-dependencies to Cargo.toml
2. Create `tests/` directory structure
3. Create `tests/common/mod.rs` with helpers

### Phase 2: Pure Unit Tests (in main.rs)
4. Add `#[cfg(test)] mod tests` block
5. Test `determine_output_path()`
6. Test `parse_batch_mode()`
7. Test date calculation functions
8. Test `format_timestamp_utc()`

### Phase 3: Config Unit Tests
9. Test `Config` default values
10. Test `Config::network_config()`
11. Test URL resolution methods
12. Test `wallet_file()` path expansion

### Phase 4: Config I/O Tests
13. Test `Config::save()` / `Config::load()` roundtrip
14. Test loading missing/invalid files
15. Test directory creation

### Phase 5: CLI Integration Tests
16. Test `--help`, `--version`
17. Test `hash` command
18. Test `config path`, `config init`, `config show`
19. Test `completions` command
20. Test error cases

### Phase 6: Complex Integration (Future)
21. HTTP mock for calendar tests
22. Kaspa RPC mock for direct stamp/verify
23. End-to-end workflow tests

---

## Test Fixtures Needed

### Proof Fixtures
- `pending_proof.kts` - Valid pending proof
- `complete_proof.kts` - Valid complete proof with Kaspa attestation
- `invalid_proof.kts` - Corrupted binary

### Config Fixtures
- `valid_config.toml` - Full configuration
- `partial_config.toml` - Only network field
- `invalid_config.toml` - Invalid TOML syntax

---

## Estimated Coverage After Implementation

| Phase | Functions Covered | Estimated Line Coverage |
|-------|-------------------|------------------------|
| Phase 2 | 6 pure functions | ~10% |
| Phase 3 | Config methods | ~15% |
| Phase 4 | Config I/O | ~20% |
| Phase 5 | CLI commands | ~40% |
| Phase 6 | Full commands | ~70% |

**Target: 40% coverage after Phase 5 (without network mocking)**
