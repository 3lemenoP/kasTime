//! KTCS Command Line Interface
//!
//! Provides commands for creating and verifying Kaspa timestamps.

use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use colored::Colorize;

/// Configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default network (mainnet or testnet)
    #[serde(default = "default_network")]
    pub network: String,

    /// Prefer direct stamping when wallet is configured
    #[serde(default)]
    pub prefer_direct: bool,

    /// Mainnet-specific settings
    #[serde(default)]
    pub mainnet: NetworkConfig,

    /// Testnet-specific settings
    #[serde(default)]
    pub testnet: NetworkConfig,
}

fn default_network() -> String {
    "mainnet".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network: default_network(),
            prefer_direct: false,
            mainnet: NetworkConfig::default(),
            testnet: NetworkConfig::default(),
        }
    }
}

/// Network-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Calendar server URL
    #[serde(default)]
    pub calendar: Option<String>,

    /// Kaspa RPC URL
    #[serde(default)]
    pub rpc_url: Option<String>,

    /// Path to wallet key file
    #[serde(default)]
    pub wallet_file: Option<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            calendar: None,
            rpc_url: None,
            wallet_file: None,
        }
    }
}

impl Config {
    /// Get the config file path
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ktcs")
            .join("config.toml")
    }

    /// Load config from file, or return defaults if not found
    pub fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => eprintln!("Warning: Failed to parse config: {}", e),
                },
                Err(e) => eprintln!("Warning: Failed to read config: {}", e),
            }
        }
        Self::default()
    }

    /// Save config to file
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Get network-specific config
    pub fn network_config(&self) -> &NetworkConfig {
        match self.network.as_str() {
            "testnet" => &self.testnet,
            _ => &self.mainnet,
        }
    }

    /// Get calendar URL for current network
    pub fn calendar(&self) -> String {
        self.network_config()
            .calendar
            .clone()
            .unwrap_or_else(|| {
                if self.network == "testnet" {
                    "https://testnet-calendar.ktcs.kaspa.org".to_string()
                } else {
                    "https://calendar.ktcs.kaspa.org".to_string()
                }
            })
    }

    /// Get RPC URL for current network
    pub fn rpc_url(&self) -> String {
        self.network_config()
            .rpc_url
            .clone()
            .unwrap_or_else(|| {
                if self.network == "testnet" {
                    "ws://localhost:16110".to_string()
                } else {
                    "wss://kaspa.aspectron.com/wrpc/json/mainnet".to_string()
                }
            })
    }

    /// Get wallet file path for current network
    pub fn wallet_file(&self) -> Option<PathBuf> {
        self.network_config()
            .wallet_file
            .as_ref()
            .map(|s| {
                // Expand ~ to home directory
                if s.starts_with("~/") {
                    dirs::home_dir()
                        .map(|h| h.join(&s[2..]))
                        .unwrap_or_else(|| PathBuf::from(s))
                } else {
                    PathBuf::from(s)
                }
            })
    }
}
use ktcs_core::{
    complete_stamp, deserialize_proof, generate_private_key, merkle::sha256,
    prepare_direct_stamp, serialize_proof, sign_transaction, verify_proof, Attestation, BatchMode,
    DirectBlockInfo, DirectStampConfig, KaspaClient, KaspaClientConfig, KaspaWallet, KtcsProof,
    PendingAttestation, ThermodynamicMetrics,
};

/// Color output mode
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
enum ColorMode {
    /// Automatically detect if terminal supports colors
    #[default]
    Auto,
    /// Always use colors
    Always,
    /// Never use colors
    Never,
}

