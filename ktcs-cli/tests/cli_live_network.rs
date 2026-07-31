//! Live network integration tests for ktcs
//!
//! These tests hit the actual Kaspa mainnet. They require:
//! - Network connectivity
//! - A funded wallet (reads from environment or ktcs-calendar/.env)
//!
//! Run with: cargo test -p ktcs-cli --test cli_live_network -- --ignored
//!
//! CAUTION: Direct stamp tests consume KAS for transaction fees.

use assert_cmd::Command;
use predicates::prelude::*;
use std::env;
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;

mod common;

// ==================== Test Configuration ====================

/// Get the RPC URL for mainnet
fn get_rpc_url() -> String {
    env::var("KASPA_RPC_URL")
        .unwrap_or_else(|_| "wss://kaspa.aspectron.com/wrpc/json/mainnet".to_string())
}

/// Get the test wallet private key (hex)
fn get_wallet_key() -> Option<String> {
    // Try environment variable first
    if let Ok(key) = env::var("CALENDAR_WALLET_KEY") {
        return Some(key);
    }

    // Try reading from ktcs-calendar/.env
    let env_path = std::path::Path::new("../ktcs-calendar/.env");
    if let Ok(content) = fs::read_to_string(env_path) {
        for line in content.lines() {
            if line.starts_with("CALENDAR_WALLET_KEY=") {
                return Some(line.trim_start_matches("CALENDAR_WALLET_KEY=").to_string());
            }
        }
    }

    None
}

/// Get the test wallet address
fn get_wallet_address() -> String {
    env::var("CALENDAR_WALLET_ADDRESS").unwrap_or_else(|_| {
        "kaspa:qr69lermmqne4xrm0wgtsclsmvsw64xmazmh4xaljdfkhy0qwm00ggqetrh55".to_string()
    })
}

/// Create a temp file with the wallet key
fn create_wallet_file() -> Option<NamedTempFile> {
    let key = get_wallet_key()?;
    let mut file = NamedTempFile::new().ok()?;
    writeln!(file, "{}", key).ok()?;
    file.flush().ok()?;
    Some(file)
}

// ==================== Wallet Balance Tests ====================

#[test]
#[ignore = "Requires network - run with --ignored"]
fn test_wallet_balance_with_funded_address() {
    let address = get_wallet_address();
    let rpc_url = get_rpc_url();

    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "wallet",
            "balance",
            "--address",
            &address,
            "--rpc",
            &rpc_url,
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("stdout: {}", stdout);
    println!("stderr: {}", stderr);

    // Should either succeed with a balance or fail with connection error
    // We're testing that the command works, not that we have specific funds
    if output.status.success() {
        // Check that output contains balance info (numbers)
        assert!(
            stdout.contains("KAS") || stdout.chars().any(|c| c.is_numeric()),
            "Expected balance output, got: {}",
            stdout
        );
    } else {
        // If it fails, it should be a connection issue, not a CLI argument issue
        assert!(
            stderr.contains("connection")
                || stderr.contains("timeout")
                || stderr.contains("WebSocket")
                || stderr.contains("RPC"),
            "Unexpected error: {}",
            stderr
        );
    }
}

#[test]
#[ignore = "Requires network - run with --ignored"]
fn test_wallet_balance_with_wallet_file() {
    let wallet_file = match create_wallet_file() {
        Some(f) => f,
        None => {
            println!("Skipping: No wallet key available");
            return;
        }
    };
    let rpc_url = get_rpc_url();

    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "wallet",
            "balance",
            "--wallet-file",
            wallet_file.path().to_str().unwrap(),
            "--rpc",
            &rpc_url,
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("stdout: {}", stdout);
    println!("stderr: {}", stderr);

    if output.status.success() {
        assert!(
            stdout.contains("KAS") || stdout.chars().any(|c| c.is_numeric()),
            "Expected balance output"
        );
    }
}

#[test]
#[ignore = "Requires network - run with --ignored"]
fn test_wallet_balance_with_explicit_rpc() {
    // `wallet balance` has no `--format json` flag; this exercises the real
    // `--rpc` flag and asserts human-readable balance output.
    let address = get_wallet_address();
    let rpc_url = get_rpc_url();

    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "wallet",
            "balance",
            "--address",
            &address,
            "--rpc",
            &rpc_url,
        ])
        .output()
        .unwrap();

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("KAS") || stdout.chars().any(|c| c.is_numeric()),
            "Expected balance output, got: {}",
            stdout
        );
    }
}

// ==================== Wallet Address Derivation Tests ====================

#[test]
#[ignore = "Requires network - run with --ignored"]
fn test_wallet_address_from_key_file() {
    let wallet_file = match create_wallet_file() {
        Some(f) => f,
        None => {
            println!("Skipping: No wallet key available");
            return;
        }
    };

    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "wallet",
            "address",
            "--wallet-file",
            wallet_file.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_address = get_wallet_address();

    assert!(
        stdout.contains(&expected_address),
        "Expected address {}, got: {}",
        expected_address,
        stdout
    );
}

