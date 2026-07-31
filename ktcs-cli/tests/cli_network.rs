//! Network-dependent integration tests for ktcs
//!
//! These tests use wiremock to mock HTTP endpoints for the calendar service.
//! Kaspa RPC tests are marked #[ignore] as they require actual network or more complex mocking.

use assert_cmd::Command;
use base64::Engine;
use ktcs_core::{
    merkle::sha256, serialize_proof, Attestation, KaspaAttestation, KtcsProof, PendingAttestation,
};
use predicates::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;
use wiremock::matchers::{body_partial_json, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;

// ==================== Calendar HTTP Mock Helpers ====================

/// A fixed stamp id used by the calendar mocks.
const MOCK_STAMP_ID: &str = "ktcs_test0000000001";

/// Mock response for `POST /v1/stamp` returning a server-assigned id.
fn submit_response(id: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": id,
        "status": "pending",
        "submitted_at": "2024-01-01T00:00:00Z",
        "estimated_confirmation": "2024-01-01T00:00:02Z",
        "pending_proof": ""
    }))
}

/// Create a mock calendar response for pending status
fn pending_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "status": "pending"
    }))
}

/// Create a mock calendar response for confirmed status with a proof
fn confirmed_response_with_proof(proof_base64: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "status": "confirmed",
        "proof": proof_base64
    }))
}

/// Create a mock 500 response
fn server_error_response() -> ResponseTemplate {
    ResponseTemplate::new(500).set_body_string("Internal Server Error")
}

/// Base64-encode a proof for `content` carrying a structurally-VALID complete
/// Kaspa attestation (digest = sha256(content)). Passes `verify_proof`.
fn valid_confirmed_proof_base64(content: &[u8]) -> String {
    let digest = sha256(content);
    let mut proof = KtcsProof::new(digest.to_vec());
    proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
        42_000_000,
        41_500_000,
        [0xde; 32],
        1_706_000_000_000,
        [0xab; 32],
        0,
        [0x12; 32],
        vec![[0x11; 32]],
    )));
    base64::engine::general_purpose::STANDARD.encode(serialize_proof(&proof))
}

/// Base64-encode a proof for `content` with a structurally-INVALID attestation
/// (zero block/tx hashes). Deserializes, but fails `verify_proof`, so the CLI
/// must reject it.
fn invalid_confirmed_proof_base64(content: &[u8]) -> String {
    let digest = sha256(content);
    let mut proof = KtcsProof::new(digest.to_vec());
    proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
        42_000_000,
        41_500_000,
        [0x00; 32], // zero block hash -> structurally invalid
        1_706_000_000_000,
        [0x00; 32], // zero tx hash
        0,
        [0x12; 32],
        vec![],
    )));
    base64::engine::general_purpose::STANDARD.encode(serialize_proof(&proof))
}

/// Base64-encode a confirmed proof for an explicit `digest` (used by `complete`
/// tests, where the digest must match the pending proof's zero digest).
fn confirmed_proof_base64_for_digest(digest: [u8; 32], valid: bool) -> String {
    let mut proof = KtcsProof::new(digest.to_vec());
    let att = if valid {
        KaspaAttestation::new(
            42_000_000,
            41_500_000,
            [0xde; 32],
            1_706_000_000_000,
            [0xab; 32],
            0,
            [0x12; 32],
            vec![[0x11; 32]],
        )
    } else {
        KaspaAttestation::new(
            42_000_000,
            41_500_000,
            [0x00; 32],
            1_706_000_000_000,
            [0x00; 32],
            0,
            [0x12; 32],
            vec![],
        )
    };
    proof.add_attestation(Attestation::Kaspa(att));
    base64::engine::general_purpose::STANDARD.encode(serialize_proof(&proof))
}

/// Serialize a pending proof (zero digest) whose pending attestation carries an
/// arbitrary raw URL, for scheme/SSRF tests.
fn pending_proof_with_raw_url(url: &str) -> Vec<u8> {
    let mut proof = KtcsProof::new([0u8; 32].to_vec());
    proof.add_attestation(Attestation::Pending(PendingAttestation {
        calendar_url: url.to_string(),
    }));
    serialize_proof(&proof)
}

// ==================== Stamp Command (Calendar Mode) Tests ====================

#[tokio::test]
async fn test_stamp_async_creates_pending_proof() {
    let mock_server = MockServer::start().await;

    // Async mode now really submits to the calendar; mock the POST endpoint.
    Mock::given(method("POST"))
        .and(path("/v1/stamp"))
        .respond_with(submit_response(MOCK_STAMP_ID))
        .mount(&mock_server)
        .await;

    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test content for stamping").unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();

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

    // Verify output file exists and embeds the SERVER-RETURNED id in its URL.
    assert!(output_file.path().exists());
    let proof_bytes = std::fs::read(output_file.path()).unwrap();
    let proof = ktcs_core::deserialize_proof(&proof_bytes).unwrap();
    let has_server_url = proof.attestations.iter().any(|a| matches!(
        a,
        Attestation::Pending(p) if p.calendar_url.contains(MOCK_STAMP_ID)
    ));
    assert!(has_server_url, "pending proof must embed the server-returned stamp id");
}