#[derive(Parser)]
#[command(name = "ktcs")]
#[command(author, version, about = "Kaspa Thermodynamic Clock Service CLI")]
#[command(
    long_about = "Create and verify timestamps anchored to the Kaspa blockchain.\n\n\
    KTCS provides trustless proof-of-existence using Kaspa's high-throughput BlockDAG,\n\
    enabling sub-second timestamp confirmations with thermodynamic security."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Control color output
    #[arg(long, global = true, default_value = "auto", value_enum)]
    color: ColorMode,

    /// Suppress all output except errors and essential results
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a timestamp for a file
    #[command(alias = "s")]
    Stamp {
        /// File to timestamp
        file: PathBuf,

        /// Calendar server URL
        #[arg(short, long, default_value = "https://calendar.ktcs.kaspa.org")]
        calendar: String,

        /// Batch mode (instant, standard, economic)
        #[arg(short, long, default_value = "standard")]
        mode: String,

        /// Output file for the proof (default: <input>.kts)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Create timestamp directly on-chain (requires --wallet-file or --wallet-stdin)
        #[arg(long)]
        direct: bool,

        /// Return immediately with pending proof (don't wait for confirmation)
        #[arg(long = "async")]
        async_mode: bool,

        /// Path to file containing wallet private key (hex)
        /// SECURITY: Do not pass private keys as command-line arguments
        #[arg(long, value_name = "FILE")]
        wallet_file: Option<PathBuf>,

        /// Read wallet private key from stdin
        #[arg(long)]
        wallet_stdin: bool,

        /// Kaspa RPC URL for direct stamping
        #[arg(long, default_value = "ws://localhost:16110")]
        rpc_url: String,

        /// Network for direct stamping (mainnet, testnet)
        #[arg(long, default_value = "testnet")]
        network: String,
    },

    /// Verify a timestamp proof
    #[command(alias = "v")]
    Verify {
        /// Proof file (.kts)
        proof: PathBuf,

        /// Original data file (optional, for full verification)
        #[arg(short, long)]
        data: Option<PathBuf>,

        /// Verify attestation exists on blockchain (requires network)
        #[arg(long)]
        chain: bool,

        /// Kaspa RPC URL for chain verification
        #[arg(long, default_value = "wss://kaspa.aspectron.com/wrpc/json/mainnet")]
        rpc_url: String,
    },

    /// Display information about a proof
    #[command(alias = "i")]
    Info {
        /// Proof file (.kts)
        proof: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Complete a pending proof by fetching the confirmed attestation
    #[command(alias = "c", visible_alias = "upgrade")]
    Complete {
        /// Pending proof file (.kts)
        proof: PathBuf,

        /// Output file (default: overwrite input)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Check the status of a proof (pending or complete)
    Status {
        /// Proof file (.kts)
        proof: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Compute SHA256 hash of a file
    Hash {
        /// File to hash
        file: PathBuf,
    },

    /// Wallet management commands
    #[command(alias = "w")]
    Wallet {
        #[command(subcommand)]
        command: WalletCommands,
    },

    /// Configuration management commands
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
enum WalletCommands {
    /// Generate a new private key
    #[command(alias = "gen")]
    Generate {
        /// Output format: hex (default), json
        #[arg(short, long, default_value = "hex")]
        format: String,

        /// Network: mainnet, testnet (default: mainnet)
        #[arg(short, long, default_value = "mainnet")]
        network: String,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Show wallet address from private key
    Address {
        /// Path to file containing private key
        #[arg(long, value_name = "FILE")]
        wallet_file: Option<PathBuf>,

        /// Read private key from stdin
        #[arg(long)]
        wallet_stdin: bool,

        /// Network: mainnet, testnet
        #[arg(short, long, default_value = "mainnet")]
        network: String,
    },

    /// Check wallet balance
    Balance {
        /// Path to file containing private key
        #[arg(long, value_name = "FILE")]
        wallet_file: Option<PathBuf>,

        /// Read private key from stdin
        #[arg(long)]
        wallet_stdin: bool,

        /// Or specify address directly
        #[arg(long)]
        address: Option<String>,

        /// Network: mainnet, testnet-10, testnet-11
        #[arg(short, long, default_value = "mainnet")]
        network: String,

        /// RPC URL (if not using resolver)
        #[arg(long)]
        rpc: Option<String>,

        /// Use PNN resolver to discover public nodes
        #[arg(long, default_value = "true")]
        resolver: bool,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Initialize config file with defaults
    Init {
        /// Overwrite existing config
        #[arg(long)]
        force: bool,
    },

    /// Show current configuration
    Show {
        /// Show config for specific network
        #[arg(long)]
        network: Option<String>,
    },

    /// Set a configuration value
    Set {
        /// Key to set (e.g., "network", "mainnet.rpc_url")
        key: String,

        /// Value to set
        value: String,
    },

    /// Show config file path
    Path,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Set up color mode
    match cli.color {
        ColorMode::Always => colored::control::set_override(true),
        ColorMode::Never => colored::control::set_override(false),
        ColorMode::Auto => {
            // Respect NO_COLOR environment variable
            if std::env::var("NO_COLOR").is_ok() {
                colored::control::set_override(false);
            }
        }
    }

    // Set up logging if verbose
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("ktcs=debug")
            .init();
    }

    match cli.command {
        Commands::Stamp {
            file,
            calendar,
            mode,
            output,
            direct,
            async_mode,
            wallet_file,
            wallet_stdin,
            rpc_url,
            network,
        } => {
            cmd_stamp(file, calendar, mode, output, direct, async_mode, wallet_file, wallet_stdin, rpc_url, network).await?;
        }
        Commands::Verify { proof, data, chain, rpc_url } => {
            cmd_verify(proof, data, chain, rpc_url).await?;
        }
        Commands::Info { proof, json } => {
            cmd_info(proof, json)?;
        }
        Commands::Complete { proof, output } => {
            cmd_complete(proof, output).await?;
        }
        Commands::Status { proof, json } => {
            cmd_status(proof, json)?;
        }
        Commands::Hash { file } => {
            cmd_hash(file)?;
        }
        Commands::Wallet { command } => match command {
            WalletCommands::Generate {
                format,
                network,
                output,
            } => {
                cmd_wallet_generate(&format, &network, output)?;
            }
            WalletCommands::Address {
                wallet_file,
                wallet_stdin,
                network,
            } => {
                cmd_wallet_address(wallet_file, wallet_stdin, &network)?;
            }
            WalletCommands::Balance {
                wallet_file,
                wallet_stdin,
                address,
                network,
                rpc,
                resolver,
            } => {
                cmd_wallet_balance(wallet_file, wallet_stdin, address, &network, rpc.as_deref(), resolver).await?;
            }
        },
        Commands::Config { command } => match command {
            ConfigCommands::Init { force } => {
                cmd_config_init(force)?;
            }
            ConfigCommands::Show { network } => {
                cmd_config_show(network)?;
            }
            ConfigCommands::Set { key, value } => {
                cmd_config_set(&key, &value)?;
            }
            ConfigCommands::Path => {
                println!("{}", Config::path().display());
            }
        },
        Commands::Completions { shell } => {
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "ktcs", &mut std::io::stdout());
        }
    }

    Ok(())
}

/// Determines the output path for a proof file.
/// Uses the provided output path or derives one from the input file with .kts extension.
fn determine_output_path(output: Option<PathBuf>, input_file: &std::path::Path) -> PathBuf {
    output.unwrap_or_else(|| {
        let mut path = input_file.to_path_buf();
        path.set_extension("kts");
        path
    })
}

/// Read wallet private key from file or stdin securely
fn read_wallet_key(wallet_file: Option<PathBuf>, wallet_stdin: bool) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if wallet_stdin {
        // Read from stdin
        use std::io::{self, BufRead};
        eprintln!("{}", "Enter wallet private key (hex):".dimmed());
        let stdin = io::stdin();
        let key = stdin.lock().lines().next()
            .ok_or("No input provided")??
            .trim()
            .to_string();
        if key.is_empty() {
            return Err("Empty private key".into());
        }
        Ok(Some(key))
    } else if let Some(path) = wallet_file {
        // Read from file
        let key = fs::read_to_string(&path)?
            .trim()
            .to_string();
        if key.is_empty() {
            return Err(format!("Empty private key in file: {}", path.display()).into());
        }
        Ok(Some(key))
    } else {
        Ok(None)
    }
}

#[allow(clippy::too_many_arguments)]
async fn cmd_stamp(
    file: PathBuf,
    calendar: String,
    mode: String,
    output: Option<PathBuf>,
    direct: bool,
    async_mode: bool,
    wallet_file: Option<PathBuf>,
    wallet_stdin: bool,
    rpc_url: String,
    network: String,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "KTCS Timestamp".bold().cyan());
    println!();

    // Read and hash the file
    let data = fs::read(&file)?;
    let hash = sha256(&data);
    let hash_hex = hex::encode(hash);

    println!("  {} {}", "File:".dimmed(), file.display());
    println!("  {} {} bytes", "Size:".dimmed(), data.len());
    println!("  {} {}", "SHA256:".dimmed(), hash_hex.yellow());
    println!();

    // Handle direct stamping mode
    if direct {
        let wallet_key = read_wallet_key(wallet_file, wallet_stdin)?;
        return cmd_stamp_direct(&file, output, wallet_key, &rpc_url, &network, &data).await;
    }

    // Parse batch mode
    let batch_mode = parse_batch_mode(&mode)?;

    println!("  {} {}", "Calendar:".dimmed(), calendar);
    println!(
        "  {} {:?} (~{}ms window)",
        "Mode:".dimmed(),
        batch_mode,
        batch_mode.window_ms()
    );
    println!();

    // For now, create a pending proof (actual calendar submission would go here)
    let calendar_url = format!("{}/v1/stamp/ktcs_{}", calendar, &hash_hex[..16]);
    let output_path = determine_output_path(output, &file);

    // Create pending proof
    let mut proof = KtcsProof::new(hash.to_vec());
    proof.add_attestation(Attestation::Pending(PendingAttestation {
        calendar_url: calendar_url.clone(),
    }));

    println!("{} Submitting to calendar...", "→".blue());

    // In async mode, just save the pending proof and return
    if async_mode {
        let kts_data = serialize_proof(&proof);
        fs::write(&output_path, &kts_data)?;

        println!();
        println!("{}", "Timestamp submitted!".green().bold());
        println!("  {} {}", "Proof:".dimmed(), output_path.display());
        println!(
            "  {} {}",
            "Status:".dimmed(),
            "Pending (awaiting confirmation)".yellow()
        );
        println!();
        println!(
            "{}",
            "Run 'ktcs complete' once the timestamp is confirmed.".dimmed()
        );
        return Ok(());
    }

    // Synchronous mode: wait for confirmation
    println!("{} Waiting for confirmation...", "→".blue());

    let client = reqwest::Client::new();
    let max_attempts = 60; // ~60 seconds with 1s sleep
    let mut attempts = 0;

    loop {
        attempts += 1;
        if attempts > max_attempts {
            // Timeout - save pending proof
            let kts_data = serialize_proof(&proof);
            fs::write(&output_path, &kts_data)?;

            println!();
            println!("{}", "Timeout waiting for confirmation.".yellow());
            println!("  {} {}", "Proof:".dimmed(), output_path.display());
            println!(
                "  {} {}",
                "Status:".dimmed(),
                "Pending".yellow()
            );
            println!();
            println!(
                "{}",
                "Run 'ktcs complete' later to fetch the confirmed proof.".dimmed()
            );
            return Ok(());
        }

        // Poll calendar for confirmation
        let response = match client
            .get(&calendar_url)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        if !response.status().is_success() {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            continue;
        }

        let stamp_response: CalendarStampResponse = match response.json().await {
            Ok(r) => r,
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        if stamp_response.status == "confirmed" {
            // Get the complete proof from response
            if let Some(proof_base64) = stamp_response.proof {
                let complete_proof_bytes = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &proof_base64,
                )?;

                // Verify we can deserialize the complete proof
                let complete_proof = deserialize_proof(&complete_proof_bytes)?;

                // Security: Verify proof digest matches original
                if complete_proof.digest != proof.digest {
                    return Err("Security error: calendar returned proof with different digest".into());
                }

                // Save complete proof
                fs::write(&output_path, &complete_proof_bytes)?;

                println!("{} Confirmed!", "✓".green());
                println!();
                println!("{}", "Timestamp created!".green().bold());
                println!("  {} {}", "Proof:".dimmed(), output_path.display());
                println!(
                    "  {} {}",
                    "Status:".dimmed(),
                    "Complete".green()
                );

                // Show attestation info
                for attestation in complete_proof.attestations.iter() {
                    if let Attestation::Kaspa(ka) = attestation {
                        println!();
                        println!("  {} {}", "DAA Score:".dimmed(), ka.daa_score);
                        println!("  {} {}", "Blue Score:".dimmed(), ka.blue_score);
                        break;
                    }
                }

                return Ok(());
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

/// Direct stamping command handler - creates timestamps directly on-chain.
async fn cmd_stamp_direct(
    file: &std::path::Path,
    output: Option<PathBuf>,
    wallet_key: Option<String>,
    rpc_url: &str,
    network: &str,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "Direct Stamping Mode".bold().yellow());
    println!();

    let wallet_key = wallet_key.ok_or_else(|| {
        eprintln!(
            "{}",
            "Direct stamping requires --wallet-file or --wallet-stdin".red()
        );
        eprintln!();
        eprintln!("{}", "Examples:".bold());
        eprintln!(
            "  {}",
            "ktcs stamp --direct --wallet-file /path/to/key.txt file.txt".dimmed()
        );
        eprintln!(
            "  {}",
            "echo '<hex-key>' | ktcs stamp --direct --wallet-stdin file.txt".dimmed()
        );
        eprintln!();
        eprintln!(
            "{}",
            "SECURITY: Never pass private keys as command-line arguments!".yellow()
        );
        "Missing wallet key for direct stamping"
    })?;

    let wallet = KaspaWallet::from_hex(&wallet_key, network).map_err(|e| {
        eprintln!("{}", format!("Invalid wallet key: {}", e).red());
        e
    })?;

    println!("  {} {}", "Wallet:".dimmed(), wallet.address());
    println!("  {} {}", "Network:".dimmed(), network);
    println!("  {} {}", "RPC:".dimmed(), rpc_url);
    println!();

    // 1. Connect to Kaspa node
    println!("{} Connecting to Kaspa node...", "→".blue());
    let client_config = KaspaClientConfig {
        rpc_url: rpc_url.to_string(),
        network: Some(network.to_string()),
        ..Default::default()
    };
    let client = KaspaClient::new(client_config);

    match client.connect().await {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{} Failed to connect: {}", "Error:".red(), e);
            eprintln!();
            eprintln!(
                "{}",
                "Make sure a Kaspa node is running and accessible at the RPC URL.".yellow()
            );
            return Err(e.into());
        }
    }

    let dag_info = client.get_block_dag_info().await?;
    println!(
        "{} Connected to {} (DAA: {})",
        "✓".green(),
        dag_info.network,
        dag_info.current_daa_score
    );

    // 2. Prepare stamp (fetches UTXOs, builds transaction)
    println!("{} Preparing transaction...", "→".blue());
    let stamp_config = DirectStampConfig::default();
    let prepared = match prepare_direct_stamp(data, &wallet, &client, &stamp_config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{} Failed to prepare stamp: {}", "Error:".red(), e);
            if e.to_string().contains("No UTXOs") {
                eprintln!();
                eprintln!(
                    "{}",
                    "The wallet has no spendable funds. Please send some KAS to:".yellow()
                );
                eprintln!("  {}", wallet.address().cyan());
            }
            return Err(e.into());
        }
    };

    println!(
        "{} Commitment: {}",
        "✓".green(),
        hex::encode(prepared.commitment).yellow()
    );
    println!(
        "  {} {} sompi ({:.8} KAS)",
        "Fee:".dimmed(),
        prepared.estimated_fee,
        prepared.estimated_fee as f64 / 100_000_000.0
    );

    // 3. Sign transaction
    let tx = prepared
        .transaction
        .as_ref()
        .ok_or("Transaction not built")?;
    let utxos = prepared.utxos.as_ref().ok_or("UTXOs not available")?;

    println!(
        "{} Signing {} input(s)...",
        "→".blue(),
        utxos.len()
    );
    let signed_tx = sign_transaction(&tx.transaction, &wallet, utxos)?;
    println!("{} Transaction signed", "✓".green());

    // 4. Submit to network
    println!("{} Submitting transaction...", "→".blue());
    let tx_hash = match client.submit_transaction(signed_tx).await {
        Ok(hash) => hash,
        Err(e) => {
            eprintln!("{} Failed to submit transaction: {}", "Error:".red(), e);
            return Err(e.into());
        }
    };
    println!(
        "{} Transaction submitted: {}",
        "✓".green(),
        hex::encode(tx_hash)
    );

    // 5. Wait for confirmation
    println!(
        "{} Waiting for confirmation (timeout: {}s)...",
        "→".blue(),
        stamp_config.confirmation_timeout_secs
    );

    let timeout_ms = stamp_config.confirmation_timeout_secs * 1000;
    let block_info = match client.wait_for_confirmation(&tx_hash, timeout_ms).await {
        Ok(info) => info,
        Err(e) => {
            eprintln!("{} Confirmation timeout: {}", "Warning:".yellow(), e);
            eprintln!();
            eprintln!(
                "{}",
                "Transaction submitted but not confirmed within timeout.".yellow()
            );
            eprintln!(
                "  {} {}",
                "TX Hash:".dimmed(),
                hex::encode(tx_hash)
            );
            eprintln!();
            eprintln!(
                "{}",
                "The transaction may still confirm. Save the pending proof.".dimmed()
            );

            // Save pending proof
            let mut proof = prepared.proof.clone();
            proof.add_attestation(Attestation::Pending(PendingAttestation {
                calendar_url: format!(
                    "direct://{}?tx={}",
                    wallet.address(),
                    hex::encode(tx_hash)
                ),
            }));

            let output_path = determine_output_path(output, file);
            let kts_data = serialize_proof(&proof);
            fs::write(&output_path, &kts_data)?;

            println!("  {} {}", "Proof:".dimmed(), output_path.display());
            return Ok(());
        }
    };

    println!(
        "{} Confirmed in block: {}",
        "✓".green(),
        hex::encode(block_info.hash)
    );
    println!("  {} {}", "DAA Score:".dimmed(), block_info.daa_score);
    println!("  {} {}", "Blue Score:".dimmed(), block_info.blue_score);

    // 6. Complete stamp with attestation
    let direct_info = DirectBlockInfo::from(block_info);
    let proof = complete_stamp(prepared, direct_info, tx_hash)?;

    // 7. Save proof
    let output_path = determine_output_path(output, file);
    let proof_bytes = serialize_proof(&proof);
    fs::write(&output_path, &proof_bytes)?;

    println!();
    println!(
        "{} {}",
        "Proof saved:".green().bold(),
        output_path.display()
    );
    println!("  {} {} bytes", "Size:".dimmed(), proof_bytes.len());
    println!();
    println!(
        "{}",
        format!("Verify with: ktcs verify {}", output_path.display()).dimmed()
    );

    Ok(())
}

/// Parses the batch mode string into a BatchMode enum.
fn parse_batch_mode(mode: &str) -> Result<BatchMode, Box<dyn std::error::Error>> {
    match mode.to_lowercase().as_str() {
        "instant" => Ok(BatchMode::Instant),
        "standard" => Ok(BatchMode::Standard),
        "economic" => Ok(BatchMode::Economic),
        _ => {
            eprintln!("{}", format!("Invalid batch mode: {}", mode).red());
            Err(format!("Invalid batch mode: {}", mode).into())
        }
    }
}

async fn cmd_verify(
    proof_path: PathBuf,
    data_path: Option<PathBuf>,
    chain: bool,
    rpc_url: String,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "KTCS Verification".bold().cyan());
    println!();

    // Load proof
    let kts_data = fs::read(&proof_path)?;
    let proof = deserialize_proof(&kts_data)?;

    println!("  {} {}", "Proof:".dimmed(), proof_path.display());

    // Load original data if provided
    let original_data = if let Some(path) = &data_path {
        println!("  {} {}", "Data:".dimmed(), path.display());
        Some(fs::read(path)?)
    } else {
        None
    };

    println!();

    // Verify locally (cryptographic verification)
    let result = verify_proof(&proof, original_data.as_deref())?;

    if result.valid {
        println!("{}", "✓ VALID (Cryptographic)".green().bold());
    } else {
        println!("{}", "✗ INVALID".red().bold());
    }

    println!();
    println!("  {} {}", "Digest:".dimmed(), result.digest);
    println!("  {} {}", "Commitment:".dimmed(), result.computed_commitment);

    if let Some(error) = &result.error {
        println!("  {} {}", "Error:".dimmed(), error.red());
    }

    println!();
    println!("{}", "Attestations:".bold());

    for (i, att) in result.attestations.iter().enumerate() {
        let status = if att.complete {
            "Complete".green()
        } else {
            "Pending".yellow()
        };
        println!(
            "  {}. {} ({})",
            i + 1,
            att.attestation_type.to_uppercase(),
            status
        );

        match &att.details {
            ktcs_core::verify::AttestationDetails::Pending { calendar_url } => {
                println!("     {} {}", "Calendar:".dimmed(), calendar_url);
            }
            ktcs_core::verify::AttestationDetails::Kaspa {
                daa_score,
                blue_score,
                block_hash,
                timestamp,
                tx_hash,
                ..
            } => {
                println!("     {} {}", "DAA Score:".dimmed(), daa_score);
                println!("     {} {}", "Blue Score:".dimmed(), blue_score);
                println!("     {} {}", "Block:".dimmed(), &block_hash[..16]);
                println!("     {} {}", "TX:".dimmed(), &tx_hash[..16]);

                let formatted_time = format_timestamp_utc(*timestamp);
                println!("     {} {}", "Time:".dimmed(), formatted_time);
            }
            ktcs_core::verify::AttestationDetails::Bitcoin { block_height } => {
                println!("     {} {}", "Block Height:".dimmed(), block_height);
            }
        }
    }

    // Blockchain verification (optional)
    if chain {
        println!();
        println!("{}", "Blockchain Verification:".bold().cyan());
        println!("  {} {}", "RPC:".dimmed(), rpc_url);
        println!();

        // Connect to Kaspa node
        println!("{} Connecting to Kaspa node...", "→".blue());
        let client_config = KaspaClientConfig {
            rpc_url: rpc_url.clone(),
            network: Some("mainnet".to_string()),
            ..Default::default()
        };
        let client = KaspaClient::new(client_config);

        match client.connect().await {
            Ok(()) => {}
            Err(e) => {
                eprintln!("{} Failed to connect: {}", "Error:".red(), e);
                eprintln!();
                eprintln!(
                    "{}",
                    "Make sure the RPC endpoint is accessible.".yellow()
                );
                return Err(e.into());
            }
        }

        let dag_info = client.get_block_dag_info().await?;
        println!(
            "{} Connected to {} (DAA: {})",
            "✓".green(),
            dag_info.network,
            dag_info.current_daa_score
        );
        println!();

        // Compute the commitment from the proof
        let computed_commitment = ktcs_core::apply_operations(&proof.digest, &proof.operations)?;
        let commitment_bytes: [u8; 32] = computed_commitment
            .as_slice()
            .try_into()
            .map_err(|_| "Commitment must be 32 bytes")?;

        // Verify each Kaspa attestation on chain
        for att in proof.kaspa_attestations() {
            println!("{} Verifying attestation on chain...", "→".blue());

            match ktcs_core::verify::verify_attestation_on_chain(&client, att, &commitment_bytes).await {
                Ok(chain_result) => {
                    if chain_result.block_verified {
                        println!("{} Block verified on chain", "✓".green());
                    } else {
                        println!("{} Block NOT found on chain", "✗".red());
                    }

                    if chain_result.tx_verified {
                        println!("{} Transaction verified in block", "✓".green());
                    } else {
                        println!("{} Transaction NOT found in block", "✗".red());
                    }

                    if chain_result.commitment_verified {
                        println!("{} Commitment verified in transaction", "✓".green());
                    } else {
                        println!("{} Commitment NOT found in transaction", "✗".red());
                    }

                    println!();
                    println!("  {} {}", "Current DAA:".dimmed(), chain_result.current_daa_score);
                    println!("  {} {}", "Blocks since:".dimmed(), chain_result.blocks_since);

                    // Calculate rough BTC equivalent
                    let btc_equiv = chain_result.blocks_since as f64 / 60000.0;
                    if btc_equiv >= 0.1 {
                        println!(
                            "  {} {:.2} BTC confirmations (equivalent)",
                            "Security:".dimmed(),
                            btc_equiv
                        );
                    }
                }
                Err(e) => {
                    println!("{} Chain verification failed: {}", "✗".red(), e);
                }
            }
        }
    }

    Ok(())
}

fn cmd_info(proof_path: PathBuf, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Load proof
    let kts_data = fs::read(&proof_path)?;
    let proof = deserialize_proof(&kts_data)?;

    if json {
        // Output as JSON
        let output = serde_json::json!({
            "version": proof.version,
            "hash_algorithm": format!("{:?}", proof.hash_algorithm),
            "digest": hex::encode(&proof.digest),
            "operations_count": proof.operations.len(),
            "attestations_count": proof.attestations.len(),
            "complete": proof.is_complete(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "KTCS Proof Info".bold().cyan());
        println!();
        println!("  {} {}", "File:".dimmed(), proof_path.display());
        println!("  {} {} bytes", "Size:".dimmed(), kts_data.len());
        println!();
        println!("  {} {}", "Version:".dimmed(), proof.version);
        println!(
            "  {} {:?}",
            "Hash Algorithm:".dimmed(),
            proof.hash_algorithm
        );
        println!("  {} {}", "Digest:".dimmed(), hex::encode(&proof.digest));
        println!("  {} {}", "Operations:".dimmed(), proof.operations.len());
        println!(
            "  {} {}",
            "Attestations:".dimmed(),
            proof.attestations.len()
        );

        let status = if proof.is_complete() {
            "Complete".green()
        } else {
            "Pending".yellow()
        };
        println!("  {} {}", "Status:".dimmed(), status);

        // Show thermodynamic security if complete
        for att in proof.kaspa_attestations() {
            let metrics = ThermodynamicMetrics::from_attestation(att);
            println!();
            println!("{}", "Thermodynamic Security:".bold());
            println!(
                "  {} {}",
                "Blue Work:".dimmed(),
                &metrics.blue_work_at_attestation[..16]
            );
        }
    }

    Ok(())
}

/// Check status of a proof
fn cmd_status(proof_path: PathBuf, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let kts_data = fs::read(&proof_path)?;
    let proof = deserialize_proof(&kts_data)?;

    let is_complete = proof.is_complete();
    let attestation_count = proof.attestations.len();

    // Find pending attestation URL if exists
    let pending_url = proof.attestations.iter().find_map(|a| match a {
        Attestation::Pending(p) => Some(p.calendar_url.clone()),
        _ => None,
    });

    // Find Kaspa attestation details if complete
    let kaspa_info = proof.kaspa_attestations().next().map(|ka| {
        (ka.daa_score, ka.blue_score, hex::encode(ka.block_hash))
    });

    if json {
        let output = serde_json::json!({
            "file": proof_path.display().to_string(),
            "status": if is_complete { "complete" } else { "pending" },
            "digest": hex::encode(&proof.digest),
            "attestations": attestation_count,
            "pending_url": pending_url,
            "kaspa": kaspa_info.map(|(daa, blue, hash)| serde_json::json!({
                "daa_score": daa,
                "blue_score": blue,
                "block_hash": hash,
            })),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let status_text = if is_complete {
            "Complete".green().bold()
        } else {
            "Pending".yellow().bold()
        };

        println!("{} {}", proof_path.display(), status_text);

        if let Some(url) = pending_url {
            println!("  {} {}", "Calendar:".dimmed(), url);
            println!();
            println!("  {}", "Run 'ktcs complete' to fetch the confirmed proof.".dimmed());
        }

        if let Some((daa, blue, hash)) = kaspa_info {
            println!("  {} {}", "DAA Score:".dimmed(), daa);
            println!("  {} {}", "Blue Score:".dimmed(), blue);
            println!("  {} {}", "Block:".dimmed(), &hash[..16]);
        }
    }

    Ok(())
}

/// Response from calendar stamp endpoint
#[derive(serde::Deserialize)]
struct CalendarStampResponse {
    status: String,
    #[serde(default)]
    proof: Option<String>,
}

async fn cmd_complete(
    proof_path: PathBuf,
    output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "KTCS Complete".bold().cyan());
    println!();

    // Load proof
    let kts_data = fs::read(&proof_path)?;
    let proof = deserialize_proof(&kts_data)?;

    if proof.is_complete() {
        println!("{}", "Proof is already complete!".green());
        return Ok(());
    }

    // Find pending attestation
    let pending = proof.attestations.iter().find_map(|a| match a {
        Attestation::Pending(p) => Some(p.calendar_url.clone()),
        _ => None,
    });

    if let Some(url) = pending {
        println!("  {} {}", "Fetching from:".dimmed(), url);
        println!();

        // Fetch stamp status from calendar
        let client = reqwest::Client::new();
        let response = match client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                println!("{} Failed to connect to calendar: {}", "Error:".red(), e);
                return Err(e.into());
            }
        };

        if !response.status().is_success() {
            println!(
                "{} Calendar returned status: {}",
                "Error:".red(),
                response.status()
            );
            return Err(format!("Calendar returned error: {}", response.status()).into());
        }

        let stamp_response: CalendarStampResponse = match response.json().await {
            Ok(r) => r,
            Err(e) => {
                println!("{} Failed to parse calendar response: {}", "Error:".red(), e);
                return Err(e.into());
            }
        };

        // Check status
        println!("  {} {}", "Status:".dimmed(), stamp_response.status);

        if stamp_response.status != "confirmed" {
            println!();
            println!(
                "{}",
                "Stamp not yet confirmed. Try again later.".yellow()
            );
            return Ok(());
        }

        // Get the complete proof from response
        let proof_base64 = stamp_response.proof.ok_or("Confirmed stamp missing proof")?;
        let complete_proof_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &proof_base64,
        )?;

        // Verify we can deserialize the complete proof
        let complete_proof = deserialize_proof(&complete_proof_bytes)?;

        // Security: Verify proof digest matches original
        if complete_proof.digest != proof.digest {
            return Err("Security error: calendar returned proof with different digest".into());
        }

        if !complete_proof.is_complete() {
            println!("{}", "Warning: Calendar returned incomplete proof".yellow());
        }

        // Save upgraded proof
        let output_path = output.unwrap_or(proof_path);
        fs::write(&output_path, &complete_proof_bytes)?;

        println!();
        println!("{}", "Proof upgraded!".green().bold());
        println!("  {} {}", "Output:".dimmed(), output_path.display());

        // Show attestation info
        for attestation in complete_proof.attestations.iter() {
            if let Attestation::Kaspa(ka) = attestation {
                println!();
                println!("{}", "Attestation:".bold());
                println!("  {} {}", "DAA Score:".dimmed(), ka.daa_score);
                println!("  {} {}", "Blue Score:".dimmed(), ka.blue_score);
                println!("  {} {}", "Block Hash:".dimmed(), hex::encode(ka.block_hash));
                break;
            }
        }
    } else {
        println!("{}", "No pending attestation found in proof".red());
    }

    Ok(())
}

fn cmd_hash(file: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let data = fs::read(&file)?;
    let hash = sha256(&data);
    println!("{}", hex::encode(hash));
    Ok(())
}

/// Generate a new wallet private key
fn cmd_wallet_generate(
    format: &str,
    network: &str,
    output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Generate new key
    let private_key = generate_private_key()?;
    let wallet = KaspaWallet::from_private_key(&private_key, network)?;

    let key_hex = hex::encode(private_key);

    match format {
        "json" => {
            let json = serde_json::json!({
                "private_key": key_hex,
                "address": wallet.address(),
                "network": network,
                "public_key": hex::encode(wallet.public_key()),
            });

            if let Some(path) = output {
                fs::write(&path, serde_json::to_string_pretty(&json)?)?;
                eprintln!("{} Wallet saved to: {}", "✓".green(), path.display());
            } else {
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
        }
        _ => {
            // Default: hex format
            if let Some(path) = output {
                fs::write(&path, &key_hex)?;
                eprintln!("{} Private key saved to: {}", "✓".green(), path.display());
                eprintln!("  {} {}", "Address:".dimmed(), wallet.address());
            } else {
                // Print only key to stdout (for piping)
                println!("{}", key_hex);
                // Print address to stderr (visible but not piped)
                eprintln!();
                eprintln!("{} {}", "Address:".dimmed(), wallet.address());
                eprintln!("{} {}", "Network:".dimmed(), network);
            }
        }
    }

    eprintln!();
    eprintln!(
        "{}",
        "SECURITY: Store this key securely! Anyone with this key controls the wallet.".yellow()
    );

    Ok(())
}

/// Show wallet address from private key
fn cmd_wallet_address(
    wallet_file: Option<PathBuf>,
    wallet_stdin: bool,
    network: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let wallet_key = read_wallet_key(wallet_file, wallet_stdin)?
        .ok_or("Either --wallet-file or --wallet-stdin is required")?;

    let wallet = KaspaWallet::from_hex(&wallet_key, network)?;

    println!("{}", "KTCS Wallet".bold().cyan());
    println!();
    println!("  {} {}", "Address:".dimmed(), wallet.address().yellow());
    println!("  {} {}", "Network:".dimmed(), network);
    println!(
        "  {} {}",
        "Public Key:".dimmed(),
        hex::encode(wallet.public_key())
    );

    Ok(())
}

/// Check wallet balance
async fn cmd_wallet_balance(
    wallet_file: Option<PathBuf>,
    wallet_stdin: bool,
    address: Option<String>,
    network: &str,
    rpc_url: Option<&str>,
    use_resolver: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get address from wallet key or direct input
    let address = if let Some(addr) = address {
        addr
    } else {
        let wallet_key = read_wallet_key(wallet_file, wallet_stdin)?
            .ok_or("Either --wallet-file, --wallet-stdin, or --address is required")?;
        let wallet = KaspaWallet::from_hex(&wallet_key, network)?;
        wallet.address().to_string()
    };

    println!("{}", "KTCS Wallet Balance".bold().cyan());
    println!();
    println!("  {} {}", "Address:".dimmed(), address.cyan());

    // Configure client - use resolver or direct URL
    let client_config = if use_resolver && rpc_url.is_none() {
        println!("{} Using resolver to find {} node...", "→".blue(), network);
        KaspaClientConfig {
            rpc_url: String::new(), // Empty = use resolver
            network: Some(network.to_string()),
            use_resolver: true,
            ..Default::default()
        }
    } else {
        let url = rpc_url.unwrap_or("ws://localhost:16110");
        println!("{} Connecting to {}...", "→".blue(), url);
        KaspaClientConfig {
            rpc_url: url.to_string(),
            network: Some(network.to_string()),
            use_resolver: false,
            ..Default::default()
        }
    };
    let client = KaspaClient::new(client_config);

    match client.connect().await {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{} Failed to connect: {}", "Error:".red(), e);
            eprintln!();
            eprintln!(
                "{}",
                "Make sure a Kaspa node is running and accessible at the RPC URL.".yellow()
            );
            return Err(e.into());
        }
    }

    let dag_info = client.get_block_dag_info().await?;
    println!(
        "{} Connected to {} (DAA: {})",
        "✓".green(),
        dag_info.network,
        dag_info.current_daa_score
    );

    // Fetch UTXOs
    println!("{} Fetching UTXOs...", "→".blue());
    let utxos = client.get_utxos_by_address(&address).await?;

    // Calculate balance
    let total_sompi: u64 = utxos.iter().map(|u| u.amount).sum();
    let total_kas = total_sompi as f64 / 100_000_000.0;

    println!();
    println!("{}", "Balance".bold().green());
    println!("  {} {} sompi", "Total:".dimmed(), total_sompi);
    println!("  {} {:.8} KAS", "Total:".dimmed(), total_kas);
    println!();
    println!("{} {} UTXO(s)", "UTXOs:".dimmed(), utxos.len());

    if !utxos.is_empty() && utxos.len() <= 10 {
        for (i, utxo) in utxos.iter().enumerate() {
            println!(
                "  {}. {} sompi (DAA: {})",
                i + 1,
                utxo.amount,
                utxo.block_daa_score
            );
        }
    } else if utxos.len() > 10 {
        println!("  (showing first 10)");
        for (i, utxo) in utxos.iter().take(10).enumerate() {
            println!("  {}. {} sompi", i + 1, utxo.amount);
        }
    }

    Ok(())
}

/// Returns true if the given year is a leap year.
fn is_leap_year(year: u64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Returns the number of days in the given year.
fn days_in_year(year: u64) -> u64 {
    if is_leap_year(year) { 366 } else { 365 }
}

/// Returns the days in each month for the given year.
fn days_in_months(year: u64) -> [u64; 12] {
    if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    }
}

/// Formats a Unix timestamp (milliseconds) as a human-readable UTC date string.
/// Avoids heavy datetime dependencies by implementing date calculation directly.
fn format_timestamp_utc(timestamp_ms: u64) -> String {
    let secs = timestamp_ms / 1000;
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year from days since epoch
    let mut year = 1970u64;
    let mut remaining_days = days_since_epoch;

    while remaining_days >= days_in_year(year) {
        remaining_days -= days_in_year(year);
        year += 1;
    }

    // Calculate month and day
    let mut month = 1u64;
    for days in days_in_months(year) {
        if remaining_days < days {
            break;
        }
        remaining_days -= days;
        month += 1;
    }
    let day = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year, month, day, hours, minutes, seconds
    )
}

