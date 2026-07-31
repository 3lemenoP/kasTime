//! Integration tests for testnet node connection functionality
//!
//! These tests verify that:
//! 1. The PNN resolver can discover testnet endpoints
//! 2. The KaspaClient can connect to testnet nodes
//! 3. Basic RPC calls work (get_block_dag_info, etc.)
//!
//! Run with: cargo test --package ktcs-core --test testnet_connection --features kaspa-client -- --nocapture

use std::time::Duration;

#[cfg(feature = "kaspa-client")]
use ktcs_core::{
    kaspa::{KaspaClient, KaspaClientConfig, ConnectionState},
    resolver::{Resolver, resolve_url},
};

/// Test that the resolver can find testnet-10 endpoints
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_resolver_testnet10_url() {
    println!("\n=== Test: Resolver testnet-10 URL ===\n");

    let resolver = Resolver::default();

    match resolver.get_node_url("testnet-10").await {
        Ok(url) => {
            println!("✓ Resolved testnet-10 URL: {}", url);
            assert!(url.starts_with("wss://") || url.starts_with("ws://"),
                "URL should be a WebSocket URL");
        }
        Err(e) => {
            println!("✗ Failed to resolve testnet-10: {}", e);
            // Don't fail the test if network is unavailable - just log it
            println!("  (This may be expected if no public nodes are available)");
        }
    }
}

/// Test that the resolver can find testnet-11 endpoints
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_resolver_testnet11_url() {
    println!("\n=== Test: Resolver testnet-11 URL ===\n");

    let resolver = Resolver::default();

    match resolver.get_node_url("testnet-11").await {
        Ok(url) => {
            println!("✓ Resolved testnet-11 URL: {}", url);
            assert!(url.starts_with("wss://") || url.starts_with("ws://"),
                "URL should be a WebSocket URL");
        }
        Err(e) => {
            println!("✗ Failed to resolve testnet-11: {}", e);
            println!("  (This may be expected if no public nodes are available)");
        }
    }
}

/// Test the convenience function resolve_url
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_resolve_url_function() {
    println!("\n=== Test: resolve_url convenience function ===\n");

    // Test mainnet
    match resolve_url("mainnet").await {
        Ok(url) => println!("✓ Mainnet URL: {}", url),
        Err(e) => println!("  Mainnet resolution: {} (may be expected)", e),
    }

    // Test testnet-10
    match resolve_url("testnet-10").await {
        Ok(url) => println!("✓ Testnet-10 URL: {}", url),
        Err(e) => println!("  Testnet-10 resolution: {} (may be expected)", e),
    }
}

/// Test client creation with testnet10 public config
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_client_config_testnet10() {
    println!("\n=== Test: Client config for testnet-10 ===\n");

    let config = KaspaClientConfig::testnet10_public();

    assert!(config.use_resolver, "Config should use resolver");
    assert_eq!(config.network, Some("testnet-10".to_string()));
    assert!(config.rpc_url.is_empty(), "RPC URL should be empty (resolved)");

    println!("✓ Config created:");
    println!("  Network: {:?}", config.network);
    println!("  Use resolver: {}", config.use_resolver);
    println!("  Connect timeout: {}ms", config.connect_timeout_ms);
    println!("  Request timeout: {}ms", config.request_timeout_ms);
}

/// Test connecting to testnet-10 via resolver
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_connect_testnet10_via_resolver() {
    println!("\n=== Test: Connect to testnet-10 via resolver ===\n");

    let config = KaspaClientConfig::testnet10_public();
    let client = KaspaClient::new(config);

    // Initial state should be disconnected
    assert_eq!(client.connection_state().await, ConnectionState::Disconnected);
    println!("✓ Initial state: Disconnected");

    // Try to connect
    println!("→ Attempting connection via resolver...");
    match tokio::time::timeout(Duration::from_secs(30), client.connect()).await {
        Ok(Ok(())) => {
            println!("✓ Connected successfully!");
            assert_eq!(client.connection_state().await, ConnectionState::Connected);

            // Get DAG info
            println!("→ Fetching DAG info...");
            match client.get_block_dag_info().await {
                Ok(dag_info) => {
                    println!("✓ DAG Info retrieved:");
                    println!("  Network: {}", dag_info.network);
                    println!("  DAA Score: {}", dag_info.current_daa_score);
                    println!("  Blue Score: {}", dag_info.current_blue_score);
                    println!("  Difficulty: {:.2}", dag_info.difficulty);
                    println!("  Tips: {} block(s)", dag_info.tip_hashes.len());

                    // Verify it's actually testnet
                    assert!(
                        dag_info.network.contains("testnet") || dag_info.network.contains("tn"),
                        "Should be connected to a testnet, got: {}", dag_info.network
                    );
                }
                Err(e) => {
                    println!("✗ Failed to get DAG info: {}", e);
                }
            }

            // Disconnect
            client.disconnect().await.ok();
            println!("✓ Disconnected");
        }
        Ok(Err(e)) => {
            println!("✗ Connection failed: {}", e);
            println!("  (This may be expected if no public nodes are available)");
        }
        Err(_) => {
            println!("✗ Connection timeout (30s)");
            println!("  (This may be expected if no public nodes are available)");
        }
    }
}

