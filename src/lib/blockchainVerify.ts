/**
 * Blockchain Verification Module
 *
 * Provides functions to verify KTCS attestations against the live Kaspa blockchain.
 * This enables trustless verification by confirming that:
 * 1. The block exists on the blockchain
 * 2. The transaction is included in the block
 * 3. The commitment is present in the transaction
 */

import type { KaspaClient, KaspaBlockInfo } from '../api/kaspa';

/** Result of blockchain verification */
export interface BlockchainVerificationResult {
  /** Whether the block was found on the blockchain */
  blockExists: boolean;
  /** Whether the transaction was found in the block */
  transactionInBlock: boolean;
  /** Whether the commitment was verified in the transaction */
  commitmentVerified: boolean;
  /** Block information if found */
  blockInfo?: {
    hash: string;
    daaScore: bigint;
    blueScore: bigint;
    timestamp: bigint;
  };
  /** Current DAA score of the network */
  currentDaaScore?: bigint;
  /** Number of blocks since the attestation */
  blocksSince?: bigint;
  /** Error message if verification failed */
  error?: string;
}

/**
 * Maximum allowed difference (in milliseconds) between the attestation's
 * timestamp and the on-chain block timestamp before verification fails.
 */
const TIMESTAMP_TOLERANCE_MS = 5000;

/**
 * Verify that a commitment is anchored as an EXACT P2PK "burn" output.
 *
 * A valid KTCS burn output script is exactly:
 *   0x20 <32-byte commitment> 0xac
 * i.e. the push-32 opcode (0x20), the 32-byte commitment, then OP_CHECKSIG
 * (0xac). The RPC may prepend a 2-byte (4-hex) little-endian script-version
 * prefix, which we tolerate. A bare substring match at an arbitrary offset is
 * NOT accepted — the commitment must occupy exactly this position, otherwise a
 * prover could "commit" to bytes that merely happen to appear inside some
 * unrelated output (e.g. another transaction's pubkey).
 *
 * @param expectedCommitment - The expected commitment (hex string, 32 bytes = 64 hex chars)
 * @param outputs - Transaction outputs with scriptPublicKey
 * @returns true if an output is an exact commitment burn script
 */
function verifyCommitmentInTransaction(
  expectedCommitment: string,
  outputs: Array<{ scriptPublicKey: string }>
): boolean {
  // Normalize commitment (remove 0x prefix if present)
  const commitment = expectedCommitment.toLowerCase().replace(/^0x/, '');

  // The commitment must be exactly 32 bytes to form a valid burn script.
  if (!/^[0-9a-f]{64}$/.test(commitment)) {
    return false;
  }

  // Exact P2PK burn script: 0x20 || <commitment> || 0xac
  const burnScript = `20${commitment}ac`;

  for (const output of outputs) {
    const script = output.scriptPublicKey.toLowerCase().replace(/^0x/, '');

    // Accept with no version prefix ...
    if (script === burnScript) {
      return true;
    }
    // ... or with exactly a 2-byte (4-hex) script-version prefix.
    if (script.length === burnScript.length + 4 && script.slice(4) === burnScript) {
      return true;
    }
  }

  return false;
}

/**
 * Verify an attestation against the live Kaspa blockchain
 *
 * @param client - Connected KaspaClient
 * @param blockHash - Block hash from the attestation (hex string)
 * @param txHash - Transaction hash from the attestation (hex string)
 * @param expectedCommitment - The commitment computed from proof operations (hex string)
 * @param expectedDaaScore - Optional DAA score from attestation for validation
 * @param expectedTimestamp - Optional timestamp (Unix ms) from attestation for validation
 * @returns BlockchainVerificationResult
 */
