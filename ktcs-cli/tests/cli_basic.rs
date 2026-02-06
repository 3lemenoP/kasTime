//! Basic CLI integration tests for ktcs

use assert_cmd::Command;
use predicates::prelude::*;

mod common;

// ==================== Help and Version ====================

#[test]
fn test_help_flag() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Kaspa blockchain"));
}

#[test]
fn test_help_flag_short() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn test_version_flag() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("ktcs"));
}

#[test]
fn test_version_flag_short() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .arg("-V")
        .assert()
        .success();
}

// ==================== No Arguments ====================

#[test]
fn test_no_arguments_shows_help() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));
}

// ==================== Hash Command ====================

#[test]
fn test_hash_command_with_file() {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "test content\n").unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["hash", file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::is_match("[0-9a-f]{64}\n").unwrap());
}

#[test]
fn test_hash_command_deterministic() {
    use std::io::Write;
    let mut file1 = tempfile::NamedTempFile::new().unwrap();
    let mut file2 = tempfile::NamedTempFile::new().unwrap();
    write!(file1, "same content").unwrap();
    write!(file2, "same content").unwrap();

    let output1 = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["hash", file1.path().to_str().unwrap()])
        .output()
        .unwrap();

    let output2 = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["hash", file2.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output1.stdout, output2.stdout);
}

#[test]
fn test_hash_command_missing_file() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["hash", "/nonexistent/file.txt"])
        .assert()
        .failure();
}

// ==================== Completions Command ====================

#[test]
fn test_completions_bash() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_ktcs"));
}

#[test]
fn test_completions_zsh() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef"));
}

#[test]
fn test_completions_fish() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn test_completions_powershell() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["completions", "powershell"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Register-ArgumentCompleter"));
}

// ==================== Global Flags ====================

#[test]
fn test_verbose_flag() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["--verbose", "--help"])
        .assert()
        .success();
}

#[test]
fn test_color_flag_never() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["--color", "never", "--help"])
        .assert()
        .success();
}

#[test]
fn test_color_flag_always() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["--color", "always", "--help"])
        .assert()
        .success();
}

#[test]
fn test_quiet_flag() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["--quiet", "--help"])
        .assert()
        .success();
}

// ==================== Subcommand Help ====================

#[test]
fn test_stamp_help() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["stamp", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("timestamp"));
}

#[test]
fn test_verify_help() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["verify", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("proof"));
}

#[test]
fn test_complete_help() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["complete", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pending"));
}

#[test]
fn test_status_help() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["status", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("proof"));
}

#[test]
fn test_info_help() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["info", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("proof"));
}

#[test]
fn test_wallet_help() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("generate"));
}

#[test]
fn test_config_help() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("init"));
}

// ==================== Command Aliases ====================

#[test]
fn test_stamp_alias_s() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["s", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("timestamp"));
}

#[test]
fn test_verify_alias_v() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["v", "--help"])
        .assert()
        .success();
}

#[test]
fn test_complete_alias_c() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["c", "--help"])
        .assert()
        .success();
}

#[test]
fn test_complete_alias_upgrade() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["upgrade", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pending"));
}

#[test]
fn test_info_alias_i() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["i", "--help"])
        .assert()
        .success();
}

#[test]
fn test_wallet_alias_w() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["w", "--help"])
        .assert()
        .success();
}
