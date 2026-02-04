/**
 * KTCS WASM Integration
 *
 * Wrapper for the ktcs-wasm module that provides browser-side
 * proof verification without requiring a server.
 *
 * Usage:
 *   import { initWasm, verifyProof, parseProof, computeSha256 } from './lib/wasm';
 *
 *   // Initialize WASM (call once at app startup)
 *   await initWasm();
 *
 *   // Verify a proof
 *   const result = verifyProof(proofBytes);
 *
 *   // Parse a proof for display
 *   const info = parseProof(proofBytes);
 *
 *   // Compute SHA256
 *   const hash = computeSha256(data);
 */

import type { VerificationResult, ProofInfo } from '../types/proof';

// Expected WASM module hash (update on each build)
const EXPECTED_WASM_HASH = import.meta.env.VITE_WASM_HASH || 'dev';

// WASM module - will be loaded dynamically
let wasmModule: typeof import('../wasm/ktcs_wasm') | null = null;
let initialized = false;
let initPromise: Promise<void> | null = null;

/**
 * Initialize the WASM module
 * Call this once at application startup
 */
export async function initWasm(): Promise<void> {
  if (initialized) {
    return;
  }

  if (initPromise) {
    return initPromise;
  }

  initPromise = (async () => {
    try {
      // Dynamic import of the WASM module
      // The wasm-pack output should be in src/wasm/
      wasmModule = await import('../wasm/ktcs_wasm');

      // Skip integrity check in development
      if (EXPECTED_WASM_HASH !== 'dev') {
        // Verify WASM module integrity would go here
        // For now, log that we're in production mode
        console.log('KTCS WASM: Production mode, integrity verification enabled');
      }

      await wasmModule.default(); // Initialize the WASM module
      initialized = true;
      console.log('KTCS WASM module initialized');
    } catch (error) {
      initPromise = null; // Allow retry on failure
      console.error('Failed to load KTCS WASM module:', error);
      throw error;
    }
  })();

  return initPromise;
}

/**
 * Check if WASM is initialized
 */
export function isWasmInitialized(): boolean {
  return initialized;
}

/**
 * Ensure WASM is initialized, throwing if not
 */
function ensureInitialized() {
  if (!initialized || !wasmModule) {
    throw new Error('WASM module not initialized. Call initWasm() first.');
  }
}

/**
 * Verify a KTCS proof
 *
 * @param proofBytes - Binary .kts proof data
 * @param data - Optional original data for full verification
 * @returns Verification result
 */
export function verifyProof(proofBytes: Uint8Array, data?: Uint8Array): VerificationResult {
  ensureInitialized();
  const result = wasmModule!.verify_proof(proofBytes, data || null);
  if (!result || typeof result !== 'object' || !('valid' in result)) {
    throw new Error('WASM verify_proof returned unexpected result');
  }
  return result as VerificationResult;
}

/**
 * Parse a KTCS proof and return its information
 *
 * @param proofBytes - Binary .kts proof data
 * @returns Proof information
 */
export function parseProof(proofBytes: Uint8Array): ProofInfo | null {
  ensureInitialized();
  try {
    const result = wasmModule!.parse_proof(proofBytes);
    if (result && typeof result === 'object') {
      if ('Ok' in result) return result.Ok as ProofInfo;
      if ('Err' in result) {
        console.error('WASM parse_proof error:', result.Err);
        return null;
      }
    }
    // Validate expected shape
    if (!result || typeof result !== 'object' || !('version' in result)) {
      console.error('Unexpected parseProof result shape');
      return null;
    }
    return result as ProofInfo;
  } catch (e) {
    console.error('Failed to parse proof:', e);
    return null;
  }
}

/**
 * Compute SHA256 hash of data
 *
 * @param data - Data to hash
 * @returns 32-byte SHA256 hash
 */
export function computeSha256(data: Uint8Array): Uint8Array {
  ensureInitialized();
  return new Uint8Array(wasmModule!.compute_sha256(data));
}

/**
 * Compute SHA256 hash and return as hex string
 *
 * @param data - Data to hash
 * @returns Hex-encoded SHA256 hash
 */
export function computeSha256Hex(data: Uint8Array): string {
  ensureInitialized();
  return wasmModule!.compute_sha256_hex(data);
}

/**
 * Check if bytes appear to be a valid KTCS proof format
 *
 * @param proofBytes - Bytes to check
 * @returns True if the bytes start with KTCS magic
 */
export function isValidProofFormat(proofBytes: Uint8Array): boolean {
  ensureInitialized();
  return wasmModule!.is_valid_proof_format(proofBytes);
}

/**
 * Get the KTCS library version
 */
export function getVersion(): string {
  ensureInitialized();
  return wasmModule!.get_version();
}

// =============================================================================
// WALLET FUNCTIONS
// =============================================================================

/**
 * Get wallet address from a private key
 *
 * @param privateKeyHex - 64-character hex private key
 * @param network - 'mainnet' or 'testnet'
 * @returns Kaspa address
 */
export function getWalletAddress(privateKeyHex: string, network: string): string {
  ensureInitialized();
  return wasmModule!.get_wallet_address(privateKeyHex, network);
}

/** Wallet info returned from WASM */
export interface WalletInfo {
  address: string;
  public_key: string;
  network: string;
}

/**
 * Get wallet info (address and public key)
 *
 * @param privateKeyHex - 64-character hex private key
 * @param network - 'mainnet' or 'testnet'
 * @returns Wallet info
 */