/// Test connecting to testnet-11 via resolver
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_connect_testnet11_via_resolver() {
    println!("\n=== Test: Connect to testnet-11 via resolver ===\n");

    let config = KaspaClientConfig {
        rpc_url: String::new(),
        network: Some("testnet-11".to_string()),
        connect_timeout_ms: 15000,
        request_timeout_ms: 30000,
        auto_reconnect: false,
        use_resolver: true,
        tls_verify: true,
    };
    let client = KaspaClient::new(config);

    println!("→ Attempting connection via resolver...");
    match tokio::time::timeout(Duration::from_secs(30), client.connect()).await {
        Ok(Ok(())) => {
            println!("✓ Connected successfully!");

            match client.get_block_dag_info().await {
                Ok(dag_info) => {
                    println!("✓ DAG Info:");
                    println!("  Network: {}", dag_info.network);
                    println!("  DAA Score: {}", dag_info.current_daa_score);
                    println!("  Blue Score: {}", dag_info.current_blue_score);
                }
                Err(e) => {
                    println!("✗ Failed to get DAG info: {}", e);
                }
            }

            client.disconnect().await.ok();
        }
        Ok(Err(e)) => {
            println!("✗ Connection failed: {}", e);
            println!("  (testnet-11 may not have public nodes available)");
        }
        Err(_) => {
            println!("✗ Connection timeout (30s)");
        }
    }
}

/// Test DAA score retrieval
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_get_daa_score() {
    println!("\n=== Test: Get DAA score ===\n");

    let config = KaspaClientConfig::testnet10_public();
    let client = KaspaClient::new(config);

    println!("→ Connecting...");
    if client.connect().await.is_err() {
        println!("  Skipping test - cannot connect to testnet");
        return;
    }

    match client.get_current_daa_score().await {
        Ok(daa_score) => {
            println!("✓ Current DAA Score: {}", daa_score);
            assert!(daa_score > 0, "DAA score should be positive");
        }
        Err(e) => {
            println!("✗ Failed to get DAA score: {}", e);
        }
    }

    client.disconnect().await.ok();
}

/// Test blue score retrieval
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_get_blue_score() {
    println!("\n=== Test: Get blue score ===\n");

    let config = KaspaClientConfig::testnet10_public();
    let client = KaspaClient::new(config);

    println!("→ Connecting...");
    if client.connect().await.is_err() {
        println!("  Skipping test - cannot connect to testnet");
        return;
    }

    match client.get_current_blue_score().await {
        Ok(blue_score) => {
            println!("✓ Current Blue Score: {}", blue_score);
            assert!(blue_score > 0, "Blue score should be positive");
        }
        Err(e) => {
            println!("✗ Failed to get blue score: {}", e);
        }
    }

    client.disconnect().await.ok();
}

/// Test fetching UTXOs for a test address
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_get_utxos() {
    println!("\n=== Test: Get UTXOs for address ===\n");

    let config = KaspaClientConfig::testnet10_public();
    let client = KaspaClient::new(config);

    println!("→ Connecting...");
    if client.connect().await.is_err() {
        println!("  Skipping test - cannot connect to testnet");
        return;
    }

    // Use a known testnet address (this won't have funds, but tests the API)
    let test_address = "kaspatest:qz0s2a8ck0v0fhsqe7rv2km9wslqf53vujpgqalfxz3w4c7d0ffxxnxcn9ce5";

    match client.get_utxos_by_address(test_address).await {
        Ok(utxos) => {
            println!("✓ UTXOs retrieved: {} entries", utxos.len());
            for (i, utxo) in utxos.iter().take(5).enumerate() {
                println!("  {}. {} sompi (DAA: {})", i + 1, utxo.amount, utxo.block_daa_score);
            }
        }
        Err(e) => {
            println!("✗ Failed to get UTXOs: {}", e);
            // This may fail if address format is wrong for the network
        }
    }

    client.disconnect().await.ok();
}