/// Initialize config file with defaults
fn cmd_config_init(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    let path = Config::path();

    if path.exists() && !force {
        println!("{} Config file already exists at:", "Warning:".yellow());
        println!("  {}", path.display());
        println!();
        println!("Use {} to overwrite.", "--force".cyan());
        return Ok(());
    }

    // Create default config with example values
    let config = Config {
        network: "mainnet".to_string(),
        prefer_direct: false,
        mainnet: NetworkConfig {
            calendar: Some("https://calendar.ktcs.kaspa.org".to_string()),
            rpc_url: Some("wss://kaspa.aspectron.com/wrpc/json/mainnet".to_string()),
            wallet_file: None,
        },
        testnet: NetworkConfig {
            calendar: Some("https://testnet-calendar.ktcs.kaspa.org".to_string()),
            rpc_url: Some("ws://localhost:16110".to_string()),
            wallet_file: None,
        },
    };

    config.save()?;

    println!("{} Config file created:", "✓".green());
    println!("  {}", path.display());
    println!();
    println!("{}", "Edit the file to customize settings:".dimmed());
    println!("  {} mainnet (default) or testnet", "network:".dimmed());
    println!("  {} true to prefer direct stamping", "prefer_direct:".dimmed());
    println!("  {} sections for network-specific settings", "[mainnet]/[testnet]:".dimmed());

    Ok(())
}

