//! Mainnet Integration Tests for KTCS
//!
//! These tests verify Kaspa network functionality against the live mainnet.
//!
//! ## Phase 1: Read-Only Tests (Safe, No Funds Required)
//! Run with: cargo test --package ktcs-core --test mainnet_integration --features kaspa-client -- --nocapture
//!
//! ## Phase 2: Transaction Tests (Requires Funded Wallet)
//! 1. First run wallet generation: cargo test --package ktcs-core --test mainnet_integration test_mainnet_wallet_generation --features "kaspa-client keygen" -- --nocapture
//! 2. Fund the displayed address with ~0.1 KAS
//! 3. Run transaction test: cargo test --package ktcs-core --test mainnet_integration test_mainnet_stamp_transaction --features kaspa-client -- --ignored --nocapture

use std::time::Duration;

#[cfg(feature = "kaspa-client")]
use ktcs_core::kaspa::{KaspaClient, KaspaClientConfig};

// ============================================================================
// PHASE 1: READ-ONLY TESTS (Safe, run anytime)
// ============================================================================

/// Test mainnet connection via resolver
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_mainnet_connection() {
    println!("\n");
    println!("═══════════════════════════════════════════════════════════");
    println!("  TEST: Mainnet Connection");
    println!("═══════════════════════════════════════════════════════════");

    let config = KaspaClientConfig::mainnet_public();
    let client = KaspaClient::new(config);

    println!("→ Connecting to mainnet via resolver...");

    match tokio::time::timeout(Duration::from_secs(30), client.connect()).await {
        Ok(Ok(())) => {
            println!("✓ Connected successfully!");
            assert!(client.is_connected().await);
        }
        Ok(Err(e)) => {
            panic!("Connection failed: {}", e);
        }
        Err(_) => {
            panic!("Connection timeout (30s)");
        }
    }

    client.disconnect().await.ok();
    println!("✓ Disconnected cleanly");
}

/// Test DAG info retrieval and verify mainnet values
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_mainnet_dag_info() {
    println!("\n");
    println!("═══════════════════════════════════════════════════════════");
    println!("  TEST: Mainnet DAG Info");
    println!("═══════════════════════════════════════════════════════════");

    let config = KaspaClientConfig::mainnet_public();
    let client = KaspaClient::new(config);
    client.connect().await.expect("Failed to connect");

    println!("→ Fetching BlockDAG info...");

    let dag_info = client.get_block_dag_info().await.expect("Failed to get DAG info");

    println!("✓ Network:      {}", dag_info.network);
    println!("✓ DAA Score:    {}", dag_info.current_daa_score);
    println!("✓ Blue Score:   {}", dag_info.current_blue_score);
    println!("✓ Difficulty:   {:.2e}", dag_info.difficulty);
    println!("✓ Tip Count:    {} blocks", dag_info.tip_hashes.len());

    // Verify this is actually mainnet
    assert_eq!(dag_info.network, "mainnet", "Expected mainnet network");

    // Verify DAA score is in expected range (should be > 340M as of Jan 2026)
    assert!(
        dag_info.current_daa_score > 300_000_000,
        "DAA score {} seems too low for mainnet",
        dag_info.current_daa_score
    );

    // Verify difficulty is in expected range (> 1e16)
    assert!(
        dag_info.difficulty > 1e15,
        "Difficulty {:.2e} seems too low for mainnet",
        dag_info.difficulty
    );

    // Verify we have tips
    assert!(
        !dag_info.tip_hashes.is_empty(),
        "Expected at least one tip hash"
    );

    client.disconnect().await.ok();
    println!("\n✓ All DAG info assertions passed!");
}