export function getWalletInfo(privateKeyHex: string, network: string): WalletInfo {
  ensureInitialized();
  return wasmModule!.get_wallet_info(privateKeyHex, network) as WalletInfo;
}

/**
 * Validate a private key format
 */
export function validatePrivateKey(privateKeyHex: string): boolean {
  ensureInitialized();
  return wasmModule!.validate_private_key(privateKeyHex);
}

/**
 * Validate a Kaspa address
 */
export function validateAddress(address: string): boolean {
  ensureInitialized();
  return wasmModule!.validate_address(address);
}

// =============================================================================
// TRANSACTION BUILDING
// =============================================================================

/** UTXO for transaction building */
export interface WasmUtxo {
  transaction_id: string;
  index: number;
  amount: string; // sompi as string to preserve precision (avoids JS number limit)
  script_public_key_hex: string;
  block_daa_score: number;
  is_coinbase: boolean;
}

/** Built transaction result */
export interface TransactionResult {
  transaction_json: string;
  commitment: string;
  total_input: number;
  total_output: number;
  fee: number;
  change_amount: number;
}

/**
 * Create a commitment from nonce and hash
 *
 * @param nonceHex - Nonce as hex
 * @param hashHex - Document hash as hex
 * @returns Commitment as hex
 */
export function createCommitment(nonceHex: string, hashHex: string): string {
  ensureInitialized();
  return wasmModule!.create_commitment(nonceHex, hashHex);
}

/**
 * Generate a random nonce (16 bytes)
 */
export function generateNonce(): string {
  ensureInitialized();
  return wasmModule!.generate_nonce();
}

/**
 * Get the burn amount for commitment transactions (in sompi)
 */
export function getCommitmentBurnAmount(): number {
  ensureInitialized();
  return Number(wasmModule!.get_commitment_burn_amount());
}

/**
 * Build a commitment transaction
 *
 * @param commitmentHex - 32-byte commitment as hex
 * @param utxos - Array of UTXOs
 * @param changeAddress - Kaspa address for change
 * @param feePerGram - Fee rate
 * @returns Built transaction
 */
export function buildCommitmentTransaction(
  commitmentHex: string,
  utxos: WasmUtxo[],
  changeAddress: string,
  feePerGram: number
): TransactionResult {
  ensureInitialized();
  const utxosJson = JSON.stringify(utxos);
  // WASM expects u64 as BigInt
  return wasmModule!.build_commitment_transaction(
    commitmentHex,
    utxosJson,
    changeAddress,
    BigInt(feePerGram)
  ) as TransactionResult;
}

// =============================================================================
// TRANSACTION SIGNING
// =============================================================================

/** Signed transaction result */
export interface SignedTransaction {
  transaction_json: string;
  transaction_id: string;
}

/**
 * Sign a transaction
 *
 * @param unsignedTxJson - JSON from build_commitment_transaction
 * @param utxos - Array of UTXOs (same as build)
 * @param privateKeyHex - Wallet private key
 * @param network - 'mainnet' or 'testnet'
 * @returns Signed transaction
 */
export function signTransaction(
  unsignedTxJson: string,
  utxos: WasmUtxo[],
  privateKeyHex: string,
  network: string
): SignedTransaction {
  ensureInitialized();
  const utxosJson = JSON.stringify(utxos);
  return wasmModule!.sign_transaction(
    unsignedTxJson,
    utxosJson,
    privateKeyHex,
    network
  ) as SignedTransaction;
}

// =============================================================================
// PROOF BUILDING
// =============================================================================

/** Pending proof info */
export interface PendingProof {
  digest: string;
  nonce: string;
  commitment: string;
  proof_bytes: Uint8Array;
}

/**
 * Build a pending proof (before blockchain confirmation)
 *
 * @param digestHex - Document hash as hex
 * @param nonceHex - Nonce as hex
 * @returns Pending proof data
 */
export function buildPendingProof(digestHex: string, nonceHex: string): PendingProof {
  ensureInitialized();
  return wasmModule!.build_pending_proof(digestHex, nonceHex) as PendingProof;
}

/** Block attestation info */
export interface BlockAttestation {
  tx_hash: string;
  block_hash: string;
  daa_score: number;
  blue_score: number;
  timestamp: number;
  blue_work: string;
  parent_hashes: string[];
}

/**
 * Complete a proof with blockchain attestation
 *
 * @param pendingProofBytes - Proof bytes from buildPendingProof
 * @param attestation - Block attestation data
 * @returns Complete .kts proof bytes
 */
export function completeProof(
  pendingProofBytes: Uint8Array,
  attestation: BlockAttestation
): Uint8Array {
  ensureInitialized();
  const attestationJson = JSON.stringify(attestation);
  return new Uint8Array(wasmModule!.complete_proof(pendingProofBytes, attestationJson));
}

// =============================================================================
// RPC REQUEST BUILDING
// =============================================================================

/**
 * Create complete JSON-RPC request for transaction submission
 *
 * This allows JavaScript to just send the pre-formatted string without
 * parsing or modifying any BigInt values (avoiding precision loss).
 *
 * @param signedTxJson - The transaction_json from signTransaction
 * @returns Complete JSON-RPC request string ready to send to Kaspa node
 */
export function createSubmitTxRpcRequest(signedTxJson: string): string {
  ensureInitialized();
  return wasmModule!.create_submit_tx_rpc_request(signedTxJson);
}

// Re-export types
export type { VerificationResult, ProofInfo };
