//! Error handling integration tests for ktcs

use assert_cmd::Command;
use predicates::prelude::*;

mod common;

// ==================== Missing Required Arguments ====================

#[test]
fn test_stamp_missing_file_arg() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["stamp"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FILE"));
}

#[test]
fn test_verify_missing_proof_arg() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["verify"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("PROOF"));
}

#[test]
fn test_info_missing_proof_arg() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["info"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("PROOF"));
}

#[test]
fn test_complete_missing_proof_arg() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["complete"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("PROOF"));
}

#[test]
fn test_status_missing_proof_arg() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["status"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("PROOF"));
}

#[test]
fn test_hash_missing_file_arg() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["hash"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FILE"));
}

#[test]
fn test_config_set_missing_args() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "set"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("KEY"));
}

#[test]
fn test_config_set_missing_value() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "set", "network"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("VALUE"));
}

#[test]
fn test_completions_missing_shell() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["completions"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("SHELL"));
}

// ==================== File Not Found Errors ====================

#[test]
fn test_stamp_file_not_found() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["stamp", "/nonexistent/path/file.txt"])
        .assert()
        .failure();
}

#[test]
fn test_verify_proof_not_found() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["verify", "/nonexistent/proof.kts"])
        .assert()
        .failure();
}

#[test]
fn test_info_proof_not_found() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["info", "/nonexistent/proof.kts"])
        .assert()
        .failure();
}

#[test]
fn test_complete_proof_not_found() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["complete", "/nonexistent/proof.kts"])
        .assert()
        .failure();
}

#[test]
fn test_status_proof_not_found() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["status", "/nonexistent/proof.kts"])
        .assert()
        .failure();
}

// ==================== Invalid Flag Values ====================

#[test]
fn test_stamp_invalid_mode() {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "test").unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["stamp", "--mode", "ultrafast", file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid batch mode"));
}

#[test]
fn test_invalid_color_value() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["--color", "rainbow", "--help"])
        .assert()
        .failure();
}

// ==================== Wallet Errors ====================

#[test]
fn test_stamp_direct_without_wallet() {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "test").unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["stamp", "--direct", file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("wallet"));
}

#[test]
fn test_wallet_address_without_key() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "address"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("wallet-file").or(predicate::str::contains("wallet-stdin")));
}

#[test]
fn test_wallet_balance_without_source() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "balance"])
        .assert()
        .failure();
}

#[test]
fn test_wallet_address_invalid_key_file() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "address", "--wallet-file", "/nonexistent/key.txt"])
        .assert()
        .failure();
}

// ==================== Subcommand Errors ====================

#[test]
fn test_unknown_subcommand() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["unknown"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized").or(predicate::str::contains("invalid")));
}

#[test]
fn test_wallet_unknown_subcommand() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "unknown"])
        .assert()
        .failure();
}

#[test]
fn test_config_unknown_subcommand() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "unknown"])
        .assert()
        .failure();
}
