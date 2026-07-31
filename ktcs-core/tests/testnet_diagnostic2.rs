//! Additional diagnostic tests - trying SID-based routing
//!
//! Run with: cargo test --package ktcs-core --test testnet_diagnostic2 --features kaspa-client -- --nocapture

use std::time::Duration;
use tokio_tungstenite::connect_async;

/// Try SID-based URL formats
#[tokio::test]
#[ignore = "requires live network"]
async fn diagnose_sid_based_endpoints() {
    println!("\n");
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║       SID-BASED ENDPOINT DIAGNOSTIC                        ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // From PNN: Online testnet-10 nodes
    // SID: 35eb6927b1cbf034 (peers: 30, clients: 922)
    // SID: 45cd71bf52cf29be (peers: 23, clients: 683)

    let sids = vec!["35eb6927b1cbf034", "45cd71bf52cf29be"];

    let uids = vec![
        "86ea7de5c88424dd", // online node
        "a027070d8362459b", // online node
    ];

    let mut endpoints = vec![];

    // Try various URL patterns with SID/UID
    for sid in &sids {
        endpoints.push((
            format!("wss://pnn.kaspa.stream/wrpc/{}", sid),
            format!("PNN wrpc/{}", &sid[..8]),
        ));
        endpoints.push((
            format!("wss://pnn.kaspa.stream/{}", sid),
            format!("PNN /{}", &sid[..8]),
        ));
        endpoints.push((
            format!("wss://pnn.kaspa.stream/node/{}", sid),
            format!("PNN node/{}", &sid[..8]),
        ));
        endpoints.push((
            format!("wss://{}.pnn.kaspa.stream/", sid),
            format!("SID.pnn.kaspa.stream"),
        ));
    }

    for uid in &uids {
        endpoints.push((
            format!("wss://pnn.kaspa.stream/wrpc/{}", uid),
            format!("PNN wrpc/{}", &uid[..8]),
        ));
        endpoints.push((
            format!("wss://pnn.kaspa.stream/{}", uid),
            format!("PNN /{}", &uid[..8]),
        ));
    }

    // Try alternative hosts
    endpoints.push((
        "wss://wrpc.kaspa.stream/borsh/testnet-10".to_string(),
        "wrpc.kaspa.stream".to_string(),
    ));
    endpoints.push((
        "wss://tn10.kaspa.stream/".to_string(),
        "tn10.kaspa.stream".to_string(),
    ));
    endpoints.push((
        "wss://testnet.kaspa.stream/".to_string(),
        "testnet.kaspa.stream".to_string(),
    ));

    // Try Kaspa Labs endpoints
    endpoints.push((
        "wss://kaspa-labs.io/wrpc/testnet-10".to_string(),
        "kaspa-labs wrpc".to_string(),
    ));
    endpoints.push((
        "wss://api.kaspa.org/wrpc/testnet-10".to_string(),
        "api.kaspa.org wrpc".to_string(),
    ));

    let mut working = vec![];

    for (url, name) in &endpoints {
        print!("Testing {:<35} ... ", name);

        let result =
            tokio::time::timeout(Duration::from_secs(8), connect_async(url.as_str())).await;

        match result {
            Ok(Ok((ws, response))) => {
                println!("✓ OK ({})", response.status());
                working.push((url.clone(), name.clone()));
                drop(ws);
            }
            Ok(Err(e)) => {
                let err = e.to_string();
                if err.contains("502") {
                    println!("✗ 502");
                } else if err.contains("404") {
                    println!("✗ 404");
                } else if err.contains("500") {
                    println!("✗ 500");
                } else if err.contains("Io(") || err.contains("dns") || err.contains("resolve") {
                    println!("✗ DNS/IO");
                } else {
                    println!("✗ {}", &err[..err.len().min(40)]);
                }
            }
            Err(_) => {
                println!("✗ Timeout");
            }
        }
    }

    println!();
    if working.is_empty() {
        println!("No SID-based endpoints found working.");
    } else {
        println!("Working endpoints:");
        for (url, name) in &working {
            println!("  ✓ {} → {}", name, url);
        }
    }
}

