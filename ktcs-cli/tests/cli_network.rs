//! Network-dependent integration tests for ktcs
//!
//! These tests use wiremock to mock HTTP endpoints for the calendar service.
//! Kaspa RPC tests are marked #[ignore] as they require actual network or more complex mocking.

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;

// ==================== Calendar HTTP Mock Helpers ====================

/// Create a mock calendar response for pending status
fn pending_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "status": "pending"
    }))
}

/// Create a mock calendar response for confirmed status with a proof
/// Note: In real usage, this would contain a valid base64-encoded KTS proof
fn confirmed_response_with_proof(proof_base64: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "status": "confirmed",
        "proof": proof_base64
    }))
}

/// Create a mock 404 response
fn not_found_response() -> ResponseTemplate {
    ResponseTemplate::new(404).set_body_string("Not Found")
}

/// Create a mock 500 response
fn server_error_response() -> ResponseTemplate {
    ResponseTemplate::new(500).set_body_string("Internal Server Error")
}

// ==================== Stamp Command (Calendar Mode) Tests ====================

#[tokio::test]
async fn test_stamp_async_creates_pending_proof() {
    // Create a temp file to stamp
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test content for stamping").unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();

    // Run stamp with --async (doesn't need mock since it doesn't poll)
    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--async",
            "--output",
            output_file.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pending"));

    // Verify output file exists
    assert!(output_file.path().exists());
}

#[tokio::test]
async fn test_stamp_with_custom_calendar_url() {
    let mock_server = MockServer::start().await;

    // Mount a mock that always returns pending (to trigger timeout eventually)
    Mock::given(method("GET"))
        .and(path_regex(r"/v1/stamp/ktcs_.*"))
        .respond_with(pending_response())
        .mount(&mock_server)
        .await;

    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test content").unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();

    // Run stamp with custom calendar URL (will timeout after 60s, use --async to avoid)
    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--async",
            "--calendar",
            &mock_server.uri(),
            "--output",
            output_file.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pending"));
}

#[tokio::test]
async fn test_stamp_shows_file_hash() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test content").unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--async",
            "--output",
            output_file.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("SHA256:"));
}

#[tokio::test]
async fn test_stamp_with_different_modes() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test").unwrap();
    file.flush().unwrap();

    // Test instant mode
    let output1 = NamedTempFile::new().unwrap();
    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--async",
            "--mode",
            "instant",
            "--output",
            output1.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Instant"));

    // Test standard mode
    let output2 = NamedTempFile::new().unwrap();
    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--async",
            "--mode",
            "standard",
            "--output",
            output2.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Standard"));

    // Test economic mode
    let output3 = NamedTempFile::new().unwrap();
    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--async",
            "--mode",
            "economic",
            "--output",
            output3.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Economic"));
}

// ==================== Complete Command Tests ====================

#[tokio::test]
async fn test_complete_with_server_error() {
    let mock_server = MockServer::start().await;

    // Mount mock that returns 500 error
    Mock::given(method("GET"))
        .and(path_regex(r"/v1/stamp/ktcs_.*"))
        .respond_with(server_error_response())
        .mount(&mock_server)
        .await;

    // Create a pending proof file with the mock server URL
    let proof_content = create_pending_proof_with_url(&mock_server.uri());
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["complete", proof_file.path().to_str().unwrap()])
        .assert()
        .failure();
}

#[tokio::test]
async fn test_complete_with_pending_response() {
    let mock_server = MockServer::start().await;

    // Mount mock that returns pending status
    Mock::given(method("GET"))
        .and(path_regex(r"/v1/stamp/ktcs_.*"))
        .respond_with(pending_response())
        .mount(&mock_server)
        .await;

    // Create a pending proof file
    let proof_content = create_pending_proof_with_url(&mock_server.uri());
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["complete", proof_file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("not yet confirmed"));
}

// ==================== Direct Stamp Tests ====================
// Note: Full live network tests are in cli_live_network.rs
// Run with: cargo test --test cli_live_network -- --ignored

#[test]
fn test_stamp_direct_requires_wallet() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test").unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["stamp", "--direct", file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("wallet").or(predicate::str::contains("required")));
}

#[test]
fn test_stamp_direct_with_invalid_wallet() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test").unwrap();

    let mut wallet_file = NamedTempFile::new().unwrap();
    writeln!(wallet_file, "invalid_hex_key").unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--direct",
            "--wallet-file",
            wallet_file.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}

// ==================== Verify with Chain Tests ====================

#[test]
fn test_verify_chain_flag_accepted() {
    // Verify the --chain flag is recognized (shows in help)
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["verify", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--chain"));
}

// ==================== Wallet Balance Tests ====================
// Note: Full live network balance tests are in cli_live_network.rs

#[test]
fn test_wallet_balance_requires_source() {
    // Balance command needs either --address or --wallet-file
    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "balance"])
        .assert()
        .failure();
}

// ==================== Info Command with Proof Files ====================

