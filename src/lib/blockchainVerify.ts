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

/** KTCS commitment prefix in hex ("KTCS") */
const KTCS_PREFIX_HEX = '4b544353';

/**
 * Verify a commitment exists in a transaction payload or outputs
 *
 * @param expectedCommitment - The expected commitment (hex string, 32 bytes = 64 hex chars)
 * @param outputs - Transaction outputs with scriptPublicKey
 * @param payload - Optional transaction payload (hex string)
 * @returns true if commitment is found
 */
function verifyCommitmentInTransaction(
  expectedCommitment: string,
  outputs: Array<{ scriptPublicKey: string }>,
  payload?: string
): boolean {
  // Normalize commitment (remove 0x prefix if present)
  const commitment = expectedCommitment.toLowerCase().replace(/^0x/, '');

  // Check payload first (if present)
  if (payload) {
    const payloadLower = payload.toLowerCase();
    // Check for KTCS-prefixed commitment
    if (payloadLower.includes(KTCS_PREFIX_HEX + commitment)) {
      return true;
    }
    // Check for bare commitment
    if (payloadLower.includes(commitment)) {
      return true;
    }
  }

  // Check each output's script
  for (const output of outputs) {
    const script = output.scriptPublicKey.toLowerCase();
    // Check for KTCS-prefixed commitment
    if (script.includes(KTCS_PREFIX_HEX + commitment)) {
      return true;
    }
    // Check for bare commitment in script
    if (script.includes(commitment)) {
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
 * @returns BlockchainVerificationResult
 */
export async function verifyOnBlockchain(
  client: KaspaClient,
  blockHash: string,
  txHash: string,
  expectedCommitment: string,
  expectedDaaScore?: number
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

      // Validate DAA score matches if provided
      if (expectedDaaScore !== undefined && BigInt(expectedDaaScore) !== blockInfo.daaScore) {
        console.warn(
          `DAA score mismatch: attestation has ${expectedDaaScore}, block has ${blockInfo.daaScore}`
        );
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
          tx.outputs.map(o => ({ scriptPublicKey: o.scriptPublicKey })),
          undefined // payload not available from block data
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
  // At ~10 BPS, 60000 blocks = ~100 minutes = ~1 BTC confirmation
  const btcEquiv = Number(blocksSince) / 60000;

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