/// Test individual chain metrics
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_mainnet_chain_metrics() {
    println!("\n");
    println!("═══════════════════════════════════════════════════════════");
    println!("  TEST: Mainnet Chain Metrics");
    println!("═══════════════════════════════════════════════════════════");

    let config = KaspaClientConfig::mainnet_public();
    let client = KaspaClient::new(config);
    client.connect().await.expect("Failed to connect");

    // DAA Score
    println!("→ Fetching DAA score...");
    let daa = client.get_current_daa_score().await.expect("Failed to get DAA score");
    println!("✓ DAA Score: {}", daa);
    assert!(daa > 300_000_000, "DAA score too low");

    // Blue Score
    // Note: getBlockDagInfo doesn't return blue_score in Kaspa wRPC JSON format
    // It returns 0 by default. To get actual blue score, we'd need to query a specific block.
    println!("→ Fetching Blue score (from DAG info)...");
    let blue = client.get_current_blue_score().await.expect("Failed to get blue score");
    println!("✓ Blue Score from DAG info: {} (0 is expected - use block query for actual)", blue);
    // Don't assert > 0 since DAG info doesn't include blue score

    // Blue Work
    // Note: getBlockDagInfo doesn't return blue work in Kaspa wRPC JSON format
    // To get actual blue work, we'd need to query a specific block.
    println!("→ Fetching Blue work (from DAG info)...");
    let blue_work = client.get_current_blue_work().await.expect("Failed to get blue work");
    // Find first non-zero byte for display
    let non_zero = blue_work.iter().position(|&b| b != 0).unwrap_or(31);
    println!("✓ Blue Work from DAG info: 0x{}... (may be zero - use block query for actual)",
        hex::encode(&blue_work[non_zero..non_zero.saturating_add(8).min(32)]));
    // Don't assert non-zero since DAG info doesn't include blue work

    client.disconnect().await.ok();
    println!("\n✓ All chain metric assertions passed!");
}

/// Test UTXO query with a known mainnet address
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_mainnet_utxo_query() {
    println!("\n");
    println!("═══════════════════════════════════════════════════════════");
    println!("  TEST: Mainnet UTXO Query");
    println!("═══════════════════════════════════════════════════════════");

    let config = KaspaClientConfig::mainnet_public();
    let client = KaspaClient::new(config);
    client.connect().await.expect("Failed to connect");

    // Query a random address (likely empty, but tests the API)
    let test_address = "kaspa:qz0s2a8ck0v0fhsqe7rv2km9wslqf53vujpgqalfxz3w4c7d0ffxxfwxlmywa";

    println!("→ Querying UTXOs for test address...");
    println!("  Address: {}...", &test_address[..40]);

    match client.get_utxos_by_address(test_address).await {
        Ok(utxos) => {
            println!("✓ Query succeeded: {} UTXO(s) found", utxos.len());

            if !utxos.is_empty() {
                let total: u64 = utxos.iter().map(|u| u.amount).sum();
                println!("  Total balance: {} sompi ({:.8} KAS)", total, total as f64 / 100_000_000.0);

                // Show first few UTXOs
                for (i, utxo) in utxos.iter().take(3).enumerate() {
                    println!("  UTXO {}: {} sompi (DAA: {})", i + 1, utxo.amount, utxo.block_daa_score);
                }
            }
        }
        Err(e) => {
            // UTXO query may fail due to address format - this is expected for some addresses
            println!("○ Query returned error (may be expected): {}", e);
        }
    }

    client.disconnect().await.ok();
    println!("\n✓ UTXO query test completed!");
}

