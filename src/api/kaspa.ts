/**
 * Kaspa WebSocket RPC Client
 *
 * Browser-compatible client for interacting with Kaspa nodes via wRPC.
 * Used for direct stamping functionality where users provide their own wallet.
 */

// UTXO from Kaspa node
export interface KaspaUtxo {
  transactionId: string;
  index: number;
  amount: bigint;
  // scriptPublicKey can be either a hex string directly or an object {version, scriptPublicKey}
  // depending on the RPC endpoint/version
  scriptPublicKey: string | { version: number; scriptPublicKey: string };
  blockDaaScore: bigint;
  isCoinbase: boolean;
}

// Block info from Kaspa node
export interface KaspaBlockInfo {
  hash: string;
  daaScore: bigint;
  blueScore: bigint;
  blueWork: string;
  timestamp: bigint;
  parentHashes: string[];
  isChainBlock: boolean;
}

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
}

/**
 * Browser Kaspa WebSocket RPC Client
 *
 * Connects directly to Kaspa nodes for:
 * - Fetching UTXOs
 * - Submitting transactions
 * - Getting block information
 * - Subscribing to transaction confirmations
 */
export class KaspaClient {
  private ws: WebSocket | null = null;
  private rpcUrl: string;
  // Start well above the fixed id (1) that the WASM-built raw request uses in
  // sendRawRequest, so counter-based ids from sendRequest can never collide
  // with it in pendingRequests (which would cross-route responses).
  private requestId = 1000;
  private pendingRequests: Map<number, PendingRequest> = new Map();
  private connectPromise: Promise<void> | null = null;
  private requestTimeout = 30000; // 30 seconds
  private connected = false;

  constructor(rpcUrl: string) {
    this.rpcUrl = rpcUrl;
  }