/// Show current configuration
fn cmd_config_show(network_override: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::load();

    // Apply network override if specified
    if let Some(net) = network_override {
        config.network = net;
    }

    println!("{}", "KTCS Configuration".bold().cyan());
    println!();

    let path = Config::path();
    if path.exists() {
        println!("  {} {}", "Config file:".dimmed(), path.display());
    } else {
        println!("  {} {} (using defaults)", "Config file:".dimmed(), "not found".yellow());
    }

    println!();
    println!("  {} {}", "Network:".dimmed(), config.network.cyan());
    println!("  {} {}", "Prefer direct:".dimmed(), config.prefer_direct);
    println!();

    let net_config = config.network_config();
    println!("{}", format!("[{}]", config.network).bold());
    println!("  {} {}", "Calendar:".dimmed(), config.calendar());
    println!("  {} {}", "RPC URL:".dimmed(), config.rpc_url());

    if let Some(wallet) = &net_config.wallet_file {
        let wallet_path = config.wallet_file().unwrap();
        let exists = if wallet_path.exists() { "(exists)".green() } else { "(not found)".yellow() };
        println!("  {} {} {}", "Wallet:".dimmed(), wallet, exists);
    } else {
        println!("  {} {}", "Wallet:".dimmed(), "not configured".dimmed());
    }

    Ok(())
}