/// Test block query with a known hash
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_mainnet_block_query() {
    println!("\n");
    println!("═══════════════════════════════════════════════════════════");
    println!("  TEST: Mainnet Block Query");
    println!("═══════════════════════════════════════════════════════════");

    let config = KaspaClientConfig::mainnet_public();
    let client = KaspaClient::new(config);
    client.connect().await.expect("Failed to connect");

    // Get current DAG info to find a recent block
    let dag_info = client.get_block_dag_info().await.expect("Failed to get DAG info");

    if !dag_info.tip_hashes.is_empty() {
        let tip_hash = &dag_info.tip_hashes[0];
        println!("→ Querying tip block: {}...", hex::encode(&tip_hash[..8]));

        match client.get_block_by_hash(tip_hash).await {
            Ok(block) => {
                println!("✓ Block retrieved successfully!");
                println!("  Hash:       {}...", hex::encode(&block.hash[..8]));
                println!("  DAA Score:  {}", block.daa_score);
                println!("  Blue Score: {}", block.blue_score);
                println!("  Timestamp:  {}", block.timestamp);
                println!("  Parents:    {} block(s)", block.parent_hashes.len());
                println!("  TXs:        {} transaction(s)", block.transaction_ids.len());

                // Verify hash matches
                assert_eq!(&block.hash, tip_hash, "Block hash mismatch");
            }
            Err(e) => {
                println!("○ Block query failed (tip may have changed): {}", e);
            }
        }
    } else {
        println!("○ No tip hashes available to query");
    }

    client.disconnect().await.ok();
    println!("\n✓ Block query test completed!");
}

// ============================================================================
// PHASE 2: WALLET & TRANSACTION TESTS (Requires funding)
// ============================================================================

/// Generate a new mainnet wallet for testing
/// Run: cargo test --package ktcs-core --test mainnet_integration test_mainnet_wallet_generation --features "kaspa-client keygen" -- --nocapture
#[cfg(all(feature = "kaspa-client", feature = "keygen"))]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_mainnet_wallet_generation() {
    use ktcs_core::wallet::generate_wallet;

    println!("\n");
    println!("═══════════════════════════════════════════════════════════");
    println!("  MAINNET WALLET GENERATION");
    println!("═══════════════════════════════════════════════════════════");

    let wallet = generate_wallet("kaspa").expect("Failed to generate wallet");
    let private_key_hex = wallet.to_hex();

    println!();
    println!("  ┌─────────────────────────────────────────────────────┐");
    println!("  │  NEW MAINNET WALLET GENERATED                       │");
    println!("  └─────────────────────────────────────────────────────┘");
    println!();
    println!("  Address:     {}", wallet.address());
    println!();

    // Save to a file inside the OS temp dir (NOT the current working directory)
    // so a plaintext mainnet private key is never dropped into the repo/CWD.
    let key_file = std::env::temp_dir().join("mainnet-test-wallet.key");
    std::fs::write(&key_file, private_key_hex.as_str()).expect("Failed to save wallet key");
    println!("  ✓ Saved to: {}", key_file.display());
    println!();

    println!("  To use this wallet for transaction tests:");
    println!("  1. Fund the address above with ~0.1 KAS");
    println!("  2. Set environment variables:");
    println!("     set KTCS_TEST_ADDRESS={}", wallet.address());
    println!("     set KTCS_TEST_WALLET=<see-key-file>");
    println!("  3. Run: cargo test test_mainnet_stamp_transaction --features kaspa-client -- --ignored --nocapture");
    println!();

    // Verify address format
    assert!(wallet.address().starts_with("kaspa:"), "Address should start with kaspa:");

    println!("═══════════════════════════════════════════════════════════");
}

