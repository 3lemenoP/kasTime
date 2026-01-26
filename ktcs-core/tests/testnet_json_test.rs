//! Test JSON-encoded wRPC endpoints with JSON-RPC
//!
//! Run with: cargo test --package ktcs-core --test testnet_json_test --features kaspa-client -- --nocapture

use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};

/// Test JSON-encoded mainnet endpoint with JSON-RPC
#[tokio::test]
async fn test_json_endpoint_mainnet() {
    println!("\n=== Testing JSON-encoded Mainnet Endpoint ===\n");

    // Try JSON encoding instead of Borsh
    let url = "wss://kaspa.aspectron.com/wrpc/json/mainnet";
    println!("Connecting to: {}", url);

    let ws_result = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async(url)
    ).await;

    let mut ws = match ws_result {
        Ok(Ok((ws, response))) => {
            println!("✓ Connected (HTTP {})", response.status());
            ws
        }
        Ok(Err(e)) => {
            println!("✗ Connection failed: {}", e);
            return;
        }
        Err(_) => {
            println!("✗ Connection timeout");
            return;
        }
    };

    // Try sending a JSON-RPC request
    println!("\nSending getBlockDagInfo request...");
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "getBlockDagInfo",
        "params": {},
        "id": 1
    });

    if let Err(e) = ws.send(Message::Text(request.to_string())).await {
        println!("✗ Failed to send: {}", e);
        return;
    }
    println!("✓ Request sent");

    // Wait for response
    println!("\nWaiting for response (10s timeout)...");
    match tokio::time::timeout(Duration::from_secs(10), ws.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => {
            println!("✓ Received text response ({} bytes)", text.len());
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                let pretty = serde_json::to_string_pretty(&json).unwrap();
                println!("\nResponse:");
                for (i, line) in pretty.lines().enumerate() {
                    if i < 25 {
                        println!("  {}", line);
                    }
                }
                if pretty.lines().count() > 25 {
                    println!("  ... (truncated)");
                }
            }
        }
        Ok(Some(Ok(Message::Binary(data)))) => {
            println!("✓ Received binary response ({} bytes)", data.len());
            println!("  (JSON endpoint returned binary - unexpected)");
            println!("  First 64 bytes: {:02x?}", &data[..data.len().min(64)]);
        }
        Ok(Some(Ok(other))) => {
            println!("? Received: {:?}", other);
        }
        Ok(Some(Err(e))) => {
            println!("✗ WebSocket error: {}", e);
        }
        Ok(None) => {
            println!("✗ Connection closed");
        }
        Err(_) => {
            println!("✗ Response timeout (10s)");
            println!("  This may indicate protocol incompatibility");
        }
    }

    let _ = ws.close(None).await;
}

/// Test different RPC method formats
#[tokio::test]
async fn test_kaspa_rpc_methods() {
    println!("\n=== Testing Kaspa RPC Method Formats ===\n");

    let url = "wss://kaspa.aspectron.com/wrpc/json/mainnet";

    let ws_result = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async(url)
    ).await;

    let mut ws = match ws_result {
        Ok(Ok((ws, _))) => {
            println!("✓ Connected to {}", url);
            ws
        }
        Ok(Err(e)) => {
            println!("✗ Connection failed: {}", e);
            return;
        }
        Err(_) => {
            println!("✗ Timeout");
            return;
        }
    };

    // Try different method name formats
    let methods = vec![
        ("getBlockDagInfo", serde_json::json!({})),
        ("get_block_dag_info", serde_json::json!({})),
        ("GetBlockDagInfo", serde_json::json!({})),
        ("getServerInfo", serde_json::json!({})),
        ("getInfo", serde_json::json!({})),
        ("ping", serde_json::json!({})),
    ];

    for (method, params) in methods {
        print!("Testing method '{}' ... ", method);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        if ws.send(Message::Text(request.to_string())).await.is_err() {
            println!("✗ Send failed");
            continue;
        }

        match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if text.contains("result") {
                    println!("✓ Got result");
                } else if text.contains("error") {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(error) = json.get("error") {
                            let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("?");
                            println!("✗ Error: {}", msg);
                        } else {
                            println!("? Unexpected: {}", &text[..text.len().min(50)]);
                        }
                    }
                } else {
                    println!("? Response: {}", &text[..text.len().min(50)]);
                }
            }
            Ok(Some(Ok(Message::Binary(_)))) => {
                println!("✗ Binary response (protocol mismatch)");
            }
            Ok(Some(Err(e))) => {
                println!("✗ Error: {}", e);
            }
            Ok(None) => {
                println!("✗ Connection closed");
                break;
            }
            Err(_) => {
                println!("✗ Timeout");
            }
            _ => {
                println!("? Other message type");
            }
        }
    }

    let _ = ws.close(None).await;
}

/// Test with Kaspa-style JSON request format (not JSON-RPC)
#[tokio::test]
async fn test_kaspa_native_format() {
    println!("\n=== Testing Kaspa Native JSON Format ===\n");

    let url = "wss://kaspa.aspectron.com/wrpc/json/mainnet";

    let ws_result = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async(url)
    ).await;

    let mut ws = match ws_result {
        Ok(Ok((ws, _))) => {
            println!("✓ Connected");
            ws
        }
        _ => {
            println!("✗ Connection failed");
            return;
        }
    };

    // Try Kaspa-native format (not JSON-RPC)
    let native_formats = vec![
        // Format 1: Simple method object
        serde_json::json!({"GetBlockDagInfoRequest": {}}),
        // Format 2: With request wrapper
        serde_json::json!({"method": "GetBlockDagInfoRequest", "params": null}),
        // Format 3: Array format
        serde_json::json!(["GetBlockDagInfoRequest", {}]),
        // Format 4: Request/Response pattern
        serde_json::json!({"request": "GetBlockDagInfo"}),
    ];

    for (i, request) in native_formats.iter().enumerate() {
        println!("\nTesting format {} ...", i + 1);
        println!("  Request: {}", serde_json::to_string(request).unwrap());

        if ws.send(Message::Text(request.to_string())).await.is_err() {
            println!("  ✗ Send failed");
            continue;
        }

        match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                println!("  ✓ Response: {}", &text[..text.len().min(100)]);
            }
            Ok(Some(Ok(Message::Binary(data)))) => {
                println!("  ✗ Binary response ({} bytes)", data.len());
            }
            Ok(Some(Err(e))) => {
                println!("  ✗ Error: {}", e);
            }
            Ok(None) => {
                println!("  ✗ Connection closed");
                break;
            }
            Err(_) => {
                println!("  ✗ Timeout (5s)");
            }
            _ => {
                println!("  ? Other message type");
            }
        }
    }

    let _ = ws.close(None).await;
}