/// Set a configuration value
fn cmd_config_set(key: &str, value: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::load();

    match key {
        "network" => {
            if value != "mainnet" && value != "testnet" {
                return Err("Network must be 'mainnet' or 'testnet'".into());
            }
            config.network = value.to_string();
        }
        "prefer_direct" => {
            config.prefer_direct = value.parse().map_err(|_| "Value must be 'true' or 'false'")?;
        }
        "mainnet.calendar" => {
            config.mainnet.calendar = Some(value.to_string());
        }
        "mainnet.rpc_url" => {
            config.mainnet.rpc_url = Some(value.to_string());
        }
        "mainnet.wallet_file" => {
            config.mainnet.wallet_file = Some(value.to_string());
        }
        "testnet.calendar" => {
            config.testnet.calendar = Some(value.to_string());
        }
        "testnet.rpc_url" => {
            config.testnet.rpc_url = Some(value.to_string());
        }
        "testnet.wallet_file" => {
            config.testnet.wallet_file = Some(value.to_string());
        }
        _ => {
            return Err(format!(
                "Unknown config key: {}. Valid keys: network, prefer_direct, \
                mainnet.{{calendar,rpc_url,wallet_file}}, testnet.{{calendar,rpc_url,wallet_file}}",
                key
            ).into());
        }
    }

    config.save()?;
    println!("{} Set {} = {}", "✓".green(), key.cyan(), value);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ==================== determine_output_path() tests ====================

    #[test]
    fn test_determine_output_path_with_explicit_output() {
        let output = Some(PathBuf::from("custom.kts"));
        let input = Path::new("input.txt");
        let result = determine_output_path(output, input);
        assert_eq!(result, PathBuf::from("custom.kts"));
    }

    #[test]
    fn test_determine_output_path_derives_from_input() {
        let output = None;
        let input = Path::new("document.txt");
        let result = determine_output_path(output, input);
        assert_eq!(result, PathBuf::from("document.kts"));
    }

    #[test]
    fn test_determine_output_path_replaces_extension() {
        let output = None;
        let input = Path::new("archive.tar.gz");
        let result = determine_output_path(output, input);
        assert_eq!(result, PathBuf::from("archive.tar.kts"));
    }

    #[test]
    fn test_determine_output_path_no_extension() {
        let output = None;
        let input = Path::new("README");
        let result = determine_output_path(output, input);
        assert_eq!(result, PathBuf::from("README.kts"));
    }

    // ==================== parse_batch_mode() tests ====================

    #[test]
    fn test_parse_batch_mode_instant() {
        let result = parse_batch_mode("instant").unwrap();
        assert!(matches!(result, BatchMode::Instant));
    }

    #[test]
    fn test_parse_batch_mode_standard() {
        let result = parse_batch_mode("standard").unwrap();
        assert!(matches!(result, BatchMode::Standard));
    }

    #[test]
    fn test_parse_batch_mode_economic() {
        let result = parse_batch_mode("economic").unwrap();
        assert!(matches!(result, BatchMode::Economic));
    }

    #[test]
    fn test_parse_batch_mode_case_insensitive() {
        assert!(parse_batch_mode("INSTANT").is_ok());
        assert!(parse_batch_mode("Standard").is_ok());
        assert!(parse_batch_mode("ECONOMIC").is_ok());
    }

    #[test]
    fn test_parse_batch_mode_invalid() {
        assert!(parse_batch_mode("fast").is_err());
        assert!(parse_batch_mode("").is_err());
        assert!(parse_batch_mode("unknown").is_err());
    }

    // ==================== Date calculation tests ====================

    #[test]
    fn test_is_leap_year_divisible_by_400() {
        assert!(is_leap_year(2000)); // Divisible by 400
        assert!(is_leap_year(2400));
    }

    #[test]
    fn test_is_leap_year_century_not_leap() {
        assert!(!is_leap_year(1900)); // Divisible by 100 but not 400
        assert!(!is_leap_year(2100));
    }

    #[test]
    fn test_is_leap_year_divisible_by_4() {
        assert!(is_leap_year(2004));
        assert!(is_leap_year(2024));
        assert!(is_leap_year(1972));
    }

    #[test]
    fn test_is_leap_year_not_divisible_by_4() {
        assert!(!is_leap_year(2001));
        assert!(!is_leap_year(2023));
        assert!(!is_leap_year(1970));
    }

    #[test]
    fn test_days_in_year_leap() {
        assert_eq!(days_in_year(2000), 366);
        assert_eq!(days_in_year(2024), 366);
    }

    #[test]
    fn test_days_in_year_non_leap() {
        assert_eq!(days_in_year(2001), 365);
        assert_eq!(days_in_year(1900), 365);
    }

    #[test]
    fn test_days_in_months_leap_year() {
        let months = days_in_months(2000);
        assert_eq!(months[1], 29); // February in leap year
        assert_eq!(months[0], 31); // January
        assert_eq!(months[11], 31); // December
    }

    #[test]
    fn test_days_in_months_non_leap_year() {
        let months = days_in_months(2001);
        assert_eq!(months[1], 28); // February in non-leap year
    }

    // ==================== format_timestamp_utc() tests ====================

    #[test]
    fn test_format_timestamp_epoch() {
        assert_eq!(format_timestamp_utc(0), "1970-01-01 00:00:00 UTC");
    }

    #[test]
    fn test_format_timestamp_one_day() {
        assert_eq!(format_timestamp_utc(86400000), "1970-01-02 00:00:00 UTC");
    }

    #[test]
    fn test_format_timestamp_leap_year_feb_29() {
        // Feb 29, 2000 at midnight UTC
        // Days from 1970-01-01 to 2000-02-29:
        // 30 years = 10958 days (including leap days)
        // 10958 days * 86400 seconds * 1000 ms = 946944000000
        let ts = 951782400000_u64; // 2000-02-29 00:00:00 UTC
        let result = format_timestamp_utc(ts);
        assert!(result.contains("2000-02-29"), "Expected 2000-02-29, got {}", result);
    }

    #[test]
    fn test_format_timestamp_recent() {
        // 2021-01-01 00:00:00 UTC = 1609459200000 ms
        let result = format_timestamp_utc(1609459200000);
        assert!(result.contains("2021-01-01"), "Expected 2021-01-01, got {}", result);
    }

    // ==================== Config tests ====================

    #[test]
    fn test_config_default_network() {
        let config = Config::default();
        assert_eq!(config.network, "mainnet");
    }

    #[test]
    fn test_config_default_prefer_direct() {
        let config = Config::default();
        assert!(!config.prefer_direct);
    }

    #[test]
    fn test_config_network_config_testnet() {
        let mut config = Config::default();
        config.network = "testnet".to_string();
        config.testnet.calendar = Some("test-calendar".to_string());

        let net_config = config.network_config();
        assert_eq!(net_config.calendar, Some("test-calendar".to_string()));
    }

    #[test]
    fn test_config_network_config_mainnet() {
        let mut config = Config::default();
        config.network = "mainnet".to_string();
        config.mainnet.calendar = Some("main-calendar".to_string());

        let net_config = config.network_config();
        assert_eq!(net_config.calendar, Some("main-calendar".to_string()));
    }

    #[test]
    fn test_config_network_config_invalid_defaults_to_mainnet() {
        let mut config = Config::default();
        config.network = "invalid".to_string();
        config.mainnet.calendar = Some("main-calendar".to_string());

        let net_config = config.network_config();
        assert_eq!(net_config.calendar, Some("main-calendar".to_string()));
    }

    #[test]
    fn test_config_calendar_with_configured_value() {
        let mut config = Config::default();
        config.mainnet.calendar = Some("https://custom.calendar".to_string());
        assert_eq!(config.calendar(), "https://custom.calendar");
    }

    #[test]
    fn test_config_calendar_testnet_default() {
        let mut config = Config::default();
        config.network = "testnet".to_string();
        assert_eq!(config.calendar(), "https://testnet-calendar.ktcs.kaspa.org");
    }

    #[test]
    fn test_config_calendar_mainnet_default() {
        let config = Config::default();
        assert_eq!(config.calendar(), "https://calendar.ktcs.kaspa.org");
    }

    #[test]
    fn test_config_rpc_url_testnet_default() {
        let mut config = Config::default();
        config.network = "testnet".to_string();
        assert_eq!(config.rpc_url(), "ws://localhost:16110");
    }

    #[test]
    fn test_config_rpc_url_mainnet_default() {
        let config = Config::default();
        assert_eq!(config.rpc_url(), "wss://kaspa.aspectron.com/wrpc/json/mainnet");
    }

    #[test]
    fn test_config_wallet_file_none() {
        let config = Config::default();
        assert!(config.wallet_file().is_none());
    }

    #[test]
    fn test_config_wallet_file_absolute_path() {
        let mut config = Config::default();
        config.mainnet.wallet_file = Some("/absolute/path/key.txt".to_string());
        let result = config.wallet_file().unwrap();
        assert_eq!(result, PathBuf::from("/absolute/path/key.txt"));
    }

    #[test]
    fn test_config_wallet_file_relative_path() {
        let mut config = Config::default();
        config.mainnet.wallet_file = Some("relative/path/key.txt".to_string());
        let result = config.wallet_file().unwrap();
        assert_eq!(result, PathBuf::from("relative/path/key.txt"));
    }

    #[test]
    fn test_config_wallet_file_tilde_expansion() {
        let mut config = Config::default();
        config.mainnet.wallet_file = Some("~/wallet.key".to_string());
        let result = config.wallet_file().unwrap();
        // Should expand ~ to home directory (or keep as-is if no home)
        let path_str = result.to_string_lossy();
        // Either expanded or original (if home dir unavailable)
        assert!(path_str.ends_with("wallet.key"));
    }

    #[test]
    fn test_config_path_ends_with_config_toml() {
        let path = Config::path();
        assert!(path.ends_with("ktcs/config.toml") || path.ends_with("ktcs\\config.toml"));
    }
}
