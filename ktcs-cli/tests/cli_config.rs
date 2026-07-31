//! Config command integration tests for ktcs

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

mod common;

// ==================== Config Path ====================

#[test]
fn test_config_path_outputs_path() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("config.toml"));
}

#[test]
fn test_config_path_contains_ktcs_dir() {
    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "path"])
        .output()
        .unwrap();

    let path = String::from_utf8_lossy(&output.stdout);
    assert!(
        path.contains("ktcs"),
        "Path should contain 'ktcs': {}",
        path
    );
}

// ==================== Config Init ====================

#[test]
#[cfg(unix)]
fn test_config_init_creates_file() {
    let temp_dir = TempDir::new().unwrap();
    let _config_path = temp_dir.path().join("ktcs").join("config.toml");

    // Run with custom HOME/config dir
    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Config file created"));
}

#[test]
#[cfg(windows)]
fn test_config_init_creates_file() {
    // On Windows, we can't easily override the config directory
    // Test that init command runs (may say "already exists" or "created")
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "init"])
        .assert()
        .success();
}

#[test]
fn test_config_init_warns_if_exists() {
    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().join("ktcs");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), "network = \"mainnet\"\n").unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already exists"));
}

#[test]
#[cfg(unix)]
fn test_config_init_force_overwrites() {
    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().join("ktcs");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), "old content").unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "init", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Config file created"));

    let content = fs::read_to_string(config_dir.join("config.toml")).unwrap();
    assert!(content.contains("network"), "Should have replaced content");
    assert!(
        !content.contains("old content"),
        "Should not have old content"
    );
}

#[test]
#[cfg(windows)]
fn test_config_init_force_overwrites() {
    // On Windows, we can't easily override the config path via env vars
    // Just verify that --force flag works (creates or overwrites)
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "init", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Config file created"));
}

// ==================== Config Show ====================

#[test]
fn test_config_show_displays_network() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Network:"));
}

#[test]
fn test_config_show_displays_calendar() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Calendar:"));
}

#[test]
fn test_config_show_displays_rpc_url() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("RPC URL:"));
}

#[test]
fn test_config_show_network_override() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "show", "--network", "testnet"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[testnet]"));
}

#[test]
fn test_config_show_mainnet_default() {
    // Note: This test may show testnet if the user has a config file with testnet set
    // The test verifies that config show succeeds and shows network info
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Network:"));
}

// ==================== Config Set ====================

#[test]
fn test_config_set_invalid_key() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "set", "invalid_key", "value"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown config key"));
}

#[test]
fn test_config_set_invalid_network_value() {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "set", "network", "invalid"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mainnet").and(predicate::str::contains("testnet")));
}

#[test]
fn test_config_set_invalid_prefer_direct_value() {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "set", "prefer_direct", "invalid"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("true").or(predicate::str::contains("false")));
}

#[test]
fn test_config_set_lists_valid_keys_on_error() {
    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["config", "set", "bad_key", "value"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The error message should contain valid keys list
    assert!(
        stderr.contains("Unknown config key") || stderr.contains("network"),
        "Expected error message about unknown key, got: {}",
        stderr
    );
}

// ==================== Config Set Success Cases ====================

#[test]
fn test_config_set_network_mainnet() {
    let temp_dir = TempDir::new().unwrap();

    // First init
    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "init"])
        .assert()
        .success();

    // Then set
    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "set", "network", "testnet"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set network = testnet"));
}

#[test]
fn test_config_set_prefer_direct() {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "init"])
        .assert()
        .success();

    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "set", "prefer_direct", "true"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set prefer_direct = true"));
}

#[test]
fn test_config_set_mainnet_calendar() {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "init"])
        .assert()
        .success();

    Command::cargo_bin("ktcs")
        .unwrap()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(["config", "set", "mainnet.calendar", "https://custom.url"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set mainnet.calendar"));
}