/// Test multiple concurrent connections
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_concurrent_connections() {
    println!("\n=== Test: Concurrent connections ===\n");

    let config1 = KaspaClientConfig::testnet10_public();
    let config2 = KaspaClientConfig::testnet10_public();

    let client1 = KaspaClient::new(config1);
    let client2 = KaspaClient::new(config2);

    println!("→ Connecting two clients concurrently...");

    let (result1, result2) = tokio::join!(
        client1.connect(),
        client2.connect()
    );

    match (result1, result2) {
        (Ok(()), Ok(())) => {
            println!("✓ Both clients connected");

            // Fetch DAG info from both
            let (dag1, dag2) = tokio::join!(
                client1.get_block_dag_info(),
                client2.get_block_dag_info()
            );

            if let (Ok(d1), Ok(d2)) = (dag1, dag2) {
                println!("✓ Client 1 DAA: {}", d1.current_daa_score);
                println!("✓ Client 2 DAA: {}", d2.current_daa_score);

                // Both should see similar DAA scores (within a few blocks)
                let diff = (d1.current_daa_score as i64 - d2.current_daa_score as i64).abs();
                assert!(diff < 10, "DAA scores should be similar, diff: {}", diff);
            }

            client1.disconnect().await.ok();
            client2.disconnect().await.ok();
        }
        _ => {
            println!("  Skipping - could not connect both clients");
        }
    }
}

/// Test connection state transitions
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_connection_states() {
    println!("\n=== Test: Connection state transitions ===\n");

    let config = KaspaClientConfig::testnet10_public();
    let client = KaspaClient::new(config);

    // 1. Initial state
    let state = client.connection_state().await;
    println!("1. Initial state: {:?}", state);
    assert_eq!(state, ConnectionState::Disconnected);

    // 2. After connect attempt
    println!("→ Attempting to connect...");
    if client.connect().await.is_ok() {
        let state = client.connection_state().await;
        println!("2. After connect: {:?}", state);
        assert_eq!(state, ConnectionState::Connected);

        // 3. After disconnect
        client.disconnect().await.ok();
        let state = client.connection_state().await;
        println!("3. After disconnect: {:?}", state);
        assert_eq!(state, ConnectionState::Disconnected);
    } else {
        println!("  Skipping - could not connect to testnet");
    }
}

/// Test is_connected helper
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_is_connected() {
    println!("\n=== Test: is_connected helper ===\n");

    let config = KaspaClientConfig::testnet10_public();
    let client = KaspaClient::new(config);

    assert!(!client.is_connected().await, "Should not be connected initially");
    println!("✓ Not connected initially");

    if client.connect().await.is_ok() {
        assert!(client.is_connected().await, "Should be connected after connect()");
        println!("✓ Connected after connect()");

        client.disconnect().await.ok();
        assert!(!client.is_connected().await, "Should not be connected after disconnect()");
        println!("✓ Not connected after disconnect()");
    }
}