#[tokio::test]
async fn test_stamp_posts_correct_body() {
    // Asserts the CLI POSTs {digest, algorithm:"sha256", batch_mode:"instant"}.
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/stamp"))
        .and(body_partial_json(serde_json::json!({
            "algorithm": "sha256",
            "batch_mode": "instant"
        })))
        .respond_with(submit_response(MOCK_STAMP_ID))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test content").unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--async",
            "--mode",
            "instant",
            "--calendar",
            &mock_server.uri(),
            "--output",
            output_file.path().to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    // MockServer verifies the `.expect(1)` on drop.
}

#[tokio::test]
async fn test_stamp_sync_confirmed_writes_verified_proof() {
    // Covers the previously-untested sync confirm path: POST -> poll GET ->
    // confirmed -> base64 decode -> verify_proof -> write.
    let content = b"sync confirm test content\n";
    let proof_b64 = valid_confirmed_proof_base64(content);

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/stamp"))
        .respond_with(submit_response(MOCK_STAMP_ID))
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/stamp/{}", MOCK_STAMP_ID)))
        .respond_with(confirmed_response_with_proof(&proof_b64))
        .mount(&mock_server)
        .await;

    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content).unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();
    let out_path = output_file.path().to_path_buf();
    drop(output_file); // ensure the output path does not exist yet

    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--calendar",
            &mock_server.uri(),
            "--output",
            out_path.to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Complete"));

    // The written proof must be the confirmed (complete) one.
    let written = std::fs::read(&out_path).unwrap();
    let proof = ktcs_core::deserialize_proof(&written).unwrap();
    assert!(proof.is_complete(), "written proof should be complete");
}

