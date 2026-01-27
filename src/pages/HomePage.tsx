import { useState, useCallback, useEffect, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { ArrowRight, Shield, Zap, GitBranch, Wallet, Server } from 'lucide-react';

import FileDropZone from '../components/ui/FileDropZone';
import Button from '../components/ui/Button';
import MetricCard from '../components/ui/MetricCard';
import WalletInput from '../components/ui/WalletInput';
import {
  computeSha256Hex,
  isWasmInitialized,
  initWasm,
  getWalletAddress,
  generateNonce,
  createCommitment,
  buildCommitmentTransaction,
  signTransaction,
  buildPendingProof,
  completeProof,
  createSubmitTxRpcRequest,
} from '../lib/wasm';
import type { WasmUtxo } from '../lib/wasm';
import {
  useStampStore,
  getDirectStepInfo,
  formatKas,
  type StampMode,
  type DirectStampStep,
} from '../stores/stamp';
import { createCalendarClient } from '../api/calendar';
import { KaspaClient, KASPA_PUBLIC_ENDPOINTS } from '../api/kaspa';
import type { BatchMode } from '../types/proof';

const CALENDAR_URL = import.meta.env.VITE_CALENDAR_URL || 'http://localhost:3001';

const BATCH_MODE_CONFIG: Record<BatchMode, { timing: string; description: string }> = {
  instant: {
    timing: '~100ms',
    description: 'Fastest confirmation, higher cost per stamp',
  },
  standard: {
    timing: '~1s',
    description: 'Balanced speed and cost (recommended)',
  },
  economic: {
    timing: '~10s',
    description: 'Lowest cost, batched with other stamps',
  },
};

const BATCH_MODES: BatchMode[] = ['instant', 'standard', 'economic'];

interface BatchModeButtonProps {
  mode: BatchMode;
  isSelected: boolean;
  onClick: () => void;
}

function BatchModeButton({ mode, isSelected, onClick }: BatchModeButtonProps): JSX.Element {
  const { timing } = BATCH_MODE_CONFIG[mode];
  const baseClasses = 'px-3 py-1.5 rounded text-sm font-medium transition-colors';
  const selectedClasses = 'bg-[var(--accent-primary)] text-black';
  const unselectedClasses =
    'bg-[var(--bg-tertiary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]';

  return (
    <button
      onClick={onClick}
      className={`${baseClasses} ${isSelected ? selectedClasses : unselectedClasses}`}
    >
      {mode.toUpperCase()}
      <span className="text-xs ml-1 opacity-70">({timing})</span>
    </button>
  );
}

interface ModeToggleProps {
  mode: StampMode;
  onModeChange: (mode: StampMode) => void;
}

function ModeToggle({ mode, onModeChange }: ModeToggleProps): JSX.Element {
  return (
    <div className="flex items-center gap-2 p-1 rounded-lg bg-[var(--bg-tertiary)] border border-[var(--border-subtle)]">
      <button
        onClick={() => onModeChange('calendar')}
        className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-all ${
          mode === 'calendar'
            ? 'bg-[var(--accent-primary)] text-black shadow-sm'
            : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
        }`}
      >
        <Server className="w-4 h-4" />
        Calendar
      </button>
      <button
        onClick={() => onModeChange('direct')}
        className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-all ${
          mode === 'direct'
            ? 'bg-[var(--accent-primary)] text-black shadow-sm'
            : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
        }`}
      >
        <Wallet className="w-4 h-4" />
        Direct
      </button>
    </div>
  );
}