#[tokio::test]
async fn test_info_with_pending_proof() {
    // Create a pending proof
    let proof_content = create_pending_proof_with_url("https://calendar.ktcs.kaspa.org");
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["info", proof_file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pending"));
}

#[tokio::test]
async fn test_status_with_pending_proof() {
    let proof_content = create_pending_proof_with_url("https://calendar.ktcs.kaspa.org");
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["status", proof_file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pending"));
}

#[tokio::test]
async fn test_status_json_output() {
    let proof_content = create_pending_proof_with_url("https://calendar.ktcs.kaspa.org");
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["status", "--json", proof_file.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Verify it's valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "Expected valid JSON, got: {}", stdout);
}

// ==================== Wallet Generate (No Network) ====================

#[test]
fn test_wallet_generate_produces_valid_hex() {
    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "generate"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Private key should be 64 hex characters
    let key = stdout.trim();
    assert_eq!(key.len(), 64, "Expected 64 char hex key, got: {}", key);
    assert!(
        key.chars().all(|c| c.is_ascii_hexdigit()),
        "Key should be hex: {}",
        key
    );
}

#[test]
fn test_wallet_generate_json_format() {
    let output = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "generate", "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "Expected valid JSON, got: {}", stdout);

    let json = parsed.unwrap();
    assert!(json.get("private_key").is_some());
    assert!(json.get("address").is_some());
    assert!(json.get("network").is_some());
    assert!(json.get("public_key").is_some());
}

#[test]
fn test_wallet_generate_different_networks() {
    // Generate for mainnet
    let output_main = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "generate", "--format", "json", "--network", "mainnet"])
        .output()
        .unwrap();

    // Generate for testnet
    let output_test = Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "generate", "--format", "json", "--network", "testnet"])
        .output()
        .unwrap();

    let main_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output_main.stdout)).unwrap();
    let test_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output_test.stdout)).unwrap();

    // Addresses should have different prefixes
    let main_addr = main_json["address"].as_str().unwrap();
    let test_addr = test_json["address"].as_str().unwrap();

    assert!(
        main_addr.starts_with("kaspa:"),
        "Mainnet address should start with kaspa:"
    );
    assert!(
        test_addr.starts_with("kaspatest:"),
        "Testnet address should start with kaspatest:"
    );
}

#[test]
fn test_wallet_generate_to_file() {
    let output_file = NamedTempFile::new().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "wallet",
            "generate",
            "--output",
            output_file.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    // Read the file and verify it contains a hex key
    let content = std::fs::read_to_string(output_file.path()).unwrap();
    let key = content.trim();
    assert_eq!(key.len(), 64);
    assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
}

// ==================== Helper Functions ====================

/// Create a minimal pending proof with a specific calendar URL
/// This creates a valid KTS proof structure that ktcs can parse
fn create_pending_proof_with_url(calendar_base_url: &str) -> Vec<u8> {
    // KTS proof format (from ktcs-core/src/proof.rs):
    // Header (26 bytes):
    //   - Magic bytes: 0x00 "KaspaTime" 0x00 0x00 "Proof" 0x00 (18 bytes)
    //   - Version: 0x01 (1 byte)
    //   - Hash algorithm: 0x08 = SHA256 (1 byte)
    //   - Flags: 0x00 (1 byte)
    //   - Reserved: 5 x 0x00 (5 bytes)
    // Digest: 32 bytes for SHA256
    // Attestations (no operations in this simple pending proof):
    //   - Tag: 0x83 = Pending (1 byte)
    //   - URL length as varint
    //   - URL bytes

    let mut proof = Vec::new();

    // Magic bytes (18 bytes): 0x00 "KaspaTime" 0x00 0x00 "Proof" 0x00
    proof.extend_from_slice(&[
        0x00, 0x4b, 0x61, 0x73, 0x70, 0x61, 0x54, 0x69, 0x6d, 0x65, // 0x00 "KaspaTime"
        0x00, 0x00, 0x50, 0x72, 0x6f, 0x6f, 0x66, 0x00, // 0x00 0x00 "Proof" 0x00
    ]);

    // Version: 0x01
    proof.push(0x01);

    // Hash algorithm: 0x08 = SHA256
    proof.push(0x08);

    // Flags: 0x00
    proof.push(0x00);

    // Reserved: 5 bytes of zeros
    proof.extend_from_slice(&[0x00; 5]);

    // Digest: 32 bytes (zeros for test - the digest value doesn't matter for parsing)
    proof.extend_from_slice(&[0u8; 32]);

    // Pending attestation
    // Tag: 0x83 = Pending
    proof.push(0x83);

    // Calendar URL
    let url = format!("{}/v1/stamp/ktcs_0000000000000000", calendar_base_url);
    let url_bytes = url.as_bytes();

    // URL length as varint (Bitcoin-style: if < 0xFD, just the byte itself)
    if url_bytes.len() < 0xFD {
        proof.push(url_bytes.len() as u8);
    } else {
        // For longer URLs, use 0xFD prefix + 2-byte length (little-endian)
        proof.push(0xFD);
        proof.extend_from_slice(&(url_bytes.len() as u16).to_le_bytes());
    }
    proof.extend_from_slice(url_bytes);

    proof
}
