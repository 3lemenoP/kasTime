/**
 * KTCS Stamp Store
 *
 * Zustand store for managing timestamp creation state.
 * Supports both calendar-based and direct (own wallet) stamping.
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import type { BatchMode, KtcsProof } from '../types/proof';
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

  // File being stamped
  file: File | null;
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
  setFile: (file: File) => void;
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
  clearWallet: () => void;
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
    }),

  setFile: (file: File) =>
    set({
      file,
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
    }),

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
    set({
      confirmedProof: proof,
      step: 'confirmed',
      directStep: 'complete',
    }),

  setError: (error: string) =>
    set({
      error,
      step: 'error',
      directStep: 'error',
    }),

  reset: () =>
    set({
      ...initialState,
      // Preserve mode, network, and RPC URL across resets
      mode: get().mode,
      network: get().network,
      rpcUrl: get().rpcUrl,
      // Preserve wallet if set (user might want to stamp another file)
      walletKey: get().walletKey,
      walletAddress: get().walletAddress,
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

  setDirectStampState: (state) =>
    set({
      ...(state.nonce !== undefined && { nonce: state.nonce }),
      ...(state.commitment !== undefined && { commitment: state.commitment }),
      ...(state.transactionId !== undefined && { transactionId: state.transactionId }),
      ...(state.blockInfo !== undefined && { blockInfo: state.blockInfo }),
    }),

  clearWallet: () =>
    set({
      walletKey: null,
      walletAddress: null,
      utxos: null,
      walletBalance: null,
    }),
    }),
    {
      name: 'ktcs-direct-proof',
      storage: createJSONStorage(() => localStorage, {
        reviver: (_key, value) => {
          // Restore bigint from string representation
          if (typeof value === 'string' && value.endsWith('n')) {
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
        // Only persist the proof itself for later download
        // All other data is transient and regenerated on each session
        confirmedProof: state.confirmedProof,
        // NEVER persist:
        // - walletKey (security - critical)
        // - walletAddress (can be re-derived)
        // - utxos, walletBalance (stale data)
        // - blockInfo, transactionId, hash, fileName (metadata exposure)
      }),
    }
  )
);

/**
 * Helper hook to get step display info (calendar mode)
 */
export function getStepInfo(step: StampStep): { label: string; description: string; progress: number } {
  switch (step) {
    case 'idle':
      return { label: 'Ready', description: 'Select a file to timestamp', progress: 0 };
    case 'hashing':
      return { label: 'Hashing', description: 'Computing SHA256 hash...', progress: 25 };
    case 'submitting':
      return { label: 'Submitting', description: 'Sending to calendar server...', progress: 50 };
    case 'pending':
      return { label: 'Pending', description: 'Waiting for block confirmation...', progress: 75 };
    case 'confirmed':
      return { label: 'Confirmed', description: 'Timestamp confirmed on Kaspa!', progress: 100 };
    case 'error':
      return { label: 'Error', description: 'Something went wrong', progress: 0 };
    default:
      return { label: 'Unknown', description: '', progress: 0 };
  }
}

/**
 * Helper hook to get direct stamping step display info
 */
export function getDirectStepInfo(step: DirectStampStep): {
  label: string;
  description: string;
  progress: number;
} {
  switch (step) {
    case 'idle':
      return { label: 'Ready', description: 'Ready to stamp directly', progress: 0 };
    case 'connecting':
      return { label: 'Connecting', description: 'Connecting to Kaspa node...', progress: 10 };
    case 'fetching-utxos':
      return { label: 'Fetching UTXOs', description: 'Getting wallet balance...', progress: 20 };
    case 'building-tx':
      return { label: 'Building', description: 'Building commitment transaction...', progress: 40 };
    case 'signing':
      return { label: 'Signing', description: 'Signing transaction...', progress: 50 };
    case 'submitting':
      return { label: 'Submitting', description: 'Submitting to Kaspa network...', progress: 60 };
    case 'confirming':
      return { label: 'Confirming', description: 'Waiting for block confirmation...', progress: 80 };
    case 'complete':
      return { label: 'Complete', description: 'Timestamp confirmed on Kaspa!', progress: 100 };
    case 'error':
      return { label: 'Error', description: 'Something went wrong', progress: 0 };
    default:
      return { label: 'Unknown', description: '', progress: 0 };
  }
}

/**
 * Format sompi to KAS with 2 decimal places
 */
export function formatKas(sompi: bigint): string {
  const kas = Number(sompi) / 100_000_000;
  return kas.toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}
