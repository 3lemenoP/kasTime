/**
 * KTCS Calendar Client
 *
 * Client for interacting with the KTCS calendar server.
 * Matches spec section 5.1 and 5.2.
 */

import type {
  StampRequest,
  StampResponse,
  VerifyResponse,
  HealthResponse,
  WsServerMessage,
  WsConfirmedMessage,
} from '../types';
import type { BatchMode } from '../types/proof';

/**
 * Calendar client for the KTCS server
 */
export class CalendarClient {
  private baseUrl: string;
  private wsUrl: string;
  private ws: WebSocket | null = null;
  private subscriptions: Map<string, (msg: WsConfirmedMessage) => void> = new Map();
  private reconnectAttempts = 0;
  private maxReconnectDelay = 30000; // Max 30 seconds between attempts
  private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;

  /**
   * Create a new calendar client
   * @param baseUrl - Base URL of the calendar server (e.g., 'https://calendar.example.com')
   * @param options - Client options
   */
  constructor(baseUrl: string, options?: { allowInsecure?: boolean }) {
    this.baseUrl = baseUrl.replace(/\/$/, ''); // Remove trailing slash

    // Security check: Warn if using unencrypted connections in production
    const isLocalhost = this.baseUrl.includes('localhost') || this.baseUrl.includes('127.0.0.1');
    const isSecure = this.baseUrl.startsWith('https://');

    if (!isLocalhost && !isSecure && !options?.allowInsecure) {
      console.warn(
        '[KTCS Security Warning] Using unencrypted HTTP connection to calendar server. ' +
        'This exposes timestamps and proofs to interception. ' +
        'Use HTTPS in production or set allowInsecure: true to suppress this warning.'
      );
    }

    // Construct WebSocket URL - prefer wss:// for https://
    if (this.baseUrl.startsWith('https://')) {
      this.wsUrl = this.baseUrl.replace(/^https/, 'wss') + '/v1/stream';
    } else {
      this.wsUrl = this.baseUrl.replace(/^http/, 'ws') + '/v1/stream';

      if (!isLocalhost && !options?.allowInsecure) {
        console.warn(
          '[KTCS Security Warning] Using unencrypted WebSocket connection (ws://). ' +
          'Timestamps and confirmations may be intercepted or tampered with. ' +
          'Use wss:// (via HTTPS) in production.'
        );
      }
    }
  }

  /**
   * Submit a digest for timestamping
   * @param digest - Hex-encoded SHA256 hash
   * @param mode - Batching mode (default: 'standard')
   * @returns Stamp response with ID and pending proof
   */
  async stamp(digest: string, mode: BatchMode = 'standard'): Promise<StampResponse> {
    const request: StampRequest = {
      digest,
      algorithm: 'sha256',
      batch_mode: mode,
    };

    const response = await fetch(`${this.baseUrl}/v1/stamp`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Stamp failed: ${error}`);
    }

    return response.json();
  }

  /**
   * Get the status of a stamp
   * @param id - Stamp ID (ktcs_...)
   * @returns Current stamp status and proof if confirmed
   */
  async getStamp(id: string): Promise<StampResponse> {
    const response = await fetch(`${this.baseUrl}/v1/stamp/${id}`);

    if (!response.ok) {
      if (response.status === 404) {
        throw new Error(`Stamp not found: ${id}`);
      }
      const error = await response.text();
      throw new Error(`Get stamp failed: ${error}`);
    }

    return response.json();
  }

  /**
   * Verify a proof
   * @param proofData - Binary .kts proof data
   * @returns Verification result
   */
  async verify(proofData: Uint8Array): Promise<VerifyResponse> {
    const response = await fetch(`${this.baseUrl}/v1/verify`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/octet-stream',
      },
      body: proofData,
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Verify failed: ${error}`);
    }

    return response.json();
  }

  /**
   * Check server health
   * @returns Health status
   */
  async health(): Promise<HealthResponse> {
    const response = await fetch(`${this.baseUrl}/health`);

    if (!response.ok) {
      throw new Error('Health check failed');
    }

    return response.json();
  }

  /**
   * Subscribe to confirmation events for a proof via WebSocket
   * @param proofId - Stamp ID to subscribe to
   * @param callback - Called when the proof is confirmed
   * @returns Unsubscribe function
   */
  subscribeToConfirmation(
    proofId: string,
    callback: (msg: WsConfirmedMessage) => void
  ): () => void {
    this.subscriptions.set(proofId, callback);
    this.ensureWebSocket();

    // Send subscribe message
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'subscribe', proof_id: proofId }));
    }

    return () => {
      this.subscriptions.delete(proofId);
      // Send unsubscribe message
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify({ type: 'unsubscribe', proof_id: proofId }));
      }
      if (this.subscriptions.size === 0) {
        this.closeWebSocket();
      }
    };
  }

  private ensureWebSocket() {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      return;
    }

    // Don't create new connection if one is already connecting
    if (this.ws && this.ws.readyState === WebSocket.CONNECTING) {
      return;
    }

    // Clear any pending reconnect
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }

    this.ws = new WebSocket(this.wsUrl);

    this.ws.onopen = () => {
      console.log('[KTCS] WebSocket connected');
      // Reset reconnect attempts on successful connection
      this.reconnectAttempts = 0;
      // Re-subscribe to all pending subscriptions
      for (const proofId of this.subscriptions.keys()) {
        this.ws?.send(JSON.stringify({ type: 'subscribe', proof_id: proofId }));
      }
    };

    this.ws.onmessage = (event) => {
      try {
        const msg: WsServerMessage = JSON.parse(event.data);
        if (msg.type === 'confirmed') {
          const callback = this.subscriptions.get(msg.proof_id);
          if (callback) {
            callback(msg);
            this.subscriptions.delete(msg.proof_id);
          }
        }
      } catch (e) {
        console.error('Failed to parse WebSocket message:', e);
      }
    };

    this.ws.onerror = (error) => {
      console.error('WebSocket error:', error);
    };

    this.ws.onclose = () => {
      this.ws = null;
      // Attempt reconnect with exponential backoff if there are active subscriptions
      if (this.subscriptions.size > 0) {
        this.reconnectAttempts++;
        // Exponential backoff: 1s, 2s, 4s, 8s, 16s, 30s (capped)
        const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts - 1), this.maxReconnectDelay);
        console.log(`[KTCS] WebSocket closed, reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
        this.reconnectTimeout = setTimeout(() => this.ensureWebSocket(), delay);
      }
    };
  }

  private closeWebSocket() {
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this.reconnectAttempts = 0;
  }

  /**
   * Close the client and clean up resources
   */
  close() {
    this.subscriptions.clear();
    this.closeWebSocket();
  }
}

/**
 * Create a calendar client with the default URL
 */
export function createCalendarClient(baseUrl = 'http://localhost:3001'): CalendarClient {
  return new CalendarClient(baseUrl);
}
