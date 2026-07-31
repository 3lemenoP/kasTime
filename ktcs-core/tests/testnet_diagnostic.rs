//! Diagnostic tests to find working testnet endpoints
//!
//! Run with: cargo test --package ktcs-core --test testnet_diagnostic --features kaspa-client -- --nocapture

use std::time::Duration;
use tokio_tungstenite::connect_async;

/// Try various URL formats to find a working endpoint
#[tokio::test]
#[ignore = "requires live network"]
async fn diagnose_testnet_endpoints() {
    println!("\n");
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║       TESTNET ENDPOINT DIAGNOSTIC                          ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Known service IDs from PNN that are online for testnet-10:
    // - sid: 45cd71bf52cf29be (peers: 23, clients: 724)
    // - sid: 35eb6927b1cbf034 (peers: 30, clients: 962)

    let endpoints = vec![
        // PNN direct routing formats
        (
            "wss://pnn.kaspa.stream/borsh/testnet-10",
            "PNN borsh/testnet-10",
        ),
        (
            "wss://pnn.kaspa.stream/testnet-10/borsh",
            "PNN testnet-10/borsh",
        ),
        (
            "wss://pnn.kaspa.stream/wrpc/borsh/testnet-10",
            "PNN wrpc/borsh/testnet-10",
        ),
        (
            "wss://pnn.kaspa.stream/wrpc/testnet-10/borsh",
            "PNN wrpc/testnet-10/borsh",
        ),
        // Aspectron endpoints
        (
            "wss://kaspa.aspectron.com/borsh/testnet-10",
            "Aspectron borsh/testnet-10",
        ),
        (
            "wss://kaspa.aspectron.com/testnet-10/borsh",
            "Aspectron testnet-10/borsh",
        ),
        (
            "wss://kaspa.aspectron.com/wrpc/borsh/testnet-10",
            "Aspectron wrpc/borsh/testnet-10",
        ),
        // Short network names
        ("wss://pnn.kaspa.stream/borsh/tn10", "PNN borsh/tn10"),
        (
            "wss://kaspa.aspectron.com/borsh/tn10",
            "Aspectron borsh/tn10",
        ),
        // JSON encoding (as fallback)
        (
            "wss://pnn.kaspa.stream/json/testnet-10",
            "PNN json/testnet-10",
        ),
        (
            "wss://kaspa.aspectron.com/json/testnet-10",
            "Aspectron json/testnet-10",
        ),
        // Mainnet (to verify connectivity works at all)
        ("wss://pnn.kaspa.stream/borsh/mainnet", "PNN borsh/mainnet"),
        (
            "wss://kaspa.aspectron.com/borsh/mainnet",
            "Aspectron borsh/mainnet",
        ),
    ];

    let mut working_endpoints = Vec::new();

    for (url, name) in &endpoints {
        print!("Testing {:<40} ... ", name);

        let result = tokio::time::timeout(Duration::from_secs(10), connect_async(*url)).await;

        match result {
            Ok(Ok((ws, response))) => {
                println!("✓ OK ({})", response.status());
                working_endpoints.push((*url, *name));
                // Close the connection
                drop(ws);
            }
            Ok(Err(e)) => {
                let err_str = e.to_string();
                if err_str.contains("502") {
                    println!("✗ 502 Bad Gateway");
                } else if err_str.contains("404") {
                    println!("✗ 404 Not Found");
                } else if err_str.contains("protocol") {
                    println!("✗ Protocol error");
                } else {
                    println!("✗ {}", &err_str[..err_str.len().min(50)]);
                }
            }
            Err(_) => {
                println!("✗ Timeout (10s)");
            }
        }
    }

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!(
        "SUMMARY: {} working endpoint(s) found",
        working_endpoints.len()
    );
    println!("═══════════════════════════════════════════════════════════");

    for (url, name) in &working_endpoints {
        println!("  ✓ {} → {}", name, url);
    }

    if working_endpoints.is_empty() {
        println!();
        println!("  No working endpoints found!");
        println!("  This may indicate:");
        println!("  - Network connectivity issues");
        println!("  - All testnet public nodes are down");
        println!("  - The URL format has changed");
    }
}