#[tokio::test]
async fn test_stamp_sync_rejects_invalid_calendar_proof() {
    // Security: a calendar returning a confirmed-but-INVALID proof must be
    // rejected (verify_proof is run on the response), and the CLI must fail.
    let content = b"invalid proof rejection test\n";
    let bad_proof_b64 = invalid_confirmed_proof_base64(content);

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/stamp"))
        .respond_with(submit_response(MOCK_STAMP_ID))
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/stamp/{}", MOCK_STAMP_ID)))
        .respond_with(confirmed_response_with_proof(&bad_proof_b64))
        .mount(&mock_server)
        .await;

    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content).unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();
    let out_path = output_file.path().to_path_buf();
    drop(output_file);

    Command::cargo_bin("ktcs")
        .unwrap()
        .args([
            "stamp",
            "--calendar",
            &mock_server.uri(),
            "--output",
            out_path.to_str().unwrap(),
            file.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid proof"));

    // The invalid proof must NOT have been written to the output path.
    assert!(!out_path.exists(), "invalid proof must not be written");
}

#[tokio::test]
async fn test_stamp_with_custom_calendar_url() {
    let mock_server = MockServer::start().await;

    // Mock the POST submission.
    Mock::given(method("POST"))
        .and(path("/v1/stamp"))
        .respond_with(submit_response(MOCK_STAMP_ID))
        .mount(&mock_server)
        .await;

    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test content").unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();

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
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/stamp"))
        .respond_with(submit_response(MOCK_STAMP_ID))
        .mount(&mock_server)
        .await;

    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test content").unwrap();
    file.flush().unwrap();

    let output_file = NamedTempFile::new().unwrap();

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
        .stdout(predicate::str::contains("SHA256:"));
}

#[tokio::test]
async fn test_stamp_with_different_modes() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/stamp"))
        .respond_with(submit_response(MOCK_STAMP_ID))
        .mount(&mock_server)
        .await;

    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test").unwrap();
    file.flush().unwrap();

    for (mode, label) in [
        ("instant", "Instant"),
        ("standard", "Standard"),
        ("economic", "Economic"),
    ] {
        let output = NamedTempFile::new().unwrap();
        Command::cargo_bin("ktcs")
            .unwrap()
            .args([
                "stamp",
                "--async",
                "--mode",
                mode,
                "--calendar",
                &mock_server.uri(),
                "--output",
                output.path().to_str().unwrap(),
                file.path().to_str().unwrap(),
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains(label));
    }
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

#[tokio::test]
async fn test_complete_confirmed_upgrades_proof() {
    // Covers the previously-untested confirm path in `complete`: the returned
    // proof is base64-decoded, digest-checked, verified, and written.
    let mock_server = MockServer::start().await;

    // Pending fixture uses a zero digest; the confirmed proof must match it.
    let proof_b64 = confirmed_proof_base64_for_digest([0u8; 32], true);
    Mock::given(method("GET"))
        .and(path_regex(r"/v1/stamp/ktcs_.*"))
        .respond_with(confirmed_response_with_proof(&proof_b64))
        .mount(&mock_server)
        .await;

    let proof_content = create_pending_proof_with_url(&mock_server.uri());
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["complete", proof_file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("upgraded"));

    // The input file must now be a complete proof.
    let written = std::fs::read(proof_file.path()).unwrap();
    let proof = ktcs_core::deserialize_proof(&written).unwrap();
    assert!(proof.is_complete());
}

#[tokio::test]
async fn test_complete_rejects_invalid_calendar_proof() {
    // Security: `complete` must run verify_proof on the returned proof and NOT
    // overwrite the input with an invalid/incomplete one.
    let mock_server = MockServer::start().await;

    let bad_proof_b64 = confirmed_proof_base64_for_digest([0u8; 32], false);
    Mock::given(method("GET"))
        .and(path_regex(r"/v1/stamp/ktcs_.*"))
        .respond_with(confirmed_response_with_proof(&bad_proof_b64))
        .mount(&mock_server)
        .await;

    let proof_content = create_pending_proof_with_url(&mock_server.uri());
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["complete", proof_file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid proof"));

    // The input file must be UNCHANGED (still the original pending proof).
    let after = std::fs::read(proof_file.path()).unwrap();
    assert_eq!(after, proof_content, "input must not be overwritten on rejection");
}

#[test]
fn test_complete_direct_scheme_is_not_fetched() {
    // A `direct://` pending URL cannot be completed via HTTP; the CLI must print
    // a clear message and NOT feed it to reqwest (which errors on the scheme).
    let proof_content =
        pending_proof_with_raw_url("direct://kaspa:qtestaddress?tx=deadbeefdeadbeef");
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["complete", proof_file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("direct on-chain"));
}

#[test]
fn test_complete_rejects_non_http_scheme() {
    // SSRF hardening: a non-http(s) scheme (e.g. file://) must be refused.
    let proof_content = pending_proof_with_raw_url("file:///etc/passwd");
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["complete", proof_file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported scheme"));
}

// ==================== Verify Exit Code Tests ====================

#[test]
fn test_verify_invalid_proof_exits_nonzero() {
    // A pending (incomplete) proof is not valid; `verify` must exit non-zero so
    // `ktcs verify && ...` is safe in scripts.
    let proof_content = create_pending_proof_with_url("https://calendar.ktcs.kaspa.org");
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["verify", proof_file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("INVALID"));
}

#[test]
fn test_verify_quiet_suppresses_banner() {
    // `--quiet` suppresses decorative output but still prints the essential
    // VALID/INVALID result.
    let proof_content = create_pending_proof_with_url("https://calendar.ktcs.kaspa.org");
    let mut proof_file = NamedTempFile::new().unwrap();
    proof_file.write_all(&proof_content).unwrap();
    proof_file.flush().unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["--quiet", "verify", proof_file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("INVALID"))
        .stdout(predicate::str::contains("KTCS Verification").not());
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
    // Use a path that does not yet exist (the key writer refuses to clobber).
    let dir = tempfile::TempDir::new().unwrap();
    let key_path = dir.path().join("wallet.key");

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "generate", "--output", key_path.to_str().unwrap()])
        .assert()
        .success();

    // Read the file and verify it contains a hex key
    let content = std::fs::read_to_string(&key_path).unwrap();
    let key = content.trim();
    assert_eq!(key.len(), 64);
    assert!(key.chars().all(|c| c.is_ascii_hexdigit()));

    // On Unix the key file must be created with 0600 permissions.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&key_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "key file must be 0600, got {:o}", mode & 0o777);
    }
}

#[test]
fn test_wallet_generate_refuses_to_overwrite() {
    // Security: `wallet generate -o` must never silently clobber an existing key
    // file (irreversible fund loss if it held a funded wallet's key).
    let dir = tempfile::TempDir::new().unwrap();
    let key_path = dir.path().join("existing.key");
    std::fs::write(&key_path, "PRE-EXISTING KEY MATERIAL").unwrap();

    Command::cargo_bin("ktcs")
        .unwrap()
        .args(["wallet", "generate", "--output", key_path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Refusing to overwrite"));

    // The original contents must be untouched.
    let content = std::fs::read_to_string(&key_path).unwrap();
    assert_eq!(content, "PRE-EXISTING KEY MATERIAL");
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
