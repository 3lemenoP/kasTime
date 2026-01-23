/**
 * KTCS Proof Types
 *
 * TypeScript types for the Kaspa Thermodynamic Clock Service proof format.
 * These mirror the Rust types in ktcs-core.
 */

/** Hash algorithm used in the proof */
export type HashAlgorithm = 'sha256' | 'ripemd160' | 'keccak256';

/** Batch mode for calendar aggregation */
export type BatchMode = 'instant' | 'standard' | 'economic';

/** Operation types for commitment sequence */
export type OperationType = 'append' | 'prepend' | 'sha256' | 'ripemd160' | 'keccak256' | 'fork';

/** An operation that transforms the hash state */
export interface Operation {
  type: OperationType;
  data?: Uint8Array;
}

/** Pending attestation - proof incomplete, needs calendar upgrade */
export interface PendingAttestation {
  type: 'pending';
  calendarUrl: string;
}

/** Kaspa block attestation - complete proof anchored to Kaspa */
export interface KaspaAttestation {
  type: 'kaspa';
  version: number;
  daaScore: bigint;
  blueScore: bigint;
  blockHash: string; // hex
  timestamp: number; // Unix milliseconds
  txHash: string; // hex
  txIndex: number;
  blueWork: string; // hex
  parentHashes: string[]; // hex array
}

/** Bitcoin attestation - for dual-anchor mode */
export interface BitcoinAttestation {
  type: 'bitcoin';
  blockHeight: number;
}

/** Union type for all attestation types */
export type Attestation = PendingAttestation | KaspaAttestation | BitcoinAttestation;

/** A complete KTCS proof */
export interface KtcsProof {
  version: number;
  hashAlgorithm: HashAlgorithm;
  digest: string; // hex-encoded
  operations: Operation[];
  attestations: Attestation[];
}

/** Verification result */
export interface VerificationResult {
  valid: boolean;
  digest: string;
  computedCommitment: string;
  attestations: AttestationInfo[];
  error?: string;
}

/** Attestation info for verification results */
export interface AttestationInfo {
  attestationType: string;
  complete: boolean;
  daaScore?: number;
  blueScore?: number;
  blockHash?: string;
  timestamp?: number;
  txHash?: string;
  calendarUrl?: string;
}

/** Thermodynamic security metrics */
export interface ThermodynamicMetrics {
  blueWorkAtAttestation: string;
  currentBlueWork?: string;
  accumulatedBlueWork?: string;
  blocksSince?: number;
  btcEquivalentConfirmations?: number;
}

/** Proof info for display */
export interface ProofInfo {
  version: number;
  hashAlgorithm: string;
  digest: string;
  operationsCount: number;
  attestationsCount: number;
  isComplete: boolean;
  attestations: AttestationInfo[];
}
