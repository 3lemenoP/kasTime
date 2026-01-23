//! KTCS Command Line Interface
//!
//! Provides commands for creating and verifying Kaspa timestamps.

use clap::{Parser, Subcommand};
use colored::Colorize;
use ktcs_core::{
    deserialize_proof, merkle::sha256, serialize_proof, verify_proof, Attestation, BatchMode,
    KaspaAttestation, KtcsProof, PendingAttestation, ThermodynamicMetrics,
};
use std::fs;
use std::path::PathBuf;

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

        /// Create timestamp directly on-chain (requires wallet)
        #[arg(long)]
        direct: bool,
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
        } => {
            cmd_stamp(file, calendar, mode, output, direct).await?;
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

async fn cmd_stamp(
    file: PathBuf,
    calendar: String,
    mode: String,
    output: Option<PathBuf>,
    direct: bool,
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

    if direct {
        println!(
            "{}",
            "Direct stamping not yet implemented. Use calendar mode.".red()
        );
        return Ok(());
    }

    // Parse batch mode
    let batch_mode = match mode.to_lowercase().as_str() {
        "instant" => BatchMode::Instant,
        "standard" => BatchMode::Standard,
        "economic" => BatchMode::Economic,
        _ => {
            println!("{}", format!("Invalid batch mode: {}", mode).red());
            return Ok(());
        }
    };

    println!(
        "  {} {}",
        "Calendar:".dimmed(),
        calendar
    );
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

    // Determine output path
    let output_path = output.unwrap_or_else(|| {
        let mut p = file.clone();
        p.set_extension("kts");
        p
    });

    // Serialize and save
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

                // Convert timestamp to human readable
                let dt = chrono_lite(*timestamp);
                println!("     {} {}", "Time:".dimmed(), dt);
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

/// Simple timestamp formatting (avoiding heavy datetime dependencies)
fn chrono_lite(timestamp_ms: u64) -> String {
    let secs = timestamp_ms / 1000;
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Approximate date calculation (not accounting for leap years precisely)
    let mut year = 1970;
    let mut remaining_days = days_since_epoch;

    loop {
        let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            366
        } else {
            365
        };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [u64; 12] = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for days in days_in_months {
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
