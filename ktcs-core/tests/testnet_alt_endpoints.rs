//! Test alternative testnet endpoints from official resolver config
//!
//! Run with: cargo test --package ktcs-core --test testnet_alt_endpoints --features kaspa-client -- --nocapture

use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};

/// Test all known resolver domains for testnet-10
#[tokio::test]
async fn test_all_resolver_domains() {
    println!("\n=== Testing All Resolver Domains for Testnet-10 ===\n");

    // From Resolvers.toml: *.kaspa.stream, *.kaspa.red, *.kaspa.green, *.kaspa.blue
    let domains = vec![
        "kaspa.stream",
        "kaspa.red",
        "kaspa.green",
        "kaspa.blue",
        "aspectron.com",
    ];

    let patterns = vec![
        "/wrpc/json/testnet-10",
        "/wrpc/borsh/testnet-10",
        "/json/testnet-10",
        "/borsh/testnet-10",
    ];

    let mut working = vec![];

    for domain in &domains {
        for pattern in &patterns {
            let url = format!("wss://{}{}", domain, pattern);
            print!("Testing {} ... ", &url[6..]);

            match tokio::time::timeout(Duration::from_secs(5), connect_async(&url)).await {
                Ok(Ok((ws, resp))) => {
                    println!("✓ Connected ({})", resp.status());
                    working.push(url.clone());
                    drop(ws);
                }
                Ok(Err(e)) => {
                    let err = e.to_string();
                    if err.contains("502") {
                        println!("✗ 502");
                    } else if err.contains("404") {
                        println!("✗ 404");
                    } else {
                        println!("✗ {}", &err[..err.len().min(30)]);
                    }
                }
                Err(_) => {
                    println!("✗ Timeout");
                }
            }
        }
    }

    println!("\n=== Working Endpoints ===");
    if working.is_empty() {
        println!("None found via simple URL patterns");
    } else {
        for url in &working {
            println!("  ✓ {}", url);
        }
    }
}

/// Test with UID-based routing through PNN
#[tokio::test]
async fn test_uid_routing() {
    println!("\n=== Testing UID-based Routing ===\n");

    // Online testnet-10 UIDs from PNN (status: online)
    let uids = vec![
        "a027070d8362459b", // online
        "86ea7de5c88424dd", // online
    ];

    let domains = vec![
        "pnn.kaspa.stream",
        "kaspa.stream",
    ];

    for domain in &domains {
        for uid in &uids {
            // Try different UID routing patterns
            let urls = vec![
                format!("wss://{}/{}", domain, uid),
                format!("wss://{}/wrpc/{}", domain, uid),
                format!("wss://{}/node/{}", domain, uid),
                format!("wss://{}.{}/", uid, domain),
            ];

            for url in urls {
                print!("Testing {} ... ", &url[6..url.len().min(50)]);

                match tokio::time::timeout(Duration::from_secs(3), connect_async(&url)).await {
                    Ok(Ok((ws, _))) => {
                        println!("✓ Connected!");
                        drop(ws);
                    }
                    Ok(Err(e)) => {
                        let err = e.to_string();
                        if err.contains("dns") || err.contains("resolve") {
                            println!("✗ DNS");
                        } else if err.contains("502") || err.contains("404") {
                            println!("✗ {}", if err.contains("502") { "502" } else { "404" });
                        } else {
                            println!("✗");
                        }
                    }
                    Err(_) => println!("✗ Timeout"),
                }
            }
        }
    }
}

/// Test explorer API for testnet-10
#[tokio::test]
async fn test_explorer_api() {
    println!("\n=== Testing Testnet-10 Explorer API ===\n");

    // The explorer must connect to a node, so let's see what it uses
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // Try explorer API
    print!("Testing https://explorer-tn10.kaspa.org/api/info ... ");
    match client.get("https://explorer-tn10.kaspa.org/api/info").send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                println!("✓ Got response");
                println!("  {}", &text[..text.len().min(200)]);
            } else {
                println!("✗ Status: {}", resp.status());
            }
        }
        Err(e) => println!("✗ {}", e),
    }

    // Try to find WebSocket endpoint in explorer
    print!("Testing https://api-tn10.kaspa.org/ ... ");
    match client.get("https://api-tn10.kaspa.org/").send().await {
        Ok(resp) => {
            println!("Status: {} ({})", resp.status(), resp.content_length().unwrap_or(0));
        }
        Err(e) => {
            let err = e.to_string();
            if err.contains("dns") {
                println!("✗ DNS error (doesn't exist)");
            } else {
                println!("✗ {}", &err[..err.len().min(40)]);
            }
        }
    }
}

/// Try to connect and send RPC to working endpoint
#[tokio::test]
async fn test_working_rpc() {
    println!("\n=== Testing RPC on Any Working Endpoint ===\n");

    // Try mainnet first to verify our RPC works, then try testnet variants
    let endpoints = vec![
        ("wss://kaspa.aspectron.com/wrpc/json/mainnet", "mainnet"),
        ("wss://kaspa.stream/wrpc/json/mainnet", "mainnet-alt"),
        ("wss://kaspa.red/wrpc/json/testnet-10", "tn10-red"),
        ("wss://kaspa.green/wrpc/json/testnet-10", "tn10-green"),
        ("wss://kaspa.blue/wrpc/json/testnet-10", "tn10-blue"),
    ];

    for (url, name) in endpoints {
        println!("\n--- Testing {} ({}) ---", name, url);

        let ws_result = tokio::time::timeout(
            Duration::from_secs(10),
            connect_async(url)
        ).await;

        let mut ws = match ws_result {
            Ok(Ok((ws, _))) => {
                println!("  ✓ WebSocket connected");
                ws
            }
            Ok(Err(e)) => {
                println!("  ✗ Connection failed: {}", &e.to_string()[..e.to_string().len().min(50)]);
                continue;
            }
            Err(_) => {
                println!("  ✗ Connection timeout");
                continue;
            }
        };

        // Send RPC request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "getBlockDagInfo",
            "params": {},
            "id": 1
        });

        if ws.send(Message::Text(request.to_string())).await.is_err() {
            println!("  ✗ Failed to send request");
            continue;
        }

        // Wait for response
        match tokio::time::timeout(Duration::from_secs(10), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(params) = json.get("params") {
                        if let Some(network) = params.get("network") {
                            println!("  ✓ RPC works! Network: {}", network);
                            if let Some(daa) = params.get("virtualDaaScore") {
                                println!("  ✓ DAA Score: {}", daa);
                            }
                        }
                    } else if let Some(result) = json.get("result") {
                        println!("  ✓ Got result: {}", &result.to_string()[..result.to_string().len().min(80)]);
                    } else {
                        println!("  ? Response: {}", &text[..text.len().min(100)]);
                    }
                }
            }
            Ok(Some(Ok(Message::Binary(data)))) => {
                println!("  ✗ Binary response ({} bytes) - wrong encoding?", data.len());
            }
            Ok(Some(Err(e))) => {
                println!("  ✗ WebSocket error: {}", e);
            }
            Ok(None) => {
                println!("  ✗ Connection closed");
            }
            Err(_) => {
                println!("  ✗ RPC timeout");
            }
            _ => {
                println!("  ? Unknown response");
            }
        }

        let _ = ws.close(None).await;
    }
}
