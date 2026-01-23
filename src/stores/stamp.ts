/**
 * KTCS Stamp Store
 *
 * Zustand store for managing timestamp creation state.
 */

import { create } from 'zustand';
import type { BatchMode, KtcsProof } from '../types/proof';
import type { StampResponse } from '../types/api';

/** Stamping flow step */
export type StampStep = 'idle' | 'hashing' | 'submitting' | 'pending' | 'confirmed' | 'error';

/** Stamp store state */
export interface StampState {
  // Current step in the stamping flow
  step: StampStep;

  // File being stamped
  file: File | null;
  fileName: string | null;

  // Hash of the file
  hash: string | null;

  // Batch mode for submission
  batchMode: BatchMode;

  // Submission response
  stampId: string | null;
  stampResponse: StampResponse | null;

  // Confirmed proof
  confirmedProof: string | null; // base64

  // Error state
  error: string | null;

  // Actions
  setFile: (file: File) => void;
  setHash: (hash: string) => void;
  setBatchMode: (mode: BatchMode) => void;
  setStep: (step: StampStep) => void;
  setStampResponse: (response: StampResponse) => void;
  setConfirmedProof: (proof: string) => void;
  setError: (error: string) => void;
  reset: () => void;
}

const initialState = {
  step: 'idle' as StampStep,
  file: null,
  fileName: null,
  hash: null,
  batchMode: 'standard' as BatchMode,
  stampId: null,
  stampResponse: null,
  confirmedProof: null,
  error: null,
};

/**
 * Stamp store for managing timestamp creation
 */
export const useStampStore = create<StampState>((set) => ({
  ...initialState,

  setFile: (file: File) =>
    set({
      file,
      fileName: file.name,
      step: 'hashing',
      hash: null,
      stampId: null,
      stampResponse: null,
      confirmedProof: null,
      error: null,
    }),

  setHash: (hash: string) =>
    set({
      hash,
      step: 'idle', // Ready to submit
    }),

  setBatchMode: (mode: BatchMode) =>
    set({
      batchMode: mode,
    }),

  setStep: (step: StampStep) =>
    set({
      step,
    }),

  setStampResponse: (response: StampResponse) =>
    set({
      stampId: response.id,
      stampResponse: response,
      step: response.status === 'confirmed' ? 'confirmed' : 'pending',
    }),

  setConfirmedProof: (proof: string) =>
    set({
      confirmedProof: proof,
      step: 'confirmed',
    }),

  setError: (error: string) =>
    set({
      error,
      step: 'error',
    }),

  reset: () => set(initialState),
}));

/**
 * Helper hook to get step display info
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