  /**
   * Connect to the Kaspa node
   */
  async connect(): Promise<void> {
    if (this.connected) return;
    if (this.connectPromise) return this.connectPromise;

    this.connectPromise = new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.connectPromise = null;
        reject(new Error('Connection timeout'));
      }, 15000);

      this.ws = new WebSocket(this.rpcUrl);

      this.ws.onopen = () => {
        clearTimeout(timeout);
        this.connected = true;
        console.log('[Kaspa] Connected to', this.rpcUrl);
        resolve();
      };

      this.ws.onerror = (error) => {
        clearTimeout(timeout);
        this.connectPromise = null;
        console.error('[Kaspa] WebSocket error:', error);
        reject(new Error('WebSocket connection failed'));
      };

      this.ws.onclose = () => {
        this.connected = false;
        this.connectPromise = null;
        console.log('[Kaspa] Connection closed');

        // Reject all pending requests
        for (const [, pending] of this.pendingRequests) {
          clearTimeout(pending.timeout);
          pending.reject(new Error('Connection closed'));
        }
        this.pendingRequests.clear();
      };

      this.ws.onmessage = (event) => {
        this.handleMessage(event.data);
      };
    });

    return this.connectPromise;
  }

  /**
   * Disconnect from the Kaspa node
   */
  disconnect(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this.connected = false;
    this.connectPromise = null;
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.connected && this.ws?.readyState === WebSocket.OPEN;
  }

  /**
   * Send an RPC request
   */
  private async sendRequest<T>(method: string, params: unknown = {}): Promise<T> {
    if (!this.isConnected()) {
      await this.connect();
    }

    return new Promise((resolve, reject) => {
      const id = ++this.requestId;

      const timeout = setTimeout(() => {
        this.pendingRequests.delete(id);
        reject(new Error(`Request timeout: ${method}`));
      }, this.requestTimeout);

      this.pendingRequests.set(id, {
        resolve: resolve as (value: unknown) => void,
        reject,
        timeout,
      });

      const request = {
        jsonrpc: '2.0',
        id,
        method,
        params,
      };

      this.ws!.send(JSON.stringify(request));
    });
  }

  /**
   * Send a raw JSON-RPC request string (no JSON parsing/building in JS)
   * This allows Rust/WASM to construct the request with proper u64 values
   */
  private async sendRawRequest<T>(jsonRequest: string): Promise<T> {
    if (!this.isConnected()) {
      await this.connect();
    }

    return new Promise((resolve, reject) => {
      // Parse the request just to get the ID
      const parsed = JSON.parse(jsonRequest);
      const id = parsed.id;

      const timeout = setTimeout(() => {
        this.pendingRequests.delete(id);
        reject(new Error(`Request timeout: ${parsed.method}`));
      }, this.requestTimeout);

      this.pendingRequests.set(id, {
        resolve: resolve as (value: unknown) => void,
        reject,
        timeout,
      });

      this.ws!.send(jsonRequest);
    });
  }

  /**
   * Handle incoming WebSocket message
   */
  private handleMessage(data: string): void {
    try {
      const response = JSON.parse(data);

      // Handle RPC response
      if (response.id !== undefined) {
        const pending = this.pendingRequests.get(response.id);
        if (pending) {
          clearTimeout(pending.timeout);
          this.pendingRequests.delete(response.id);

          if (response.error) {
            pending.reject(new Error(response.error.message || 'RPC error'));
          } else {
            // Kaspa wRPC returns result in 'params' or 'result' field
            const result = response.params ?? response.result;
            pending.resolve(result);
          }
        }
      }
    } catch (e) {
      console.error('[Kaspa] Failed to parse message:', e);
    }
  }

  // ==========================================================================
  // RPC Methods
  // ==========================================================================

  /**
   * Get UTXOs for an address
   */
  async getUtxosByAddress(address: string): Promise<KaspaUtxo[]> {
    interface RpcUtxoEntry {
      outpoint: {
        transactionId: string;
        index: number;
      };
      utxoEntry: {
        amount: string;
        // API may return scriptPublicKey as string or object depending on version
        scriptPublicKey: string | { version: number; scriptPublicKey: string };
        blockDaaScore: string;
        isCoinbase: boolean;
      };
    }

    const result = await this.sendRequest<{ entries: RpcUtxoEntry[] }>(
      'getUtxosByAddresses',
      { addresses: [address] }
    );

    return (result.entries || []).map((entry) => ({
      transactionId: entry.outpoint.transactionId,
      index: entry.outpoint.index,
      amount: BigInt(entry.utxoEntry.amount),
      scriptPublicKey: entry.utxoEntry.scriptPublicKey,
      blockDaaScore: BigInt(entry.utxoEntry.blockDaaScore),
      isCoinbase: entry.utxoEntry.isCoinbase,
    }));
  }

  /**
   * Submit a signed transaction using WASM-generated RPC request
   *
   * This avoids JavaScript's BigInt precision loss for u64::MAX sequence values
   * by having Rust construct the complete JSON-RPC request.
   *
   * @param signedTxJson - The transaction_json from WASM signTransaction
   * @param createRpcRequest - Function to create RPC request (from wasm.ts)
   */
  async submitTransactionFromWasm(
    signedTxJson: string,
    createRpcRequest: (txJson: string) => string
  ): Promise<string> {
    const jsonRequest = createRpcRequest(signedTxJson);
    const result = await this.sendRawRequest<{ transactionId: string }>(jsonRequest);
    return result.transactionId;
  }

  /**
   * Get block by hash
   */
  async getBlockByHash(hash: string): Promise<KaspaBlockInfo> {
    interface RpcBlock {
      header: {
        hash: string;
        daaScore: string;
        blueScore: string;
        blueWork: string;
        timestamp: string;
        // Parents is array of arrays: [["hash1", "hash2"], ["hash3"]]
        parents?: string[][];
      };
      verboseData?: {
        isChainBlock: boolean;
      };
    }

    const result = await this.sendRequest<{ block: RpcBlock }>(
      'getBlock',
      { hash, includeTransactions: false }
    );

    const block = result.block;
    return {
      hash: block.header.hash,
      daaScore: BigInt(block.header.daaScore),
      blueScore: BigInt(block.header.blueScore),
      blueWork: block.header.blueWork,
      timestamp: BigInt(block.header.timestamp),
      parentHashes: block.header.parents?.[0] || [],
      isChainBlock: block.verboseData?.isChainBlock ?? true,
    };
  }

  /**
   * Get current block DAG info including tip hashes
   */
  async getBlockDagInfo(): Promise<{
    networkName: string;
    virtualDaaScore: bigint;
    tipHashes: string[];
  }> {
    const result = await this.sendRequest<{
      networkName: string;
      virtualDaaScore: string;
      tipHashes: string[];
    }>('getBlockDagInfo', {});

    return {
      networkName: result.networkName,
      virtualDaaScore: BigInt(result.virtualDaaScore),
      tipHashes: result.tipHashes || [],
    };
  }

  /**
   * Get block with transaction IDs
   * Transaction IDs come from verboseData.transactionIds (not individual transactions)
   */
  async getBlockWithTransactions(hash: string): Promise<{
    hash: string;
    daaScore: bigint;
    blueScore: bigint;
    blueWork: string;
    timestamp: bigint;
    parentHashes: string[];
    transactionIds: string[];
  }> {
    interface RpcBlock {
      header: {
        hash: string;
        daaScore: string;
        blueScore: string;
        blueWork: string;
        timestamp: string;
        // Parents is array of arrays: [["hash1", "hash2"], ["hash3"]]
        parents?: string[][];
      };
      verboseData?: {
        transactionIds?: string[];
        isChainBlock?: boolean;
      };
    }

    // Use includeTransactions: false - we only need the transaction IDs from verboseData
    const result = await this.sendRequest<{ block: RpcBlock }>(
      'getBlock',
      { hash, includeTransactions: false }
    );

    const block = result.block;
    const transactionIds = block.verboseData?.transactionIds || [];
    // Flatten parents array (first level only, like calendar server)
    const parentHashes = block.header.parents?.[0] || [];

    return {
      hash: block.header.hash,
      daaScore: BigInt(block.header.daaScore),
      blueScore: BigInt(block.header.blueScore),
      blueWork: block.header.blueWork,
      timestamp: BigInt(block.header.timestamp),
      parentHashes,
      transactionIds,
    };
  }

  /**
   * Get block with full transaction data (including outputs)
   * Use this for verifying commitments in transaction outputs
   */
  async getBlockWithFullTransactions(hash: string): Promise<{
    hash: string;
    daaScore: bigint;
    blueScore: bigint;
    timestamp: bigint;
    transactionIds: string[];
    transactions: Array<{
      transactionId: string;
      outputs: Array<{
        value: bigint;
        scriptPublicKey: string;
      }>;
    }>;
  }> {
    interface RpcOutput {
      value: string;
      scriptPublicKey: string | { scriptPublicKey: string };
    }

    interface RpcTransaction {
      outputs: RpcOutput[];
      verboseData?: {
        transactionId: string;
      };
    }

    interface RpcBlock {
      header: {
        hash: string;
        daaScore: string;
        blueScore: string;
        timestamp: string;
      };
      transactions?: RpcTransaction[];
      verboseData?: {
        transactionIds?: string[];
      };
    }

    const result = await this.sendRequest<{ block: RpcBlock }>(
      'getBlock',
      { hash, includeTransactions: true }
    );

    const block = result.block;
    const transactionIds = block.verboseData?.transactionIds || [];

    // Parse transactions
    const transactions = (block.transactions || []).map(tx => {
      const txId = tx.verboseData?.transactionId || '';
      const outputs = tx.outputs.map(o => {
        // scriptPublicKey can be string or object
        const spk = typeof o.scriptPublicKey === 'string'
          ? o.scriptPublicKey
          : o.scriptPublicKey.scriptPublicKey;
        return {
          value: BigInt(o.value),
          scriptPublicKey: spk,
        };
      });
      return { transactionId: txId, outputs };
    });

    return {
      hash: block.header.hash,
      daaScore: BigInt(block.header.daaScore),
      blueScore: BigInt(block.header.blueScore),
      timestamp: BigInt(block.header.timestamp),
      transactionIds,
      transactions,
    };
  }

  /**
   * Wait for a transaction to be accepted in a block.
   *
   * Each poll seeds a breadth-first walk from the current DAG tips and descends
   * through parent generations, checking every block's verboseData transaction
   * ids for our tx. Blocks are remembered across polls (`checkedBlocks`), so the
   * walk naturally terminates when it reaches the frontier explored previously —
   * meaning newly produced blocks are always covered and a tx that has already
   * slipped a few generations below the tips between polls is still found (the
   * old "tips + one parent level" scan would miss it and time out spuriously).
   *
   * On timeout the burn is already committed on-chain, so we throw a recoverable
   * error that carries the txId — the caller must surface it so the user can
   * recover their proof rather than being left with burned funds and nothing.
   */
  async waitForTransactionAcceptance(
    txId: string,
    timeoutMs = 60000
  ): Promise<{ txId: string; blockHash: string; blockInfo: KaspaBlockInfo }> {
    console.log(`[Kaspa] Waiting for transaction ${txId} confirmation...`);

    const startTime = Date.now();
    // A relaxed interval avoids hammering the public endpoint; the cross-poll
    // frontier below means we no longer need an aggressive 100ms cadence.
    const pollInterval = 1500;
    // Safety cap on how deep to descend from the tips in a single poll. In
    // practice the walk stops earlier when it hits already-checked blocks.
    const maxGenerationsPerCycle = 30;
    const checkedBlocks = new Set<string>();

    while (Date.now() - startTime < timeoutMs) {
      try {
        const dagInfo = await this.getBlockDagInfo();

        // BFS frontier seeded from the current tips; descend into parents.
        let frontier = dagInfo.tipHashes.filter((h) => !checkedBlocks.has(h));
        let generation = 0;

        while (frontier.length > 0 && generation < maxGenerationsPerCycle) {
          const nextFrontier: string[] = [];

          for (const blockHash of frontier) {
            if (checkedBlocks.has(blockHash)) continue;

            try {
              const block = await this.getBlockWithTransactions(blockHash);
              checkedBlocks.add(blockHash);

              if (block.transactionIds.includes(txId)) {
                console.log(`[Kaspa] Transaction ${txId} confirmed in block ${blockHash} at DAA score ${block.daaScore}`);
                const blockInfo = await this.getBlockByHash(blockHash);
                return { txId, blockHash, blockInfo };
              }

              for (const parentHash of block.parentHashes) {
                if (!checkedBlocks.has(parentHash)) {
                  nextFrontier.push(parentHash);
                }
              }
            } catch (e) {
              // Mark as checked so a persistently-bad block doesn't cause a
              // tight refetch loop; it will still be reachable via other paths.
              checkedBlocks.add(blockHash);
              console.log(`[Kaspa] Block fetch error for ${blockHash.slice(0, 16)}...:`, e);
            }
          }

          frontier = nextFrontier;
          generation++;

          // Stop descending if we've run out of time mid-walk.
          if (Date.now() - startTime >= timeoutMs) break;
        }
      } catch (e) {
        console.log('[Kaspa] DAG info fetch failed, retrying...', e);
      }

      await new Promise((r) => setTimeout(r, pollInterval));
    }

    console.log(`[Kaspa] Timeout: checked ${checkedBlocks.size} blocks, tx ${txId} not found`);
    const error = new Error(
      `Transaction was submitted but not confirmed within ${Math.round(timeoutMs / 1000)}s. ` +
        `The burn may still confirm on-chain — save this transaction id to recover your proof: ${txId}`
    ) as Error & { txId: string; recoverable: boolean };
    error.txId = txId;
    error.recoverable = true;
    throw error;
  }
}

// =============================================================================
// Public Kaspa RPC Endpoints
// =============================================================================

export const KASPA_PUBLIC_ENDPOINTS = {
  mainnet: [
    { url: 'wss://kaspa.aspectron.com/wrpc/json/mainnet', name: 'Aspectron' },
    { url: 'wss://resolver.kaspa.stream/wrpc/mainnet', name: 'Kaspa Stream' },
  ],
  testnet: [
    { url: 'wss://resolver.kaspa.stream/wrpc/testnet-11', name: 'Kaspa Stream' },
  ],
} as const;

/**
 * Create a Kaspa client with the default mainnet endpoint
 */
export function createKaspaClient(
  rpcUrl: string = KASPA_PUBLIC_ENDPOINTS.mainnet[0].url
): KaspaClient {
  return new KaspaClient(rpcUrl);
}