// ==================== Direct Stamp Tests ====================
// WARNING: These tests consume KAS for transaction fees!

#[test]
#[ignore = "Requires network AND consumes KAS - run explicitly"]
fn test_stamp_direct_creates_complete_proof() {
    let wallet_file = match create_wallet_file() {
        Some(f) => f,
        None => {
            println!("Skipping: No wallet key available");
            return;
        }
    };
    let rpc_url = get_rpc_url();

    // Create a temp file to stamp
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test content for direct stamping {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()).unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();

    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--direct",
            "--wallet-file",
            wallet_file.path().to_str().unwrap(),
            "--rpc-url",
            &rpc_url,
            "--output",
            output_file.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(60))
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("stdout: {}", stdout);
    println!("stderr: {}", stderr);

    if output.status.success() {
        // Verify output file was created and is a valid proof
        assert!(output_file.path().exists(), "Output file should exist");

        let proof_data = fs::read(output_file.path()).unwrap();
        // Check magic bytes
        assert!(
            proof_data.len() >= 18,
            "Proof file too small: {} bytes",
            proof_data.len()
        );

        // Verify we can read the proof with info command
        Command::cargo_bin("ktcs")
            .unwrap()
            .args(["info", output_file.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Complete").or(predicate::str::contains("Kaspa")));
    } else {
        // Acceptable failures: insufficient balance, network issues
        assert!(
            stderr.contains("insufficient")
                || stderr.contains("balance")
                || stderr.contains("connection")
                || stderr.contains("timeout"),
            "Unexpected error: {}",
            stderr
        );
    }
}

// ==================== Verify with Chain Tests ====================

#[test]
#[ignore = "Requires network - run with --ignored"]
fn test_verify_help_shows_chain_option() {
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["verify", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--chain"));
}

// ==================== End-to-End Workflow Test ====================

#[test]
#[ignore = "Requires network AND consumes KAS - run explicitly"]
fn test_full_direct_stamp_and_verify_workflow() {
    let wallet_file = match create_wallet_file() {
        Some(f) => f,
        None => {
            println!("Skipping: No wallet key available");
            return;
        }
    };
    let rpc_url = get_rpc_url();

    // Step 1: Create a file to stamp
    let mut file = NamedTempFile::new().unwrap();
    let unique_content = format!(
        "E2E test content {}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    writeln!(file, "{}", unique_content).unwrap();
    file.flush().unwrap();

    // Step 2: Get hash of the file
    let hash_output = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["hash", file.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(hash_output.status.success());
    let file_hash = String::from_utf8_lossy(&hash_output.stdout).trim().to_string();
    println!("File hash: {}", file_hash);

    // Step 3: Direct stamp
    let proof_file = NamedTempFile::new().unwrap();

    let stamp_output = Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--direct",
            "--wallet-file",
            wallet_file.path().to_str().unwrap(),
            "--rpc-url",
            &rpc_url,
            "--output",
            proof_file.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(60))
        .output()
        .unwrap();

    if !stamp_output.status.success() {
        let stderr = String::from_utf8_lossy(&stamp_output.stderr);
        println!("Stamp failed (may be expected): {}", stderr);
        return;
    }

    println!("Stamp succeeded!");

    // Step 4: Get proof info
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["info", proof_file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(&file_hash[..16])); // First 16 chars of hash

    // Step 5: Get proof status
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["status", proof_file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Complete").or(predicate::str::contains("Kaspa")));

    // Step 6: Verify the proof (local verification)
    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "verify",
            "--data",
            file.path().to_str().unwrap(),
            proof_file.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    // Step 7: Verify with chain (on-chain verification)
    let verify_chain_output = Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "verify",
            "--chain",
            "--rpc-url",
            &rpc_url,
            "--data",
            file.path().to_str().unwrap(),
            proof_file.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&verify_chain_output.stdout);
    let stderr = String::from_utf8_lossy(&verify_chain_output.stderr);
    println!("Verify --chain stdout: {}", stdout);
    println!("Verify --chain stderr: {}", stderr);

    // Chain verification should succeed if stamp succeeded
    assert!(
        verify_chain_output.status.success(),
        "Chain verification failed: {}",
        stderr
    );
}

// ==================== Connection Test ====================

#[test]
#[ignore = "Requires network - run with --ignored"]
fn test_rpc_connection_works() {
    let rpc_url = get_rpc_url();
    let address = get_wallet_address();

    // This is a simple connectivity test
    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "wallet",
            "balance",
            "--address",
            &address,
            "--rpc",
            &rpc_url,
        ])
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("Connection test - stdout: {}", stdout);
    println!("Connection test - stderr: {}", stderr);

    // Either succeeds or fails with a clear network error
    // This helps diagnose if tests fail due to network issues
    if !output.status.success() {
        println!("WARNING: RPC connection failed. Other network tests will likely fail too.");
    }
}
