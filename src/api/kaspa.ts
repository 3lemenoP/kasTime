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

// Signed transaction for submission
export interface KaspaSignedTransaction {
  version: number;
  inputs: Array<{
    previousOutpoint: {
      transactionId: string;
      index: number;
    };
    signatureScript: string; // hex
    sequence: bigint;
    sigOpCount: number;
  }>;
  outputs: Array<{
    value: bigint;
    scriptPublicKey: {
      version: number;
      scriptPublicKey: string;
    };
  }>;
  lockTime: bigint;
  subnetworkId: string;
}

// Transaction acceptance info
export interface TransactionAcceptance {
  transactionId: string;
  blockHash: string;
}

type RpcCallback = (result: unknown, error?: RpcError) => void;

interface RpcError {
  code: number;
  message: string;
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
  private requestId = 0;
  private pendingRequests: Map<number, PendingRequest> = new Map();
  private eventCallbacks: Map<string, RpcCallback[]> = new Map();
  private connectPromise: Promise<void> | null = null;
  private requestTimeout = 30000; // 30 seconds
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private connected = false;
  private connecting = false;

  constructor(rpcUrl: string) {
    this.rpcUrl = rpcUrl;
  }

  /**
   * Connect to the Kaspa node
   */
  async connect(): Promise<void> {
    if (this.connected) return;
    if (this.connectPromise) return this.connectPromise;

    this.connecting = true;
    this.connectPromise = new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.connecting = false;
        this.connectPromise = null;
        reject(new Error('Connection timeout'));
      }, 15000);

      this.ws = new WebSocket(this.rpcUrl);

      this.ws.onopen = () => {
        clearTimeout(timeout);
        this.connected = true;
        this.connecting = false;
        this.reconnectAttempts = 0;
        console.log('[Kaspa] Connected to', this.rpcUrl);
        resolve();
      };

      this.ws.onerror = (error) => {
        clearTimeout(timeout);
        this.connecting = false;
        this.connectPromise = null;
        console.error('[Kaspa] WebSocket error:', error);
        reject(new Error('WebSocket connection failed'));
      };

      this.ws.onclose = () => {
        this.connected = false;
        this.connecting = false;
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
    this.connecting = false;
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

      if (method === 'submitTransaction') {
        console.log('DEBUG: Kaspa RPC request:', JSON.stringify(request, null, 2));
      }
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

      console.log('DEBUG: Kaspa RPC raw request:', jsonRequest);
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
            console.error('DEBUG: Kaspa RPC error response:', JSON.stringify(response, null, 2));
            pending.reject(new Error(response.error.message || 'RPC error'));
          } else {
            // Kaspa wRPC returns result in 'params' or 'result' field
            const result = response.params ?? response.result;
            pending.resolve(result);
          }
        }
      }

      // Handle notification/event
      if (response.method) {
        const callbacks = this.eventCallbacks.get(response.method);
        if (callbacks) {
          for (const callback of callbacks) {
            callback(response.params);
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
   * Submit a signed transaction (legacy - may have precision issues)
   * @deprecated Use submitTransactionFromWasm instead
   */
  async submitTransaction(tx: KaspaSignedTransaction): Promise<string> {
    // Convert to Kaspa RPC format - use camelCase field names (matches Rust serde)
    const rpcTx = {
      version: tx.version,
      inputs: tx.inputs.map((inp) => ({
        previousOutpoint: {
          transactionId: inp.previousOutpoint.transactionId,
          index: inp.previousOutpoint.index,
        },
        signatureScript: inp.signatureScript,
        sequence: 0, // Must match sighash computation
        sigOpCount: inp.sigOpCount,
      })),
      outputs: tx.outputs.map((out) => ({
        value: Number(out.value),
        scriptPublicKey: {
          version: out.scriptPublicKey.version,
          script: out.scriptPublicKey.scriptPublicKey,
        },
      })),
      lockTime: Number(tx.lockTime),
      subnetworkId: tx.subnetworkId, // Keep as-is, don't manipulate
      gas: 0,
      payload: '',
      mass: 0,
    };

    const result = await this.sendRequest<{ transactionId: string }>(
      'submitTransaction',
      { transaction: rpcTx, allowOrphan: false }
    );

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
   * Get virtual chain from block
   * Used to find which block accepted a transaction
   */
  async getVirtualChainFromBlock(
    startHash: string,
    includeAcceptedTransactionIds: boolean
  ): Promise<{
    addedChainBlockHashes: string[];
    acceptedTransactionIds: TransactionAcceptance[];
  }> {
    interface RpcResult {
      addedChainBlockHashes: string[];
      acceptedTransactionIds: Array<{
        acceptingBlockHash: string;
        acceptedTransactionIds: string[];
      }>;
    }

    const result = await this.sendRequest<RpcResult>(
      'getVirtualChainFromBlock',
      { startHash, includeAcceptedTransactionIds }
    );

    const acceptedTxs: TransactionAcceptance[] = [];
    if (result.acceptedTransactionIds) {
      for (const block of result.acceptedTransactionIds) {
        for (const txId of block.acceptedTransactionIds) {
          acceptedTxs.push({
            transactionId: txId,
            blockHash: block.acceptingBlockHash,
          });
        }
      }
    }

    return {
      addedChainBlockHashes: result.addedChainBlockHashes || [],
      acceptedTransactionIds: acceptedTxs,
    };
  }

  /**
   * Subscribe to virtual chain changes
   * Returns unsubscribe function
   */
  subscribeToVirtualChainChanged(
    callback: (data: {
      addedChainBlockHashes: string[];
      acceptedTransactionIds: TransactionAcceptance[];
    }) => void
  ): () => void {
    const method = 'notifyVirtualChainChanged';

    // Add callback
    const callbacks = this.eventCallbacks.get(method) || [];
    const wrappedCallback: RpcCallback = (params) => {
      const data = params as {
        addedChainBlockHashes: string[];
        acceptedTransactionIds: Array<{
          acceptingBlockHash: string;
          acceptedTransactionIds: string[];
        }>;
      };

      const acceptedTxs: TransactionAcceptance[] = [];
      if (data.acceptedTransactionIds) {
        for (const block of data.acceptedTransactionIds) {
          for (const txId of block.acceptedTransactionIds) {
            acceptedTxs.push({
              transactionId: txId,
              blockHash: block.acceptingBlockHash,
            });
          }
        }
      }

      callback({
        addedChainBlockHashes: data.addedChainBlockHashes || [],
        acceptedTransactionIds: acceptedTxs,
      });
    };

    callbacks.push(wrappedCallback);
    this.eventCallbacks.set(method, callbacks);

    // Subscribe
    this.sendRequest('notifyVirtualChainChanged', {
      includeAcceptedTransactionIds: true,
    }).catch((e) => console.error('[Kaspa] Subscribe error:', e));

    // Return unsubscribe function
    return () => {
      const cbs = this.eventCallbacks.get(method);
      if (cbs) {
        const idx = cbs.indexOf(wrappedCallback);
        if (idx >= 0) cbs.splice(idx, 1);
      }
    };
  }

  /**
   * Wait for a transaction to be accepted in a block
   *
   * Uses the same approach as ktcs-core: poll DAG tips and check
   * verboseData.transactionIds for our transaction.
   */
  async waitForTransactionAcceptance(
    txId: string,
    timeoutMs = 60000
  ): Promise<{ txId: string; blockHash: string; blockInfo: KaspaBlockInfo }> {
    console.log(`[Kaspa] Waiting for transaction ${txId} confirmation...`);

    const startTime = Date.now();
    const pollInterval = 100; // Poll every 100ms like calendar server
    const checkedBlocks = new Set<string>();
    let pollCount = 0;

    while (Date.now() - startTime < timeoutMs) {
      pollCount++;
      try {
        // Get current DAG tips
        const dagInfo = await this.getBlockDagInfo();

        if (pollCount <= 3 || pollCount % 50 === 0) {
          console.log(`[Kaspa] Poll #${pollCount}: ${dagInfo.tipHashes.length} tips, checked ${checkedBlocks.size} blocks`);
        }

        // Check each tip block for our transaction
        for (const tipHash of dagInfo.tipHashes) {
          if (checkedBlocks.has(tipHash)) continue;

          try {
            const block = await this.getBlockWithTransactions(tipHash);
            checkedBlocks.add(tipHash);

            // Debug: log first few blocks' transaction counts
            if (checkedBlocks.size <= 5) {
              console.log(`[Kaspa] Block ${tipHash.slice(0, 16)}... has ${block.transactionIds.length} txs`);
              if (block.transactionIds.length > 0 && block.transactionIds.length <= 3) {
                console.log(`[Kaspa]   TxIds: ${block.transactionIds.join(', ')}`);
              }
            }

            // Check if our transaction is in this block
            if (block.transactionIds.includes(txId)) {
              console.log(`[Kaspa] Transaction ${txId} confirmed in block ${tipHash} at DAA score ${block.daaScore}`);
              const blockInfo = await this.getBlockByHash(tipHash);
              return { txId, blockHash: tipHash, blockInfo };
            }

            // Also check parent blocks
            for (const parentHash of block.parentHashes) {
              if (checkedBlocks.has(parentHash)) continue;

              try {
                const parentBlock = await this.getBlockWithTransactions(parentHash);
                checkedBlocks.add(parentHash);

                if (parentBlock.transactionIds.includes(txId)) {
                  console.log(`[Kaspa] Transaction ${txId} confirmed in parent block ${parentHash} at DAA score ${parentBlock.daaScore}`);
                  const blockInfo = await this.getBlockByHash(parentHash);
                  return { txId, blockHash: parentHash, blockInfo };
                }
              } catch (e) {
                console.log(`[Kaspa] Parent block fetch error:`, e);
              }
            }
          } catch (e) {
            console.log(`[Kaspa] Block fetch error for ${tipHash.slice(0, 16)}...:`, e);
          }
        }
      } catch (e) {
        console.log('[Kaspa] DAG info fetch failed, retrying...', e);
      }

      // Wait before next poll
      await new Promise((r) => setTimeout(r, pollInterval));
    }

    console.log(`[Kaspa] Timeout: checked ${checkedBlocks.size} blocks, tx ${txId} not found`);
    throw new Error(`Transaction ${txId} confirmation timeout after ${timeoutMs}ms`);
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
  rpcUrl = KASPA_PUBLIC_ENDPOINTS.mainnet[0].url
): KaspaClient {
  return new KaspaClient(rpcUrl);
}
