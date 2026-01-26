//! Kaspa Public Node Network (PNN) Resolver
//!
//! This module provides functionality to discover and connect to public
//! Kaspa wRPC endpoints through the PNN resolver service.

use crate::error::{KtcsError, Result};
use serde::Deserialize;

/// PNN resolver base URL
const PNN_URL: &str = "https://pnn.kaspa.stream";

/// Node information from the PNN
#[derive(Debug, Clone, Deserialize)]
pub struct PnnNode {
    /// Service ID
    pub sid: Option<String>,
    /// Protocol (borsh or json)
    pub protocol: Option<String>,
    /// Encoding type
    pub encoding: Option<String>,
    /// Network (mainnet, testnet-10, testnet-11)
    pub network: Option<String>,
    /// Node URL/hostname
    pub url: Option<String>,
    /// Connection status
    pub online: Option<bool>,
    /// Number of delegator nodes
    pub delegators: Option<u32>,
}

/// Resolver for discovering public Kaspa wRPC endpoints
#[derive(Debug, Clone)]
pub struct Resolver {
    /// Base URL for the PNN service
    pnn_url: String,
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            pnn_url: PNN_URL.to_string(),
        }
    }
}

impl Resolver {
    /// Create a new resolver with custom PNN URL
    pub fn new(pnn_url: &str) -> Self {
        Self {
            pnn_url: pnn_url.to_string(),
        }
    }

    /// Get the wRPC endpoint URL for a specific network
    ///
    /// # Arguments
    ///
    /// * `network` - The network name (e.g., "mainnet", "testnet-10", "testnet-11")
    /// * `encoding` - The encoding type ("borsh" or "json")
    ///
    /// # Returns
    ///
    /// The WebSocket URL for connecting to a public node.
    #[cfg(feature = "kaspa-client")]
    pub async fn get_url(&self, network: &str, encoding: &str) -> Result<String> {
        // First, try the direct PNN WebSocket endpoint format
        // The PNN uses paths like: /wrpc/{network}/{encoding}
        let ws_url = format!("wss://{}/wrpc/{}/{}",
            self.pnn_url.trim_start_matches("https://").trim_start_matches("http://"),
            network,
            encoding
        );

        // Try to connect to verify it works
        if self.test_endpoint(&ws_url).await {
            return Ok(ws_url);
        }

        // Alternative format: /{encoding}/{network}
        let alt_url = format!("wss://{}/{}/{}",
            self.pnn_url.trim_start_matches("https://").trim_start_matches("http://"),
            encoding,
            network
        );

        if self.test_endpoint(&alt_url).await {
            return Ok(alt_url);
        }

        // Try fetching node list and finding an available endpoint
        if let Ok(nodes) = self.fetch_nodes().await {
            for node in nodes {
                if let (Some(node_network), Some(node_encoding), Some(url), Some(online)) =
                    (&node.network, &node.encoding, &node.url, node.online)
                {
                    if node_network == network && node_encoding == encoding && online {
                        // Construct WebSocket URL from node info
                        let node_ws_url = if url.starts_with("ws://") || url.starts_with("wss://") {
                            url.clone()
                        } else {
                            format!("wss://{}", url)
                        };

                        if self.test_endpoint(&node_ws_url).await {
                            return Ok(node_ws_url);
                        }
                    }
                }
            }
        }

        // If all else fails, try common endpoint patterns
        // The Aspectron endpoint with /wrpc/{encoding}/{network} pattern works best
        let common_endpoints = vec![
            // Primary working pattern (confirmed working)
            format!("wss://kaspa.aspectron.com/wrpc/{}/{}", encoding, network),
            // Alternative patterns
            format!("wss://kaspa.aspectron.com/{}/wrpc/{}", network, encoding),
            format!("wss://kaspa.aspectron.com/wrpc/{}/{}", network, encoding),
            // Try PNN with various patterns
            format!("wss://pnn.kaspa.stream/wrpc/{}/{}", encoding, network),
            format!("wss://pnn.kaspa.stream/{}/{}", network, encoding),
        ];

        for endpoint in common_endpoints {
            tracing::debug!("Trying endpoint: {}", endpoint);
            if self.test_endpoint(&endpoint).await {
                tracing::info!("Found working endpoint: {}", endpoint);
                return Ok(endpoint);
            }
        }

        // Return the most likely endpoint even if we couldn't verify it
        // Let the actual connection attempt handle the error
        let fallback = format!("wss://kaspa.aspectron.com/wrpc/{}/{}", encoding, network);
        tracing::warn!("No verified endpoint found, using fallback: {}", fallback);
        Ok(fallback)
    }