function HomePage(): JSX.Element {
  const navigate = useNavigate();
  const [hash, setHash] = useState('');
  const [isHashing, setIsHashing] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const kaspaClientRef = useRef<KaspaClient | null>(null);

  const {
    mode,
    setMode,
    setFile,
    setHash: setStoreHash,
    setStampResponse,
    setError: setStoreError,
    batchMode,
    setBatchMode,
    // Direct mode state
    walletKey,
    setWalletKey,
    walletAddress,
    network,
    rpcUrl,
    setRpcUrl,
    utxos,
    setUtxos,
    walletBalance,
    directStep,
    setDirectStep,
    setDirectStampState,
    setConfirmedProof,
    isRpcConnected,
    setRpcConnected,
  } = useStampStore();

  // Derive wallet address when key changes
  useEffect(() => {
    if (!walletKey || walletKey.length !== 64) {
      return;
    }

    const deriveAddress = async () => {
      try {
        await initWasm();
        const addr = getWalletAddress(walletKey, network);
        useStampStore.setState({ walletAddress: addr });
      } catch (e) {
        console.error('Failed to derive address:', e);
      }
    };

    deriveAddress();
  }, [walletKey, network]);

  // Fetch UTXOs when address changes
  useEffect(() => {
    if (!walletAddress || mode !== 'direct') {
      return;
    }

    const fetchUtxos = async () => {
      try {
        setDirectStep('fetching-utxos');

        // Create or reuse Kaspa client
        if (!kaspaClientRef.current || kaspaClientRef.current['rpcUrl'] !== rpcUrl) {
          kaspaClientRef.current = new KaspaClient(rpcUrl);
        }

        await kaspaClientRef.current.connect();
        setRpcConnected(true);

        const utxoList = await kaspaClientRef.current.getUtxosByAddress(walletAddress);
        const wasmUtxos = utxoList.map((u) => ({
          transaction_id: u.transactionId,
          index: u.index,
          amount: Number(u.amount),
          script_public_key_hex: u.scriptPublicKey.scriptPublicKey,
          block_daa_score: Number(u.blockDaaScore),
          is_coinbase: u.isCoinbase,
        }));

        setUtxos(utxoList);
        setDirectStep('idle');
      } catch (e) {
        console.error('Failed to fetch UTXOs:', e);
        setError('Failed to connect to Kaspa node');
        setDirectStep('error');
        setRpcConnected(false);
      }
    };

    fetchUtxos();
  }, [walletAddress, rpcUrl, mode]);

  const handleFileDrop = useCallback(
    async (file: File) => {
      setIsHashing(true);
      setError(null);
      try {
        const buffer = await file.arrayBuffer();
        const data = new Uint8Array(buffer);

        // Use WASM module for hashing if available, fallback to Web Crypto
        let hashHex: string;
        if (isWasmInitialized()) {
          hashHex = computeSha256Hex(data);
          console.log('Hash computed using KTCS WASM module');
        } else {
          // Fallback to Web Crypto API
          const hashBuffer = await crypto.subtle.digest('SHA-256', buffer);
          const hashArray = Array.from(new Uint8Array(hashBuffer));
          hashHex = hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
          console.log('Hash computed using Web Crypto API (WASM not ready)');
        }

        setHash(hashHex);
        setFile(file);
        setStoreHash(hashHex);
      } catch (err) {
        console.error('Error hashing file:', err);
        setError('Failed to hash file');
      } finally {
        setIsHashing(false);
      }
    },
    [setFile, setStoreHash]
  );

  // Calendar mode stamp
  const handleCalendarStamp = async () => {
    if (!hash) return;

    setIsSubmitting(true);
    setError(null);

    try {
      const client = createCalendarClient(CALENDAR_URL);
      const response = await client.stamp(hash, batchMode);

      setStampResponse(response);

      // Navigate to the proof page
      navigate(`/proof/${response.id}`);
    } catch (err) {
      console.error('Error submitting stamp:', err);
      const errorMsg = err instanceof Error ? err.message : 'Failed to submit timestamp';
      setError(errorMsg);
      setStoreError(errorMsg);
    } finally {
      setIsSubmitting(false);
    }
  };

  // Direct mode stamp
  const handleDirectStamp = async () => {
    if (!hash || !walletKey || !walletAddress || !utxos) return;

    setIsSubmitting(true);
    setError(null);

    try {
      await initWasm();

      // Step 1: Generate nonce and commitment
      setDirectStep('building-tx');
      const nonce = generateNonce();
      const commitment = createCommitment(nonce, hash);
      setDirectStampState({ nonce, commitment });

      // Step 2: Build transaction
      // scriptPublicKey from Kaspa RPC includes version prefix (2 bytes = 4 hex chars)
      // We need to extract just the raw script bytes for sighash computation
      const getScriptHex = (spk: unknown): string => {
        let script: string;
        if (typeof spk === 'string') {
          script = spk;
        } else if (spk && typeof spk === 'object' && 'scriptPublicKey' in spk) {
          script = (spk as { scriptPublicKey: string }).scriptPublicKey;
        } else {
          return '';
        }
        // Strip version prefix (first 4 hex chars = 2 bytes) if present
        // Version 0 = "0000", script starts after that
        if (script.length > 4 && script.startsWith('0000')) {
          return script.slice(4);
        }
        return script;
      };

      const wasmUtxos: WasmUtxo[] = utxos.map((u) => ({
        transaction_id: u.transactionId,
        index: u.index,
        amount: Number(u.amount),
        script_public_key_hex: getScriptHex(u.scriptPublicKey),
        block_daa_score: Number(u.blockDaaScore),
        is_coinbase: u.isCoinbase,
      }));
      const txResult = buildCommitmentTransaction(commitment, wasmUtxos, walletAddress, 10);

      // Step 3: Sign transaction
      setDirectStep('signing');
      const signedTx = signTransaction(txResult.transaction_json, wasmUtxos, walletKey, network);
      setDirectStampState({ transactionId: signedTx.transaction_id });

      // Step 4: Submit transaction
      // Use WASM to create the complete RPC request, avoiding JS BigInt precision issues
      setDirectStep('submitting');
      const client = kaspaClientRef.current!;

      console.log('DEBUG: Submitting transaction via WASM RPC request');
      const submittedTxId = await client.submitTransactionFromWasm(
        signedTx.transaction_json,
        createSubmitTxRpcRequest
      );
      console.log('Transaction submitted:', submittedTxId);

      // Step 5: Wait for confirmation
      setDirectStep('confirming');
      const confirmation = await client.waitForTransactionAcceptance(submittedTxId, 60000);

      // Step 6: Build proof
      const blockInfo = confirmation.blockInfo;
      setDirectStampState({
        blockInfo: {
          hash: blockInfo.hash,
          daaScore: blockInfo.daaScore,
          blueScore: blockInfo.blueScore,
          blueWork: blockInfo.blueWork,
          timestamp: blockInfo.timestamp,
          parentHashes: blockInfo.parentHashes,
          isChainBlock: blockInfo.isChainBlock,
        },
      });

      // Build pending proof
      const pendingProof = buildPendingProof(hash, nonce);

      // Complete proof with attestation
      const attestation = {
        tx_hash: submittedTxId,
        block_hash: blockInfo.hash,
        daa_score: Number(blockInfo.daaScore),
        blue_score: Number(blockInfo.blueScore),
        timestamp: Number(blockInfo.timestamp),
        blue_work: blockInfo.blueWork,
        parent_hashes: blockInfo.parentHashes,
      };

      const completeProofBytes = completeProof(pendingProof.proof_bytes, attestation);

      // Store as base64 for download
      const base64Proof = btoa(String.fromCharCode(...completeProofBytes));
      setConfirmedProof(base64Proof);

      setDirectStep('complete');

      // Navigate to a results page or show download
      // For now, show success message
    } catch (err) {
      console.error('Direct stamp failed:', err);
      const errorMsg = err instanceof Error ? err.message : 'Direct stamp failed';
      setError(errorMsg);
      setStoreError(errorMsg);
      setDirectStep('error');
    } finally {
      setIsSubmitting(false);
    }
  };

  // Commitment burn amount is constant: 0.2 KAS = 20,000,000 sompi
  // Using hardcoded value to avoid WASM call during render (before init)
  const BURN_AMOUNT_SOMPI = 20_000_000;
  const burnKas = BURN_AMOUNT_SOMPI / 100_000_000;
  const hasEnoughBalance = walletBalance !== null && walletBalance >= BigInt(BURN_AMOUNT_SOMPI + 10000);

  const directStepInfo = getDirectStepInfo(directStep);

  return (
    <div className="max-w-4xl mx-auto px-6 py-12">
      {/* Hero Section */}
      <div className="text-center mb-12">
        <h1 className="text-display-lg font-display mb-4 tracking-tight">
          <span className="text-[var(--text-primary)]">TRUSTLESS </span>
          <span className="text-[var(--accent-primary)]">TIMESTAMPING</span>
        </h1>
        <p className="text-body-lg text-[var(--text-secondary)] max-w-2xl mx-auto">
          Prove data existed. Backed by physics. Sub-second confirmation with Kaspa's 10 blocks per
          second BlockDAG.
        </p>
      </div>

      {/* Mode Toggle */}
      <div className="flex justify-center mb-6">
        <ModeToggle mode={mode} onModeChange={setMode} />
      </div>

      {/* Stamp Interface */}
      <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-8 mb-8">
        {/* Direct Mode: Wallet Setup */}
        {mode === 'direct' && (
          <div className="mb-6 pb-6 border-b border-[var(--border-subtle)]">
            <WalletInput
              value={walletKey || ''}
              onChange={setWalletKey}
              network={network}
              address={walletAddress}
              onAddressChange={(addr) => useStampStore.setState({ walletAddress: addr })}
              disabled={isSubmitting}
            />

            {/* Balance Display */}
            {walletBalance !== null && (
              <div className="mt-4 flex items-center justify-between p-3 rounded bg-[var(--bg-elevated)] border border-[var(--border-default)]">
                <span className="text-sm text-[var(--text-secondary)]">Wallet Balance</span>
                <span className="font-mono text-lg text-[var(--accent-primary)]">
                  {formatKas(walletBalance)} KAS
                </span>
              </div>
            )}

            {/* Cost Warning */}
            {walletAddress && (
              <div className="mt-4 p-3 rounded bg-[var(--bg-warning)]/10 border border-[var(--bg-warning)]/30">
                <p className="text-xs text-[var(--text-secondary)]">
                  <span className="font-medium text-[var(--bg-warning)]">Direct stamping cost:</span>{' '}
                  {burnKas} KAS burned per stamp + network fee
                </p>
              </div>
            )}

            {/* RPC Endpoint */}
            <div className="mt-4">
              <label className="text-xs text-[var(--text-tertiary)] mb-1.5 block uppercase tracking-wider">
                Kaspa RPC Endpoint
              </label>
              <select
                value={rpcUrl}
                onChange={(e) => setRpcUrl(e.target.value)}
                className="w-full bg-[var(--bg-input)] border border-[var(--border-default)] rounded px-3 py-2 text-sm text-[var(--text-primary)] focus:border-[var(--accent-primary)] focus:outline-none"
                disabled={isSubmitting}
              >
                {KASPA_PUBLIC_ENDPOINTS.mainnet.map((ep) => (
                  <option key={ep.url} value={ep.url}>
                    {ep.name} (Mainnet)
                  </option>
                ))}
              </select>
              <div className="flex items-center gap-2 mt-1">
                <div
                  className={`w-2 h-2 rounded-full ${isRpcConnected ? 'bg-[var(--bg-success)]' : 'bg-[var(--text-tertiary)]'}`}
                />
                <span className="text-xs text-[var(--text-tertiary)]">
                  {isRpcConnected ? 'Connected' : 'Disconnected'}
                </span>
              </div>
            </div>
          </div>
        )}

        <FileDropZone onFileDrop={handleFileDrop} isLoading={isHashing} />

        <div className="mt-6">
          <label className="text-label block mb-2">DOCUMENT HASH</label>
          <div className="flex gap-3">
            <div className="flex-1 relative">
              <span className="absolute left-4 top-1/2 -translate-y-1/2 text-[var(--text-tertiary)] font-data text-sm">
                0x
              </span>
              <input
                type="text"
                value={hash}
                onChange={(e) => setHash(e.target.value)}
                placeholder="Enter SHA-256 hash or drop file above"
                className="w-full bg-[var(--bg-quaternary)] border border-[var(--border-default)] rounded px-4 py-3 pl-10 font-data text-sm text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)] focus:border-[var(--accent-primary)] focus:outline-none transition-colors"
              />
            </div>
            {mode === 'calendar' ? (
              <Button onClick={handleCalendarStamp} disabled={!hash || isSubmitting}>
                {isSubmitting ? 'SUBMITTING...' : 'STAMP'}
                {!isSubmitting && <ArrowRight className="w-4 h-4 ml-2" />}
              </Button>
            ) : (
              <Button
                onClick={handleDirectStamp}
                disabled={!hash || !walletKey || !hasEnoughBalance || isSubmitting}
              >
                {isSubmitting ? directStepInfo.label.toUpperCase() : 'STAMP DIRECTLY'}
                {!isSubmitting && <ArrowRight className="w-4 h-4 ml-2" />}
              </Button>
            )}
          </div>
          <p className="text-xs text-[var(--text-tertiary)] mt-2">
            File never leaves your device. Hash computed locally in browser.
          </p>

          {/* Calendar Mode: Batch Mode Selector */}
          {mode === 'calendar' && (
            <div className="mt-4 pt-4 border-t border-[var(--border-subtle)]">
              <label className="text-label block mb-2">BATCH MODE</label>
              <div className="flex gap-2">
                {BATCH_MODES.map((bm) => (
                  <BatchModeButton
                    key={bm}
                    mode={bm}
                    isSelected={batchMode === bm}
                    onClick={() => setBatchMode(bm)}
                  />
                ))}
              </div>
              <p className="text-xs text-[var(--text-tertiary)] mt-1">
                {BATCH_MODE_CONFIG[batchMode].description}
              </p>
            </div>
          )}

          {/* Direct Mode: Progress Indicator */}
          {mode === 'direct' && directStep !== 'idle' && directStep !== 'error' && (
            <div className="mt-4 pt-4 border-t border-[var(--border-subtle)]">
              <div className="flex items-center justify-between mb-2">
                <span className="text-sm text-[var(--text-secondary)]">{directStepInfo.label}</span>
                <span className="text-xs text-[var(--text-tertiary)]">
                  {directStepInfo.progress}%
                </span>
              </div>
              <div className="w-full h-2 bg-[var(--bg-tertiary)] rounded-full overflow-hidden">
                <div
                  className="h-full bg-[var(--accent-primary)] transition-all duration-300"
                  style={{ width: `${directStepInfo.progress}%` }}
                />
              </div>
              <p className="text-xs text-[var(--text-tertiary)] mt-1">
                {directStepInfo.description}
              </p>
            </div>
          )}

          {error && <p className="text-xs text-red-500 mt-2">{error}</p>}
        </div>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-3 gap-4 mb-12">
        <MetricCard
          label="NETWORK STATUS"
          value="OPERATIONAL"
          status="success"
          sublabel="10 BPS | 42.8M DAA"
        />
        <MetricCard label="LAST STAMP" value="0.3s ago" sublabel="abc123..." />
        <MetricCard label="SECURITY" value="1.23 x 10^18" sublabel="~6 BTC confirmations" />
      </div>

      {/* Features */}
      <div className="grid grid-cols-3 gap-6">
        <div className="p-6 bg-[var(--bg-tertiary)] rounded-lg border border-[var(--border-subtle)]">
          <Zap className="w-8 h-8 text-[var(--accent-primary)] mb-4" />
          <h3 className="text-heading-md text-[var(--text-primary)] mb-2">Sub-Second Proof</h3>
          <p className="text-body-sm text-[var(--text-secondary)]">
            Timestamps confirmed in under 1 second. No more waiting hours for Bitcoin blocks.
          </p>
        </div>
        <div className="p-6 bg-[var(--bg-tertiary)] rounded-lg border border-[var(--border-subtle)]">
          <Shield className="w-8 h-8 text-[var(--accent-primary)] mb-4" />
          <h3 className="text-heading-md text-[var(--text-primary)] mb-2">Thermodynamic Security</h3>
          <p className="text-body-sm text-[var(--text-secondary)]">
            Security backed by real energy expenditure. Proof-of-work you can measure.
          </p>
        </div>
        <div className="p-6 bg-[var(--bg-tertiary)] rounded-lg border border-[var(--border-subtle)]">
          <GitBranch className="w-8 h-8 text-[var(--accent-primary)] mb-4" />
          <h3 className="text-heading-md text-[var(--text-primary)] mb-2">DAG-Native Ordering</h3>
          <p className="text-body-sm text-[var(--text-secondary)]">
            Prove concurrent events and causal ordering. Not just "before block N".
          </p>
        </div>
      </div>
    </div>
  );
}

export default HomePage;
