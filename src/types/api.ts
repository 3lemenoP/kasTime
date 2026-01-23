/**
 * KTCS API Types
 *
 * TypeScript types for the Calendar server REST API.
 */

import type { BatchMode } from './proof';

/** Request to submit a timestamp */
export interface StampRequest {
  /** Hex-encoded SHA256 hash */
  digest: string;
  /** Hash algorithm (currently only sha256) */
  algorithm?: 'sha256';
  /** Batching mode */
  batchMode?: BatchMode;
}

/** Response from stamp submission */
export interface StampResponse {
  /** Unique stamp ID (ktcs_...) */
  id: string;
  /** Current status */
  status: 'pending' | 'batched' | 'confirmed';
  /** ISO 8601 timestamp of submission */
  submittedAt: string;
  /** Estimated confirmation time (ISO 8601) */
  estimatedConfirmation?: string;
  /** Base64-encoded pending proof */
  pendingProof?: string;
  /** Base64-encoded confirmed proof */
  confirmedProof?: string;
  /** Attestation details (if confirmed) */
  attestation?: {
    daaScore: number;
    blueScore: number;
    blockHash: string;
    timestamp: number;
    txHash: string;
  };
}

/** Request to verify a proof */
export interface VerifyRequest {
  /** Binary .kts proof data */
  proof: ArrayBuffer;
}

/** Response from proof verification */
export interface VerifyResponse {
  /** Whether the proof is valid */
  valid: boolean;
  /** Hex-encoded digest */
  digest: string;
  /** Attestation information */
  attestations: Array<{
    type: string;
    complete: boolean;
    daaScore?: number;
    blueScore?: number;
    blockHash?: string;
    timestamp?: number;
    txHash?: string;
  }>;
  /** Error message if invalid */
  error?: string;
}

/** Health check response */
export interface HealthResponse {
  status: string;
  version: string;
  pendingStamps: number;
}

/** WebSocket message types */
export interface WsSubscribeMessage {
  type: 'subscribe';
  proofId: string;
}

export interface WsConfirmedMessage {
  type: 'confirmed';
  proofId: string;
  proof: string; // base64
}

export type WsMessage = WsSubscribeMessage | WsConfirmedMessage;