    /// Fetch available nodes from the PNN
    #[cfg(feature = "kaspa-client")]
    async fn fetch_nodes(&self) -> Result<Vec<PnnNode>> {
        let json_url = format!("{}/json", self.pnn_url);

        // Use reqwest if available, otherwise skip node fetching
        #[cfg(feature = "kaspa-client")]
        {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|e| KtcsError::ConnectionError(format!("Failed to create HTTP client: {}", e)))?;

            let response = client
                .get(&json_url)
                .send()
                .await
                .map_err(|e| KtcsError::ConnectionError(format!("Failed to fetch PNN data: {}", e)))?;

            if !response.status().is_success() {
                return Err(KtcsError::ConnectionError(format!(
                    "PNN returned status: {}",
                    response.status()
                )));
            }

            let nodes: Vec<PnnNode> = response
                .json()
                .await
                .map_err(|e| KtcsError::ConnectionError(format!("Failed to parse PNN response: {}", e)))?;

            Ok(nodes)
        }
    }

    /// Test if an endpoint is reachable
    #[cfg(feature = "kaspa-client")]
    async fn test_endpoint(&self, url: &str) -> bool {
        use tokio_tungstenite::connect_async;
        use std::time::Duration;

        let timeout = Duration::from_secs(5);

        match tokio::time::timeout(timeout, connect_async(url)).await {
            Ok(Ok(_)) => true,
            _ => false,
        }
    }

    /// Get the default wRPC URL for a network using JSON encoding
    ///
    /// Note: JSON encoding is preferred for compatibility with JSON-RPC style requests.
    /// Borsh encoding requires binary serialization which is more complex.
    #[cfg(feature = "kaspa-client")]
    pub async fn get_node_url(&self, network: &str) -> Result<String> {
        self.get_url(network, "json").await
    }

    /// Stub for when kaspa-client feature is disabled
    #[cfg(not(feature = "kaspa-client"))]
    pub async fn get_url(&self, network: &str, encoding: &str) -> Result<String> {
        Err(KtcsError::ConnectionError(format!(
            "Resolver requires kaspa-client feature. Network: {}, Encoding: {}",
            network, encoding
        )))
    }

    /// Stub for when kaspa-client feature is disabled
    #[cfg(not(feature = "kaspa-client"))]
    pub async fn get_node_url(&self, network: &str) -> Result<String> {
        Err(KtcsError::ConnectionError(format!(
            "Resolver requires kaspa-client feature. Network: {}",
            network
        )))
    }
}

/// Get a public wRPC URL for the specified network
///
/// This is a convenience function that uses the default resolver.
///
/// # Arguments
///
/// * `network` - The network name (e.g., "mainnet", "testnet-10")
///
/// # Returns
///
/// A WebSocket URL for connecting to a public Kaspa node.
#[cfg(feature = "kaspa-client")]
pub async fn resolve_url(network: &str) -> Result<String> {
    let resolver = Resolver::default();
    resolver.get_node_url(network).await
}

#[cfg(not(feature = "kaspa-client"))]
pub async fn resolve_url(network: &str) -> Result<String> {
    Err(KtcsError::ConnectionError(format!(
        "Resolver requires kaspa-client feature. Network: {}",
        network
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_creation() {
        let resolver = Resolver::default();
        assert_eq!(resolver.pnn_url, PNN_URL);

        let custom = Resolver::new("https://custom.resolver.com");
        assert_eq!(custom.pnn_url, "https://custom.resolver.com");
    }
}
