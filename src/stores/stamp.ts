/**
 * KTCS Stamp Store
 *
 * Zustand store for managing timestamp creation state.
 * Supports both calendar-based and direct (own wallet) stamping.
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import type { BatchMode } from '../types/proof';
import type { StampResponse } from '../types/api';
import type { KaspaUtxo, KaspaBlockInfo } from '../api/kaspa';

/** Stamping mode */
export type StampMode = 'calendar' | 'direct';

/** Network selection */
export type KaspaNetwork = 'mainnet' | 'testnet';

/** Stamping flow step */
export type StampStep = 'idle' | 'hashing' | 'submitting' | 'pending' | 'confirmed' | 'error';

/** Direct stamping step */
export type DirectStampStep =
  | 'idle'
  | 'connecting'
  | 'fetching-utxos'
  | 'building-tx'
  | 'signing'
  | 'submitting'
  | 'confirming'
  | 'complete'
  | 'error';

/** File metadata (serializable alternative to File object) */
export interface FileMetadata {
  name: string;
  size: number;
  type: string;
  lastModified: number;
}

/** Stamp store state */
export interface StampState {
  // ==========================================================================
  // Stamping Mode
  // ==========================================================================
  mode: StampMode;

  // ==========================================================================
  // Common State (both modes)
  // ==========================================================================

  // Current step in the stamping flow (calendar mode)
  step: StampStep;

  // File being stamped (metadata only, not the actual File object)
  file: FileMetadata | null;
  fileName: string | null;

  // Hash of the file
  hash: string | null;

  // Confirmed proof
  confirmedProof: string | null; // base64

  // Error state
  error: string | null;

  // ==========================================================================
  // Calendar Mode State
  // ==========================================================================

  // Batch mode for submission
  batchMode: BatchMode;

  // Submission response
  stampId: string | null;
  stampResponse: StampResponse | null;

  // ==========================================================================
  // Direct Mode State
  // ==========================================================================

  // Direct stamping step
  directStep: DirectStampStep;

  // Wallet (never persisted, memory only)
  walletKey: string | null; // Private key hex - NEVER persisted!
  walletAddress: string | null; // Derived address
  network: KaspaNetwork;

  // RPC connection
  rpcUrl: string;
  isRpcConnected: boolean;

  // UTXOs from wallet
  utxos: KaspaUtxo[] | null;
  walletBalance: bigint | null;

  // Direct stamp transaction state
  nonce: string | null; // hex
  commitment: string | null; // hex
  transactionId: string | null;
  blockInfo: KaspaBlockInfo | null;

  // ==========================================================================
  // Common Actions
  // ==========================================================================
  setMode: (mode: StampMode) => void;
  setFile: (file: File | null) => void;
  setHash: (hash: string) => void;
  setStep: (step: StampStep) => void;
  setConfirmedProof: (proof: string) => void;
  setError: (error: string) => void;
  reset: () => void;

  // ==========================================================================
  // Calendar Mode Actions
  // ==========================================================================
  setBatchMode: (mode: BatchMode) => void;
  setStampResponse: (response: StampResponse) => void;

  // ==========================================================================
  // Direct Mode Actions
  // ==========================================================================
  setWalletKey: (key: string | null) => void;
  setNetwork: (network: KaspaNetwork) => void;
  setRpcUrl: (url: string) => void;
  setRpcConnected: (connected: boolean) => void;
  setDirectStep: (step: DirectStampStep) => void;
  setUtxos: (utxos: KaspaUtxo[]) => void;
  setDirectStampState: (state: {
    nonce?: string;
    commitment?: string;
    transactionId?: string;
    blockInfo?: KaspaBlockInfo;
  }) => void;
}

/** Default RPC endpoint */
const DEFAULT_RPC_URL = 'wss://kaspa.aspectron.com/wrpc/json/mainnet';

const initialState = {
  // Mode
  mode: 'calendar' as StampMode,

  // Common state
  step: 'idle' as StampStep,
  file: null,
  fileName: null,
  hash: null,
  confirmedProof: null,
  error: null,

  // Calendar mode state
  batchMode: 'standard' as BatchMode,
  stampId: null,
  stampResponse: null,

  // Direct mode state
  directStep: 'idle' as DirectStampStep,
  walletKey: null,
  walletAddress: null,
  network: 'mainnet' as KaspaNetwork,
  rpcUrl: DEFAULT_RPC_URL,
  isRpcConnected: false,
  utxos: null,
  walletBalance: null,
  nonce: null,
  commitment: null,
  transactionId: null,
  blockInfo: null,
};

/**
 * Stamp store for managing timestamp creation
 */