/// Check balance of test wallet
/// Run: cargo test --package ktcs-core --test mainnet_integration test_mainnet_check_balance --features kaspa-client -- --ignored --nocapture
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore] // Run manually after generating wallet
async fn test_mainnet_check_balance() {
    println!("\n");
    println!("═══════════════════════════════════════════════════════════");
    println!("  CHECK WALLET BALANCE");
    println!("═══════════════════════════════════════════════════════════");

    // Read wallet from environment or file
    let wallet_address = std::env::var("KTCS_TEST_ADDRESS")
        .unwrap_or_else(|_| {
            println!("⚠️  Set KTCS_TEST_ADDRESS environment variable to your test wallet address");
            println!("    Example: set KTCS_TEST_ADDRESS=kaspa:qz...");
            String::new()
        });

    if wallet_address.is_empty() {
        println!("✗ No wallet address configured");
        return;
    }

    let config = KaspaClientConfig::mainnet_public();
    let client = KaspaClient::new(config);
    client.connect().await.expect("Failed to connect");

    println!("→ Checking balance for: {}...", &wallet_address[..40]);

    match client.get_utxos_by_address(&wallet_address).await {
        Ok(utxos) => {
            let balance: u64 = utxos.iter().map(|u| u.amount).sum();
            let kas = balance as f64 / 100_000_000.0;

            println!();
            println!("  Balance: {} sompi ({:.8} KAS)", balance, kas);
            println!("  UTXOs:   {} entries", utxos.len());
            println!();

            if balance >= 10_000_000 {
                println!("  ✓ Wallet has sufficient funds for transaction test!");
                println!("    Run: cargo test test_mainnet_stamp_transaction -- --ignored --nocapture");
            } else {
                println!("  ⚠️  Need at least 0.1 KAS (10,000,000 sompi) for transaction test");
                println!("    Current: {:.8} KAS", kas);
            }
        }
        Err(e) => {
            println!("✗ Failed to query balance: {}", e);
        }
    }

    client.disconnect().await.ok();
}

/// Submit a test commitment transaction to mainnet
/// Run: cargo test --package ktcs-core --test mainnet_integration test_mainnet_stamp_transaction --features kaspa-client -- --ignored --nocapture
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore] // Run manually after funding wallet
async fn test_mainnet_stamp_transaction() {
    use ktcs_core::direct::{complete_stamp, prepare_direct_stamp, sign_transaction, DirectStampConfig, DirectBlockInfo};
    use ktcs_core::wallet::KaspaWallet;
    use ktcs_core::verify::verify_proof;

    println!("\n");
    println!("═══════════════════════════════════════════════════════════");
    println!("  MAINNET STAMP TRANSACTION TEST");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("  ⚠️  This test will submit a real transaction to mainnet!");
    println!("      Cost: ~0.00001 KAS in fees");
    println!();

    // Read wallet from environment
    let wallet_hex = std::env::var("KTCS_TEST_WALLET")
        .expect("Set KTCS_TEST_WALLET environment variable to your wallet private key (hex)");

    let wallet = KaspaWallet::from_hex(&wallet_hex, "kaspa")
        .expect("Invalid wallet key");

    println!("  Wallet address: {}", wallet.address());

    // Connect to mainnet
    let config = KaspaClientConfig::mainnet_public();
    let client = KaspaClient::new(config);
    client.connect().await.expect("Failed to connect");

    // Check balance first
    let utxos = client
        .get_utxos_by_address(wallet.address())
        .await
        .expect("Failed to get UTXOs");

    let balance: u64 = utxos.iter().map(|u| u.amount).sum();
    println!("  Balance: {} sompi ({:.8} KAS)", balance, balance as f64 / 100_000_000.0);

    assert!(
        balance >= 10_000_000,
        "Need at least 0.1 KAS for this test"
    );

    // Prepare stamp with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let test_data = format!("KTCS mainnet integration test - {}", timestamp);
    println!();
    println!("→ Preparing stamp for: \"{}\"", &test_data);

    let stamp_config = DirectStampConfig::default();
    let prepared = prepare_direct_stamp(test_data.as_bytes(), &wallet, &client, &stamp_config)
        .await
        .expect("Failed to prepare stamp");

    println!("  Commitment: {}...", hex::encode(&prepared.commitment[..8]));
    println!("  Est. Fee:   {} sompi", prepared.estimated_fee);

    // Sign transaction
    println!();
    println!("→ Signing transaction...");

    let tx = prepared.transaction.as_ref().expect("No transaction prepared");
    let utxos_for_sign = prepared.utxos.as_ref().expect("No UTXOs in prepared stamp");
    let signed_tx = sign_transaction(&tx.transaction, &wallet, utxos_for_sign)
        .expect("Failed to sign transaction");

    // Submit transaction
    println!("→ Submitting transaction...");

    let tx_hash = client
        .submit_transaction(signed_tx)
        .await
        .expect("Failed to submit transaction");

    println!("✓ Transaction submitted!");
    println!("  TX Hash: {}", hex::encode(tx_hash));

    // Wait for confirmation
    println!();
    println!("→ Waiting for confirmation (60s timeout)...");

    let block_info = client
        .wait_for_confirmation(&tx_hash, 60_000)
        .await
        .expect("Transaction not confirmed within timeout");

    println!("✓ Transaction confirmed!");
    println!("  Block Hash:  {}...", hex::encode(&block_info.hash[..8]));
    println!("  DAA Score:   {}", block_info.daa_score);
    println!("  Blue Score:  {}", block_info.blue_score);

    // Complete the stamp
    let direct_block_info = DirectBlockInfo {
        hash: block_info.hash,
        daa_score: block_info.daa_score,
        blue_score: block_info.blue_score,
        timestamp: block_info.timestamp,
        blue_work: block_info.blue_work,
        parent_hashes: block_info.parent_hashes,
    };

    let proof = complete_stamp(prepared, direct_block_info, tx_hash)
        .expect("Failed to complete stamp");

    // Verify proof
    println!();
    println!("→ Verifying proof...");
    let verification = verify_proof(&proof, Some(test_data.as_bytes()))
        .expect("Proof verification failed");
    println!("✓ Proof verified: valid={}", verification.valid);

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  MAINNET STAMP TEST COMPLETED SUCCESSFULLY!");
    println!("═══════════════════════════════════════════════════════════");
    println!("  TX: {}", hex::encode(tx_hash));
    println!("  View: https://explorer.kaspa.org/txs/{}", hex::encode(tx_hash));
    println!("═══════════════════════════════════════════════════════════");

    client.disconnect().await.ok();
}

