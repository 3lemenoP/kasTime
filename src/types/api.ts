/**
 * KTCS API Types
 *
 * TypeScript types for the Calendar server REST API.
 * These match the spec section 5.1 exactly.
 */

import type { BatchMode } from './proof';

/** Request to submit a timestamp */
export interface StampRequest {
  /** Hex-encoded SHA256 hash */
  digest: string;
  /** Hash algorithm (currently only sha256) */
  algorithm?: 'sha256';
  /** Batching mode */
  batch_mode?: BatchMode;
}

/** Thermodynamic weight information per spec Section 5.1.2 */
export interface ThermodynamicWeight {
  /** Blue work at the time of confirmation (scientific notation) */
  blue_work_at_confirmation: string;
  /** Current blue work on the chain */
  current_blue_work?: string;
  /** Blue work accumulated since confirmation */
  accumulated_since?: string;
}

/** Response from stamp submission - spec Section 5.1.1 and 5.1.2 */
export interface StampResponse {
  /** Unique stamp ID (ktcs_...) */
  id: string;
  /** Current status */
  status: 'pending' | 'batched' | 'confirmed';
  /** ISO 8601 timestamp of submission */
  submitted_at: string;
  /** Estimated confirmation time (ISO 8601) */
  estimated_confirmation?: string;
  /** Base64-encoded pending proof */
  pending_proof?: string;

  // Confirmed fields (flat per spec - not nested)
  /** ISO 8601 timestamp of confirmation */
  confirmed_at?: string;
  /** DAA score of confirming block */
  daa_score?: number;
  /** Blue score of confirming block */
  blue_score?: number;
  /** Block hash (hex) */
  block_hash?: string;
  /** Transaction hash (hex) */
  tx_hash?: string;
  /** Base64-encoded complete proof */
  proof?: string;
  /** Thermodynamic security metrics */
  thermodynamic_weight?: ThermodynamicWeight;
  /** Parent block hashes (hex, for confirmed stamps) */
  parent_hashes?: string[];
}

/** Current confirmations info per spec Section 5.1.3 */
export interface CurrentConfirmations {
  /** Blocks since attestation */
  blocks_since: number;
  /** Blue work accumulated (scientific notation) */
  blue_work_accumulated: string;
  /** Time elapsed in seconds */
  time_elapsed_seconds: number;
}

/** Attestation information in verification response */
export interface AttestationInfo {
  /** Type of attestation */
  type: 'kaspa_block' | 'pending' | 'bitcoin';
  /** DAA score (Kaspa only) */
  daa_score?: number;
  /** Blue score (Kaspa only) */
  blue_score?: number;
  /** Block hash (hex) */
  block_hash?: string;
  /** ISO 8601 timestamp */
  timestamp?: string;
  /** Thermodynamic weight (scientific notation) */
  thermodynamic_weight?: string;
}

/** Request to verify a proof */
export interface VerifyRequest {
  /** Binary .kts proof data */
  proof: ArrayBuffer;
}

/** Response from proof verification - spec Section 5.1.3 */
export interface VerifyResponse {
  /** Whether the proof is valid */
  valid: boolean;
  /** Hex-encoded digest */
  digest: string;
  /** Attestation information */
  attestations: AttestationInfo[];
  /** Current confirmations (if connected to chain) */
  current_confirmations?: CurrentConfirmations;
  /** Error message if invalid */
  error?: string;
}

/** Health check response */
export interface HealthResponse {
  status: string;
  version: string;
  pending_stamps: number;
}

/** WebSocket message from client */
export interface WsSubscribeMessage {
  type: 'subscribe';
  proof_id: string;
}

export interface WsUnsubscribeMessage {
  type: 'unsubscribe';
  proof_id: string;
}

export interface WsPingMessage {
  type: 'ping';
}

export type WsClientMessage = WsSubscribeMessage | WsUnsubscribeMessage | WsPingMessage;

/** WebSocket message from server */
export interface WsConfirmedMessage {
  type: 'confirmed';
  proof_id: string;
  block_hash: string;
  daa_score: number;
  blue_score: number;
  timestamp: number;
  proof: string; // base64
}

export interface WsSubscribedMessage {
  type: 'subscribed';
  proof_id: string;
}

export interface WsUnsubscribedMessage {
  type: 'unsubscribed';
  proof_id: string;
}

export interface WsPongMessage {
  type: 'pong';
}

export interface WsErrorMessage {
  type: 'error';
  message: string;
}

export type WsServerMessage =
  | WsConfirmedMessage
  | WsSubscribedMessage
  | WsUnsubscribedMessage
  | WsPongMessage
  | WsErrorMessage;