export const useStampStore = create<StampState>()(
  persist(
    (set, get) => ({
      ...initialState,

  // ==========================================================================
  // Common Actions
  // ==========================================================================

  setMode: (mode: StampMode) =>
    set({
      mode,
      // Reset stamping state when switching modes
      step: 'idle',
      directStep: 'idle',
      error: null,
      // Clear the private key (and its derived data) whenever the user leaves
      // the direct-stamp flow — the key must not linger in the store.
      ...(mode !== 'direct'
        ? {
            walletKey: null,
            walletAddress: null,
            utxos: null,
            walletBalance: null,
          }
        : {}),
    }),

  setFile: (file: File | null) => {
    if (file) {
      set({
        file: {
          name: file.name,
          size: file.size,
          type: file.type,
          lastModified: file.lastModified,
        },
        fileName: file.name,
        step: 'hashing',
        directStep: 'idle',
        hash: null,
        stampId: null,
        stampResponse: null,
        confirmedProof: null,
        nonce: null,
        commitment: null,
        transactionId: null,
        blockInfo: null,
        error: null,
      });
    } else {
      // Reset ALL file-derived fields together so a stale fileName/hash can't
      // leak into (and mislabel) the next proof.
      set({
        file: null,
        fileName: null,
        hash: null,
        stampId: null,
        stampResponse: null,
        confirmedProof: null,
        nonce: null,
        commitment: null,
        transactionId: null,
        blockInfo: null,
        error: null,
      });
    }
  },

  setHash: (hash: string) =>
    set({
      hash,
      step: 'idle', // Ready to submit
    }),

  setStep: (step: StampStep) =>
    set({
      step,
    }),

  setConfirmedProof: (proof: string) =>
    // Only advance the state machine that is actually in use, so a calendar
    // confirmation can't move the direct machine (and vice versa).
    set(
      get().mode === 'direct'
        ? { confirmedProof: proof, directStep: 'complete' }
        : { confirmedProof: proof, step: 'confirmed' }
    ),

  setError: (error: string) =>
    set(
      get().mode === 'direct'
        ? { error, directStep: 'error' }
        : { error, step: 'error' }
    ),

  reset: () =>
    set({
      ...initialState,
      // Preserve mode, network, and RPC URL across resets
      mode: get().mode,
      network: get().network,
      rpcUrl: get().rpcUrl,
      // Do NOT preserve the wallet key — clearing state must also clear the
      // private key from memory (initialState leaves it null).
    }),

  // ==========================================================================
  // Calendar Mode Actions
  // ==========================================================================

  setBatchMode: (mode: BatchMode) =>
    set({
      batchMode: mode,
    }),

  setStampResponse: (response: StampResponse) =>
    set({
      stampId: response.id,
      stampResponse: response,
      step: response.status === 'confirmed' ? 'confirmed' : 'pending',
    }),

  // ==========================================================================
  // Direct Mode Actions
  // ==========================================================================

  setWalletKey: (key: string | null) => {
    // Note: This is stored in memory only, never persisted
    // The wallet address will be derived when needed via WASM
    set({
      walletKey: key,
      walletAddress: null, // Will be derived separately
      utxos: null,
      walletBalance: null,
    });
  },

  setNetwork: (network: KaspaNetwork) => {
    const rpcUrl =
      network === 'mainnet'
        ? 'wss://kaspa.aspectron.com/wrpc/json/mainnet'
        : 'wss://resolver.kaspa.stream/wrpc/testnet-11';

    set({
      network,
      rpcUrl,
      // Reset connection state when switching networks
      isRpcConnected: false,
      utxos: null,
      walletBalance: null,
      walletAddress: null, // Need to re-derive for new network
    });
  },

  setRpcUrl: (url: string) =>
    set({
      rpcUrl: url,
      isRpcConnected: false,
    }),

  setRpcConnected: (connected: boolean) =>
    set({
      isRpcConnected: connected,
    }),

  setDirectStep: (step: DirectStampStep) =>
    set({
      directStep: step,
    }),

  setUtxos: (utxos: KaspaUtxo[]) => {
    const balance = utxos.reduce((sum, u) => sum + u.amount, BigInt(0));
    set({
      utxos,
      walletBalance: balance,
    });
  },

  setDirectStampState: (state) => {
    const current = get();
    const updates: Partial<StampState> = {};

    // Only include fields that are provided and different from current state
    if (state.nonce !== undefined && state.nonce !== current.nonce) {
      updates.nonce = state.nonce;
    }
    if (state.commitment !== undefined && state.commitment !== current.commitment) {
      updates.commitment = state.commitment;
    }
    if (state.transactionId !== undefined && state.transactionId !== current.transactionId) {
      updates.transactionId = state.transactionId;
    }
    if (state.blockInfo !== undefined && state.blockInfo !== current.blockInfo) {
      updates.blockInfo = state.blockInfo;
    }

    // Only call set() if there are actual updates
    if (Object.keys(updates).length > 0) {
      set(updates);
    }
  },
    }),
    {
      name: 'ktcs-direct-proof-v2',
      storage: createJSONStorage(() => localStorage, {
        reviver: (_key, value) => {
          // Restore bigint from its "<digits>n" serialization. Match ONLY a
          // sign + digits + trailing 'n' — a naive endsWith('n') would try to
          // BigInt() ordinary strings that happen to end in 'n' (e.g. a base64
          // proof or a filename like "main"), throwing and breaking rehydration.
          if (typeof value === 'string' && /^-?\d+n$/.test(value)) {
            return BigInt(value.slice(0, -1));
          }
          return value;
        },
        replacer: (_key, value) => {
          // Serialize bigint as string with 'n' suffix
          if (typeof value === 'bigint') {
            return value.toString() + 'n';
          }
          return value;
        },
      }),
      partialize: (state) => ({
        // Persist everything needed to re-render a COMPLETED direct proof after
        // a reload. Without blockInfo the direct branch of ProofPage renders
        // "PROOF NOT FOUND" even though the proof itself is in localStorage.
        confirmedProof: state.confirmedProof,
        blockInfo: state.blockInfo,
        transactionId: state.transactionId,
        hash: state.hash,
        fileName: state.fileName,
        // NEVER persist:
        // - walletKey (security - critical)
        // - walletAddress (can be re-derived)
        // - utxos, walletBalance (stale data)
        // - nonce, commitment (transient tx-build state)
      }),
    }
  )
);