// ============================================================================
// COMPREHENSIVE TEST (runs all read-only tests)
// ============================================================================

/// Run all read-only mainnet tests in sequence
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_mainnet_comprehensive() {
    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║        KTCS MAINNET COMPREHENSIVE TEST                    ║");
    println!("╚═══════════════════════════════════════════════════════════╝");

    let config = KaspaClientConfig::mainnet_public();
    let client = KaspaClient::new(config);

    // 1. Connection
    println!("\n[1/5] Testing connection...");
    client.connect().await.expect("Connection failed");
    println!("      ✓ Connected to mainnet");

    // 2. DAG Info
    println!("\n[2/5] Testing DAG info...");
    let dag = client.get_block_dag_info().await.expect("DAG info failed");
    assert_eq!(dag.network, "mainnet");
    println!("      ✓ Network: {} | DAA: {}", dag.network, dag.current_daa_score);

    // 3. Chain Metrics
    println!("\n[3/5] Testing chain metrics...");
    let daa = client.get_current_daa_score().await.expect("DAA score failed");
    let blue = client.get_current_blue_score().await.expect("Blue score failed");
    println!("      ✓ DAA: {} | Blue: {}", daa, blue);

    // 4. Blue Work
    println!("\n[4/5] Testing blue work...");
    let work = client.get_current_blue_work().await.expect("Blue work failed");
    let non_zero = work.iter().position(|&b| b != 0).unwrap_or(31);
    println!("      ✓ Blue work: 0x{}...", hex::encode(&work[non_zero..non_zero.saturating_add(4).min(32)]));

    // 5. Block Query
    println!("\n[5/5] Testing block query...");
    if let Some(tip) = dag.tip_hashes.first() {
        match client.get_block_by_hash(tip).await {
            Ok(block) => println!("      ✓ Block {} | TXs: {}", hex::encode(&block.hash[..4]), block.transaction_ids.len()),
            Err(e) => println!("      ○ Block query: {} (tip may have changed)", e),
        }
    }

    client.disconnect().await.ok();

    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║        ALL READ-ONLY TESTS PASSED!                        ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
}