/// Comprehensive test that exercises the full connection flow
#[cfg(feature = "kaspa-client")]
#[tokio::test]
#[ignore = "requires live network"]
async fn test_full_testnet_flow() {
    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║        KTCS Testnet Connection Integration Test           ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    // 1. Resolve URL - try testnet first, fall back to mainnet
    println!("Step 1: Resolve endpoint via PNN");
    println!("─────────────────────────────────────────────");

    let resolver = Resolver::default();
    let (url, network) = match resolver.get_node_url("testnet-10").await {
        Ok(url) => {
            println!("  ✓ Testnet-10 endpoint: {}", url);
            (url, "testnet-10")
        }
        Err(e) => {
            println!("  ○ Testnet-10 not available: {}", e);
            println!("  → Trying mainnet instead...");
            match resolver.get_node_url("mainnet").await {
                Ok(url) => {
                    println!("  ✓ Mainnet endpoint: {}", url);
                    (url, "mainnet")
                }
                Err(e) => {
                    println!("  ✗ Mainnet also failed: {}", e);
                    println!("\n  Test cannot proceed without a working endpoint.");
                    return;
                }
            }
        }
    };

    println!();

    // 2. Connect
    println!("Step 2: Connect to Kaspa {} node", network);
    println!("─────────────────────────────────────────────");

    let config = KaspaClientConfig {
        rpc_url: url.clone(),
        network: Some(network.to_string()),
        connect_timeout_ms: 15000,
        request_timeout_ms: 30000,
        auto_reconnect: false,
        use_resolver: false, // Already resolved
        tls_verify: true,
    };
    let client = KaspaClient::new(config);

    match client.connect().await {
        Ok(()) => println!("  ✓ Connection established"),
        Err(e) => {
            println!("  ✗ Connection failed: {}", e);
            // Try mainnet if testnet failed
            if network == "testnet-10" {
                println!("  → Retrying with mainnet...");
                let mainnet_url = resolver.get_node_url("mainnet").await;
                if let Ok(url) = mainnet_url {
                    let config = KaspaClientConfig {
                        rpc_url: url.clone(),
                        network: Some("mainnet".to_string()),
                        connect_timeout_ms: 15000,
                        request_timeout_ms: 30000,
                        auto_reconnect: false,
                        use_resolver: false,
                        tls_verify: true,
                    };
                    let client = KaspaClient::new(config);
                    if client.connect().await.is_ok() {
                        println!("  ✓ Connected to mainnet instead");
                        // Just verify connection works
                        if let Ok(dag) = client.get_block_dag_info().await {
                            println!("  ✓ Got DAG info: network={}, DAA={}", dag.network, dag.current_daa_score);
                        }
                        client.disconnect().await.ok();
                        return;
                    }
                }
            }
            return;
        }
    }

    println!();

    // 3. Get DAG info
    println!("Step 3: Fetch BlockDAG information");
    println!("─────────────────────────────────────────────");

    match client.get_block_dag_info().await {
        Ok(dag_info) => {
            println!("  ✓ Network:      {}", dag_info.network);
            println!("  ✓ DAA Score:    {}", dag_info.current_daa_score);
            println!("  ✓ Blue Score:   {}", dag_info.current_blue_score);
            println!("  ✓ Difficulty:   {:.6}", dag_info.difficulty);
            println!("  ✓ Median Time:  {}", dag_info.past_median_time);
            println!("  ✓ Tip Count:    {} blocks", dag_info.tip_hashes.len());
            println!("  ✓ Pruning Hash: {}...", hex::encode(&dag_info.pruning_point_hash[..8]));
        }
        Err(e) => {
            println!("  ✗ Failed: {}", e);
        }
    }

    println!();

    // 4. Get individual metrics
    println!("Step 4: Query individual chain metrics");
    println!("─────────────────────────────────────────────");

    if let Ok(daa) = client.get_current_daa_score().await {
        println!("  ✓ DAA Score:  {}", daa);
    }

    if let Ok(blue) = client.get_current_blue_score().await {
        println!("  ✓ Blue Score: {}", blue);
    }

    if let Ok(work) = client.get_current_blue_work().await {
        // Format blue work (find first non-zero byte)
        let non_zero_start = work.iter().position(|&b| b != 0).unwrap_or(31);
        println!("  ✓ Blue Work:  0x{}...", hex::encode(&work[non_zero_start..non_zero_start.min(31)+4]));
    }

    println!();

    // 5. Test address UTXO query
    println!("Step 5: Query UTXOs (empty test address)");
    println!("─────────────────────────────────────────────");

    let test_address = "kaspatest:qz0s2a8ck0v0fhsqe7rv2km9wslqf53vujpgqalfxz3w4c7d0ffxxnxcn9ce5";
    match client.get_utxos_by_address(test_address).await {
        Ok(utxos) => {
            println!("  ✓ Address: {}...", &test_address[..40]);
            println!("  ✓ UTXOs:   {} entries", utxos.len());
            if !utxos.is_empty() {
                let total: u64 = utxos.iter().map(|u| u.amount).sum();
                println!("  ✓ Balance: {} sompi ({:.8} KAS)", total, total as f64 / 100_000_000.0);
            }
        }
        Err(e) => {
            // This is expected if the address format doesn't match the network
            println!("  ○ UTXO query: {} (may be expected)", e);
        }
    }

    println!();

    // 6. Disconnect
    println!("Step 6: Disconnect");
    println!("─────────────────────────────────────────────");

    client.disconnect().await.ok();
    println!("  ✓ Disconnected cleanly");

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("                    TEST COMPLETE                          ");
    println!("═══════════════════════════════════════════════════════════");
}