/// Test mainnet connectivity (usually more reliable)
#[tokio::test]
#[ignore = "requires live network"]
async fn test_mainnet_connectivity() {
    println!("\n=== Testing Mainnet Connectivity ===\n");

    let endpoints = vec![
        "wss://pnn.kaspa.stream/borsh/mainnet",
        "wss://kaspa.aspectron.com/borsh/mainnet",
    ];

    for url in endpoints {
        print!("Connecting to {} ... ", url);

        match tokio::time::timeout(Duration::from_secs(15), connect_async(url)).await {
            Ok(Ok((mut ws, _))) => {
                println!("✓ Connected!");

                // Try sending a simple RPC request
                use futures_util::{SinkExt, StreamExt};
                use tokio_tungstenite::tungstenite::Message;

                let request = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "getBlockDagInfo",
                    "params": {},
                    "id": 1
                });

                print!("  Sending getBlockDagInfo request ... ");
                if ws.send(Message::Text(request.to_string())).await.is_ok() {
                    // Wait for response
                    match tokio::time::timeout(Duration::from_secs(10), ws.next()).await {
                        Ok(Some(Ok(Message::Text(text)))) => {
                            // Parse response
                            if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Some(result) = resp.get("result") {
                                    if let Some(network) = result.get("networkName") {
                                        println!("✓ Got response: network={}", network);
                                        if let Some(daa) = result.get("virtualDaaScore") {
                                            println!("    DAA Score: {}", daa);
                                        }
                                    } else {
                                        println!("✓ Got response (no networkName)");
                                    }
                                } else if let Some(err) = resp.get("error") {
                                    println!("✗ RPC error: {}", err);
                                } else {
                                    println!("✓ Got response: {}", &text[..text.len().min(100)]);
                                }
                            } else {
                                println!("✗ Invalid JSON response");
                            }
                        }
                        Ok(Some(Ok(other))) => {
                            println!("✗ Unexpected message type: {:?}", other);
                        }
                        Ok(Some(Err(e))) => {
                            println!("✗ WebSocket error: {}", e);
                        }
                        Ok(None) => {
                            println!("✗ Connection closed");
                        }
                        Err(_) => {
                            println!("✗ Response timeout");
                        }
                    }
                } else {
                    println!("✗ Failed to send request");
                }

                // Close
                let _ = ws.close(None).await;
            }
            Ok(Err(e)) => {
                println!("✗ Connection failed: {}", e);
            }
            Err(_) => {
                println!("✗ Connection timeout");
            }
        }
        println!();
    }
}