export async function verifyOnBlockchain(
  client: KaspaClient,
  blockHash: string,
  txHash: string,
  expectedCommitment: string,
  expectedDaaScore?: number,
  expectedTimestamp?: number
): Promise<BlockchainVerificationResult> {
  const result: BlockchainVerificationResult = {
    blockExists: false,
    transactionInBlock: false,
    commitmentVerified: false,
  };

  try {
    // 1. Get block by hash
    let blockInfo: KaspaBlockInfo;
    try {
      blockInfo = await client.getBlockByHash(blockHash);
      result.blockExists = true;
      result.blockInfo = {
        hash: blockInfo.hash,
        daaScore: blockInfo.daaScore,
        blueScore: blockInfo.blueScore,
        timestamp: blockInfo.timestamp,
      };

      // Validate DAA score matches if provided — a mismatch is a HARD failure,
      // not a warning: the attested DAA score must equal the on-chain block's.
      if (expectedDaaScore !== undefined && BigInt(expectedDaaScore) !== blockInfo.daaScore) {
        result.error = `DAA score mismatch: attestation claims ${expectedDaaScore}, block has ${blockInfo.daaScore}`;
        return result;
      }

      // Validate the attested timestamp against the on-chain block timestamp.
      // Both are Unix milliseconds; they should be identical, so anything beyond
      // a few seconds of drift means the proof carries a forged time — FAIL.
      if (expectedTimestamp !== undefined) {
        const drift = Math.abs(expectedTimestamp - Number(blockInfo.timestamp));
        if (drift > TIMESTAMP_TOLERANCE_MS) {
          result.error =
            `Timestamp mismatch: attestation claims ${expectedTimestamp}, ` +
            `block timestamp is ${blockInfo.timestamp} (drift ${drift}ms)`;
          return result;
        }
      }
    } catch (e) {
      result.error = `Block not found: ${blockHash}`;
      return result;
    }

    // 2. Get block with full transactions to verify tx and commitment
    try {
      const blockWithTxs = await client.getBlockWithFullTransactions(blockHash);
      const txHashLower = txHash.toLowerCase();

      // Check if transaction is in block
      result.transactionInBlock = blockWithTxs.transactionIds.some(
        id => id.toLowerCase() === txHashLower
      );

      if (!result.transactionInBlock) {
        result.error = `Transaction ${txHash} not found in block ${blockHash}`;
        return result;
      }

      // 3. Find our transaction and verify commitment in outputs
      const tx = blockWithTxs.transactions.find(
        t => t.transactionId.toLowerCase() === txHashLower
      );

      if (tx) {
        // Verify commitment is in the transaction outputs
        result.commitmentVerified = verifyCommitmentInTransaction(
          expectedCommitment,
          tx.outputs.map(o => ({ scriptPublicKey: o.scriptPublicKey }))
        );

        if (!result.commitmentVerified) {
          result.error = `Commitment not found in transaction ${txHash}`;
        }
      } else {
        // Transaction ID was in verboseData but full tx data not found
        // This can happen with coinbase transactions or if includeTransactions doesn't return all
        console.warn('Transaction data not found in block response, skipping commitment verification');
        result.commitmentVerified = false;
        result.error = 'Transaction data not available for commitment verification';
      }
    } catch (e) {
      result.error = `Failed to get block transactions: ${e}`;
      return result;
    }

    // 4. Get current chain state
    try {
      const dagInfo = await client.getBlockDagInfo();
      result.currentDaaScore = dagInfo.virtualDaaScore;
      if (result.blockInfo?.daaScore !== undefined) {
        result.blocksSince = dagInfo.virtualDaaScore - result.blockInfo.daaScore;
      }
    } catch (e) {
      console.warn('Failed to get current DAG info:', e);
    }

    return result;
  } catch (e) {
    result.error = e instanceof Error ? e.message : String(e);
    return result;
  }
}

/**
 * Calculate thermodynamic security metrics
 *
 * @param blocksSince - Number of blocks since attestation
 * @returns Equivalent security metrics
 */
export function calculateSecurityMetrics(blocksSince: bigint): {
  btcEquivalentConfirmations: number;
  securityLevel: 'low' | 'medium' | 'high' | 'very-high';
} {
  // At ~10 BPS, ~6000 blocks = ~600 seconds = ~10 minutes = ~1 BTC confirmation
  const btcEquiv = Number(blocksSince) / 6000;

  let securityLevel: 'low' | 'medium' | 'high' | 'very-high';
  if (btcEquiv < 0.5) {
    securityLevel = 'low';
  } else if (btcEquiv < 2) {
    securityLevel = 'medium';
  } else if (btcEquiv < 6) {
    securityLevel = 'high';
  } else {
    securityLevel = 'very-high';
  }

  return {
    btcEquivalentConfirmations: btcEquiv,
    securityLevel,
  };
}
