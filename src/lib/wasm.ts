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
      await wasmModule.default(); // Initialize the WASM module
      initialized = true;
      console.log('KTCS WASM module initialized');
    } catch (error) {
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
    if (result && typeof result === 'object' && 'Ok' in result) {
      return result.Ok as ProofInfo;
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

// Re-export types
export type { VerificationResult, ProofInfo };
