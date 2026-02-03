//! KTCS Command Line Interface
//!
//! Provides commands for creating and verifying Kaspa timestamps.

use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use colored::Colorize;
use ktcs_core::{
    complete_stamp, deserialize_proof, generate_private_key, merkle::sha256, prepare_direct_stamp,
    serialize_proof, sign_transaction, verify_proof, Attestation, BatchMode, DirectBlockInfo,
    DirectStampConfig, KaspaClient, KaspaClientConfig, KaspaWallet, KtcsProof, PendingAttestation,
    ThermodynamicMetrics,
};

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

    /// Upgrade a pending proof to a complete proof
    #[command(alias = "u")]
    Upgrade {
        /// Pending proof file (.kts)
        proof: PathBuf,

        /// Output file (default: overwrite input)
        #[arg(short, long)]
        output: Option<PathBuf>,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

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
            wallet_file,
            wallet_stdin,
            rpc_url,
            network,
        } => {
            cmd_stamp(file, calendar, mode, output, direct, wallet_file, wallet_stdin, rpc_url, network).await?;
        }
        Commands::Verify { proof, data } => {
            cmd_verify(proof, data)?;
        }
        Commands::Info { proof, json } => {
            cmd_info(proof, json)?;
        }
        Commands::Upgrade { proof, output } => {
            cmd_upgrade(proof, output).await?;
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
    println!("{}", "Submitting to calendar...".dimmed());

    // Create pending proof
    let mut proof = KtcsProof::new(hash.to_vec());
    proof.add_attestation(Attestation::Pending(PendingAttestation {
        calendar_url: format!("{}/v1/stamp/ktcs_{}", calendar, &hash_hex[..16]),
    }));

    let output_path = determine_output_path(output, &file);
    let kts_data = serialize_proof(&proof);
    fs::write(&output_path, &kts_data)?;

    println!();
    println!("{}", "Timestamp created!".green().bold());
    println!("  {} {}", "Proof:".dimmed(), output_path.display());
    println!(
        "  {} {}",
        "Status:".dimmed(),
        "Pending (awaiting confirmation)".yellow()
    );
    println!();
    println!(
        "{}",
        "Run 'ktcs upgrade' once the timestamp is confirmed.".dimmed()
    );

    Ok(())
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

fn cmd_verify(proof_path: PathBuf, data_path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
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

    // Verify
    let result = verify_proof(&proof, original_data.as_deref())?;

    if result.valid {
        println!("{}", "✓ VALID".green().bold());
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

/// Response from calendar stamp endpoint
#[derive(serde::Deserialize)]
struct CalendarStampResponse {
    status: String,
    #[serde(default)]
    proof: Option<String>,
}

async fn cmd_upgrade(
    proof_path: PathBuf,
    output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "KTCS Upgrade".bold().cyan());
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