/// Test fetching the PNN node list
#[tokio::test]
#[ignore = "requires live network"]
async fn test_fetch_pnn_nodes() {
    println!("\n=== Fetching PNN Node List ===\n");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    match client.get("https://pnn.kaspa.stream/json").send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<Vec<serde_json::Value>>().await {
                    Ok(nodes) => {
                        println!("✓ Fetched {} nodes from PNN\n", nodes.len());

                        // Count by network and status
                        let mut mainnet_online = 0;
                        let mut testnet10_online = 0;
                        let mut testnet11_online = 0;

                        for node in &nodes {
                            let network =
                                node.get("network").and_then(|v| v.as_str()).unwrap_or("");
                            let status = node.get("status").and_then(|v| v.as_str()).unwrap_or("");
                            let is_online = status == "online" || status == "delegator";

                            match network {
                                "mainnet" if is_online => mainnet_online += 1,
                                "testnet-10" if is_online => testnet10_online += 1,
                                "testnet-11" if is_online => testnet11_online += 1,
                                _ => {}
                            }
                        }

                        println!("Network Summary:");
                        println!("  mainnet:    {} online nodes", mainnet_online);
                        println!("  testnet-10: {} online nodes", testnet10_online);
                        println!("  testnet-11: {} online nodes", testnet11_online);

                        // Show some testnet-10 details
                        println!("\nTestnet-10 Online Nodes:");
                        for node in &nodes {
                            let network =
                                node.get("network").and_then(|v| v.as_str()).unwrap_or("");
                            let status = node.get("status").and_then(|v| v.as_str()).unwrap_or("");
                            let encoding =
                                node.get("encoding").and_then(|v| v.as_str()).unwrap_or("");

                            if network == "testnet-10"
                                && (status == "online" || status == "delegator")
                                && encoding == "borsh"
                            {
                                let sid = node.get("sid").and_then(|v| v.as_str()).unwrap_or("?");
                                let uid = node.get("uid").and_then(|v| v.as_str()).unwrap_or("?");
                                let peers = node.get("peers").and_then(|v| v.as_u64()).unwrap_or(0);
                                let clients =
                                    node.get("clients").and_then(|v| v.as_u64()).unwrap_or(0);
                                println!(
                                    "  - SID: {}  UID: {}  peers: {}  clients: {}  status: {}",
                                    &sid[..sid.len().min(16)],
                                    &uid[..uid.len().min(16)],
                                    peers,
                                    clients,
                                    status
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!("✗ Failed to parse PNN response: {}", e);
                    }
                }
            } else {
                println!("✗ PNN returned status: {}", response.status());
            }
        }
        Err(e) => {
            println!("✗ Failed to fetch PNN: {}", e);
        }
    }
}

/// Test direct connection using JSON-RPC over WebSocket (more verbose)
#[tokio::test]
#[ignore = "requires live network"]
async fn test_json_rpc_connection() {
    println!("\n=== JSON-RPC WebSocket Connection Test ===\n");

    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    // Try testnet-10 with borsh encoding
    let url = "wss://pnn.kaspa.stream/borsh/testnet-10";
    println!("Target: {}\n", url);

    println!("1. Establishing WebSocket connection...");
    let ws_result = tokio::time::timeout(Duration::from_secs(15), connect_async(url)).await;

    let mut ws = match ws_result {
        Ok(Ok((ws, response))) => {
            println!("   ✓ Connected (HTTP {})", response.status());
            ws
        }
        Ok(Err(e)) => {
            println!("   ✗ Connection failed: {}", e);
            return;
        }
        Err(_) => {
            println!("   ✗ Connection timeout (15s)");
            return;
        }
    };

    println!("\n2. Sending getServerInfo request...");
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "getServerInfo",
        "params": {},
        "id": 1
    });

    if let Err(e) = ws.send(Message::Text(request.to_string())).await {
        println!("   ✗ Failed to send: {}", e);
        return;
    }
    println!("   ✓ Request sent");

    println!("\n3. Waiting for response...");
    match tokio::time::timeout(Duration::from_secs(10), ws.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => {
            println!("   ✓ Received response ({} bytes)", text.len());
            // Pretty print the JSON
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                println!("\n   Response:");
                let pretty = serde_json::to_string_pretty(&json).unwrap_or(text.clone());
                for line in pretty.lines().take(20) {
                    println!("   {}", line);
                }
                if pretty.lines().count() > 20 {
                    println!("   ... (truncated)");
                }
            } else {
                println!("   Raw: {}", &text[..text.len().min(200)]);
            }
        }
        Ok(Some(Ok(Message::Binary(data)))) => {
            println!("   ✓ Received binary response ({} bytes)", data.len());
            println!("   (Binary data - this is expected for Borsh encoding)");
            println!("   First 32 bytes: {:02x?}", &data[..data.len().min(32)]);
        }
        Ok(Some(Ok(other))) => {
            println!("   ? Unexpected message: {:?}", other);
        }
        Ok(Some(Err(e))) => {
            println!("   ✗ WebSocket error: {}", e);
        }
        Ok(None) => {
            println!("   ✗ Connection closed unexpectedly");
        }
        Err(_) => {
            println!("   ✗ Response timeout (10s)");
        }
    }

    // Close connection
    let _ = ws.close(None).await;
    println!("\n4. Connection closed");
}