/// Try the official Kaspa resolver SDK approach
#[tokio::test]
#[ignore = "requires live network"]
async fn test_kaspa_resolver_format() {
    println!("\n=== Testing Official Kaspa Resolver Format ===\n");

    // The official Kaspa SDK uses a Resolver that:
    // 1. Fetches node list from PNN
    // 2. Picks an online node
    // 3. Constructs URL using the node's routing info

    // Based on the Kaspa Rust SDK, the URL format appears to be:
    // wss://pnn.kaspa.stream/wrpc/v1/{network}/{encoding}

    let urls = vec![
        "wss://pnn.kaspa.stream/wrpc/v1/testnet-10/borsh",
        "wss://pnn.kaspa.stream/wrpc/v1/kaspa-testnet-10/borsh",
        "wss://pnn.kaspa.stream/v1/testnet-10/borsh",
        "wss://pnn.kaspa.stream/v1/wrpc/testnet-10/borsh",
        // Try with network id
        "wss://pnn.kaspa.stream/wrpc/10/borsh",
        // JSON variant
        "wss://pnn.kaspa.stream/wrpc/v1/testnet-10/json",
    ];

    for url in urls {
        print!("Testing {} ... ", &url[25..]);

        match tokio::time::timeout(Duration::from_secs(8), connect_async(url)).await {
            Ok(Ok((ws, resp))) => {
                println!("✓ Connected ({})", resp.status());
                drop(ws);
            }
            Ok(Err(e)) => {
                let err = e.to_string();
                println!("✗ {}", &err[..err.len().min(50)]);
            }
            Err(_) => {
                println!("✗ Timeout");
            }
        }
    }
}

/// Try direct Kaspa node ports (local or known public)
#[tokio::test]
#[ignore = "requires live network"]
async fn test_direct_ports() {
    println!("\n=== Testing Direct Port Connections ===\n");

    // Kaspa testnet typically uses:
    // - gRPC: 16210
    // - wRPC: 17210 (WebSocket RPC)

    let endpoints = vec![
        // Local node (if running)
        ("ws://localhost:17210", "localhost:17210 (wRPC)"),
        ("ws://localhost:16210", "localhost:16210 (gRPC)"),
        ("ws://127.0.0.1:17210", "127.0.0.1:17210"),
        // Standard testnet ports on known hosts
        ("wss://testnet.kaspa.org:17210", "testnet.kaspa.org:17210"),
    ];

    for (url, name) in endpoints {
        print!("Testing {} ... ", name);

        match tokio::time::timeout(Duration::from_secs(5), connect_async(url)).await {
            Ok(Ok((ws, resp))) => {
                println!("✓ Connected ({})", resp.status());
                drop(ws);
            }
            Ok(Err(e)) => {
                let err = e.to_string();
                if err.contains("refused") {
                    println!("✗ Refused (no local node?)");
                } else if err.contains("dns") || err.contains("resolve") {
                    println!("✗ DNS error");
                } else {
                    println!("✗ {}", &err[..err.len().min(40)]);
                }
            }
            Err(_) => {
                println!("✗ Timeout");
            }
        }
    }
}

/// Verify basic HTTPS connectivity to PNN
#[tokio::test]
#[ignore = "requires live network"]
async fn test_pnn_https() {
    println!("\n=== Testing PNN HTTPS Connectivity ===\n");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // Test basic HTTPS
    print!("Fetching https://pnn.kaspa.stream/json ... ");
    match client.get("https://pnn.kaspa.stream/json").send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                println!("✓ OK ({} bytes)", text.len());
            } else {
                println!("✗ Status: {}", resp.status());
            }
        }
        Err(e) => {
            println!("✗ {}", e);
        }
    }

    // Test if there's an API endpoint
    print!("Fetching https://pnn.kaspa.stream/        ... ");
    match client.get("https://pnn.kaspa.stream/").send().await {
        Ok(resp) => {
            println!(
                "Status: {} ({})",
                resp.status(),
                resp.content_length().unwrap_or(0)
            );
        }
        Err(e) => {
            println!("✗ {}", e);
        }
    }
}
