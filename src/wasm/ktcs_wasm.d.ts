/* tslint:disable */
/* eslint-disable */

/**
 * Build a commitment transaction
 *
 * # Arguments
 * * `commitment_hex` - 32-byte commitment as hex
 * * `utxos_json` - JSON array of WasmUtxo
 * * `change_address` - Kaspa address for change
 * * `fee_per_gram` - Fee rate in sompi per gram
 *
 * # Returns
 * WasmTransactionResult as JsValue
 */
export function build_commitment_transaction(commitment_hex: string, utxos_json: string, change_address: string, fee_per_gram: bigint): any;

/**
 * Build a pending proof (before blockchain confirmation)
 *
 * # Arguments
 * * `digest_hex` - 32-byte document hash as hex
 * * `nonce_hex` - Nonce used for commitment
 *
 * # Returns
 * WasmPendingProof with partial proof data
 */
export function build_pending_proof(digest_hex: string, nonce_hex: string): any;

/**
 * Complete a proof with blockchain attestation
 *
 * # Arguments
 * * `pending_proof_bytes` - The proof_bytes from build_pending_proof
 * * `attestation_json` - JSON of WasmBlockAttestation
 *
 * # Returns
 * Complete .kts proof bytes
 */
export function complete_proof(pending_proof_bytes: Uint8Array, attestation_json: string): Uint8Array;

/**
 * Compute SHA256 hash of data
 *
 * # Arguments
 * * `data` - The data to hash
 *
 * # Returns
 * 32-byte SHA256 hash as Uint8Array
 */
export function compute_sha256(data: Uint8Array): Uint8Array;

/**
 * Compute SHA256 hash and return as hex string
 */
export function compute_sha256_hex(data: Uint8Array): string;

/**
 * Create a commitment from nonce and hash
 *
 * commitment = SHA256(nonce || hash)
 *
 * # Arguments
 * * `nonce_hex` - Hex-encoded nonce (typically 16 bytes)
 * * `hash_hex` - Hex-encoded document hash (32 bytes)
 *
 * # Returns
 * 32-byte commitment as hex string
 */
export function create_commitment(nonce_hex: string, hash_hex: string): string;

/**
 * Create complete JSON-RPC request for transaction submission
 *
 * This allows JavaScript to just send the pre-formatted string without
 * parsing or modifying any BigInt values (avoiding precision loss).
 *
 * # Arguments
 * * `signed_tx_json` - The transaction JSON from sign_transaction_for_direct_stamp
 *
 * # Returns
 * Complete JSON-RPC request string ready to send to Kaspa node
 */
export function create_submit_tx_rpc_request(signed_tx_json: string): string;

/**
 * Generate a random nonce (16 bytes)
 *
 * Uses browser's crypto.getRandomValues via getrandom
 */
export function generate_nonce(): string;

/**
 * Get the burn amount for commitment transactions
 */
export function get_commitment_burn_amount(): bigint;

/**
 * Get the KTCS library version
 */
export function get_version(): string;

/**
 * Get wallet address from a private key
 *
 * # Arguments
 * * `private_key_hex` - 64-character hex string (32 bytes)
 * * `network` - "mainnet" or "testnet"
 *
 * # Returns
 * Kaspa address string or error
 */
export function get_wallet_address(private_key_hex: string, network: string): string;

/**
 * Get wallet info (address and public key)
 *
 * # Arguments
 * * `private_key_hex` - 64-character hex string (32 bytes)
 * * `network` - "mainnet" or "testnet"
 *
 * # Returns
 * WasmWalletInfo as JsValue
 */
export function get_wallet_info(private_key_hex: string, network: string): any;

/**
 * Initialize panic hook for better error messages in console
 */
export function init(): void;

/**
 * Check if proof bytes appear to be a valid KTCS proof (quick check)
 */
export function is_valid_proof_format(proof_bytes: Uint8Array): boolean;

/**
 * Parse a KTCS proof and return its information
 *
 * # Arguments
 * * `proof_bytes` - The raw .kts proof file bytes
 *
 * # Returns
 * A JsValue containing the proof info
 */
export function parse_proof(proof_bytes: Uint8Array): any;

/**
 * Serialize a proof to .kts format
 *
 * This is primarily for testing - proofs are normally created by the calendar server
 */
export function serialize_proof_to_bytes(proof_json: any): Uint8Array;

/**
 * Sign a transaction
 *
 * # Arguments
 * * `unsigned_tx_json` - JSON serialized Transaction from build_commitment_transaction
 * * `utxos_json` - JSON array of WasmUtxo (same UTXOs used to build the transaction)
 * * `private_key_hex` - Wallet private key
 * * `network` - "mainnet" or "testnet"
 *
 * # Returns
 * WasmSignedTransaction with the signed transaction ready for submission
 */
export function sign_transaction(unsigned_tx_json: string, utxos_json: string, private_key_hex: string, network: string): any;

/**
 * Validate a Kaspa address
 */
export function validate_address(address: string): boolean;

/**
 * Validate a private key format
 */
export function validate_private_key(private_key_hex: string): boolean;

/**
 * Verify a KTCS proof
 *
 * # Arguments
 * * `proof_bytes` - The raw .kts proof file bytes
 * * `data` - Optional original data for full verification
 *
 * # Returns
 * A JsValue containing the verification result
 */
export function verify_proof(proof_bytes: Uint8Array, data?: Uint8Array | null): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly build_commitment_transaction: (a: number, b: number, c: number, d: number, e: number, f: number, g: bigint) => [number, number, number];
    readonly build_pending_proof: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly complete_proof: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly compute_sha256: (a: number, b: number) => [number, number];
    readonly compute_sha256_hex: (a: number, b: number) => [number, number];
    readonly create_commitment: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly create_submit_tx_rpc_request: (a: number, b: number) => [number, number, number, number];
    readonly generate_nonce: () => [number, number, number, number];
    readonly get_commitment_burn_amount: () => bigint;
    readonly get_version: () => [number, number];
    readonly get_wallet_address: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly get_wallet_info: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly is_valid_proof_format: (a: number, b: number) => number;
    readonly parse_proof: (a: number, b: number) => any;
    readonly serialize_proof_to_bytes: (a: any) => [number, number, number, number];
    readonly sign_transaction: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number];
    readonly validate_address: (a: number, b: number) => number;
    readonly validate_private_key: (a: number, b: number) => number;
    readonly verify_proof: (a: number, b: number, c: number, d: number) => any;
    readonly init: () => void;
    readonly rustsecp256k1_v0_10_0_context_create: (a: number) => number;
    readonly rustsecp256k1_v0_10_0_context_destroy: (a: number) => void;
    readonly rustsecp256k1_v0_10_0_default_error_callback_fn: (a: number, b: number) => void;
    readonly rustsecp256k1_v0_10_0_default_illegal_callback_fn: (a: number, b: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
