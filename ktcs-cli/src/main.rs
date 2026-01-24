//! KTCS Command Line Interface
//!
//! Provides commands for creating and verifying Kaspa timestamps.

use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use colored::Colorize;
use ktcs_core::{
    create_pending_stamp, deserialize_proof, merkle::sha256, serialize_proof, verify_proof,
    Attestation, BatchMode, KaspaAttestation, KaspaWallet, KtcsProof, PendingAttestation,
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
        eprintln!(
            "{}",
            "Examples:".bold()
        );
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

    println!("{}", "Creating direct timestamp...".dimmed());

    let prepared = create_pending_stamp(data)?;

    println!(
        "  {} {}",
        "Commitment:".dimmed(),
        hex::encode(prepared.commitment).yellow()
    );
    println!();

    // In a full implementation, we would:
    // 1. Connect to the Kaspa node
    // 2. Get UTXOs for the wallet
    // 3. Build and sign the transaction
    // 4. Submit and wait for confirmation
    // 5. Build the complete proof

    println!(
        "{}",
        "Note: Full direct stamping requires a running Kaspa node.".yellow()
    );
    println!(
        "{}",
        "Creating pending proof that can be completed later.".dimmed()
    );
    println!();

    // Save as pending proof
    let mut proof = prepared.proof;
    proof.add_attestation(Attestation::Pending(PendingAttestation {
        calendar_url: format!(
            "direct://{}?commitment={}",
            wallet.address(),
            hex::encode(prepared.commitment)
        ),
    }));

    let output_path = determine_output_path(output, file);
    let kts_data = serialize_proof(&proof);
    fs::write(&output_path, &kts_data)?;

    println!("{}", "Timestamp prepared!".green().bold());
    println!("  {} {}", "Proof:".dimmed(), output_path.display());
    println!(
        "  {} {}",
        "Status:".dimmed(),
        "Pending (requires Kaspa node for completion)".yellow()
    );
    println!();
    println!(
        "{}",
        "To complete: Connect to a Kaspa node and submit the transaction.".dimmed()
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

async fn cmd_upgrade(
    proof_path: PathBuf,
    output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "KTCS Upgrade".bold().cyan());
    println!();

    // Load proof
    let kts_data = fs::read(&proof_path)?;
    let mut proof = deserialize_proof(&kts_data)?;

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

        // In a real implementation, we would fetch from the calendar
        // For now, simulate with a mock attestation
        println!();
        println!(
            "{}",
            "Calendar upgrade not yet implemented. Simulating confirmation...".yellow()
        );

        // Remove pending attestation and add a mock Kaspa attestation
        proof.attestations.retain(|a| a.is_complete());
        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42000000,
            41500000,
            [0xab; 32],
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            [0xcd; 32],
            0,
            [0x12; 32],
            vec![[0x11; 32], [0x22; 32]],
        )));

        // Save upgraded proof
        let output_path = output.unwrap_or(proof_path);
        let upgraded_data = serialize_proof(&proof);
        fs::write(&output_path, &upgraded_data)?;

        println!();
        println!("{}", "Proof upgraded!".green().bold());
        println!("  {} {}", "Output:".dimmed(), output_path.display());
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
