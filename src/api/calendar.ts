/**
 * KTCS Calendar Client
 *
 * Client for interacting with the KTCS calendar server.
 * Matches spec section 5.1 and 5.2.
 */

import type {
  StampRequest,
  StampResponse,
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
  private pendingMessages: string[] = [];
  private requestTimeout = 30000; // Default 30 second timeout for API requests

  /**
   * Create a new calendar client
   * @param baseUrl - Base URL of the calendar server (e.g., 'https://calendar.example.com')
   * @param options - Client options
   */
  constructor(baseUrl: string, options?: { allowInsecure?: boolean }) {
    this.baseUrl = baseUrl.replace(/\/$/, ''); // Remove trailing slash

    // Resolve relative URLs to absolute using the current page location
    let absoluteUrl = this.baseUrl;
    if (typeof window !== 'undefined' && !this.baseUrl.startsWith('http')) {
      absoluteUrl = new URL(this.baseUrl, window.location.origin).href;
    }

    // Construct WebSocket URL from the absolute URL
    if (absoluteUrl.startsWith('https://')) {
      this.wsUrl = absoluteUrl.replace(/^https/, 'wss') + '/v1/stream';
    } else if (absoluteUrl.startsWith('http://')) {
      this.wsUrl = absoluteUrl.replace(/^http/, 'ws') + '/v1/stream';

      const isLocalhost = absoluteUrl.includes('localhost') || absoluteUrl.includes('127.0.0.1');
      if (!isLocalhost && !options?.allowInsecure) {
        console.warn(
          '[KTCS Security Warning] Using unencrypted WebSocket connection (ws://). ' +
          'Timestamps and confirmations may be intercepted or tampered with. ' +
          'Use wss:// (via HTTPS) in production.'
        );
      }
    } else {
      // Fallback for relative URLs without window context
      this.wsUrl = this.baseUrl + '/v1/stream';
    }
  }

  /**
   * Fetch with timeout support
   * @param url - URL to fetch
   * @param options - Fetch options
   * @returns Fetch response
   */
  private async fetchWithTimeout(url: string, options: RequestInit = {}): Promise<Response> {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.requestTimeout);
    try {
      const response = await fetch(url, { ...options, signal: controller.signal });
      clearTimeout(timeoutId);
      return response;
    } catch (e) {
      clearTimeout(timeoutId);
      if (e instanceof Error && e.name === 'AbortError') {
        throw new Error('Request timed out');
      }
      throw e;
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

    const response = await this.fetchWithTimeout(`${this.baseUrl}/v1/stamp`, {
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
    const response = await this.fetchWithTimeout(`${this.baseUrl}/v1/stamp/${id}`);

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
   * Subscribe to confirmation events for a proof via WebSocket.
   * IMPORTANT: Always call the returned unsubscribe function in cleanup!
   *
   * @example
   * useEffect(() => {
   *   const unsubscribe = client.subscribeToConfirmation(id, callback);
   *   return unsubscribe; // Critical for cleanup
   * }, [id]);
   *
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

    // Queue message if not yet connected
    const message = JSON.stringify({ type: 'subscribe', proof_id: proofId });
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(message);
    } else {
      this.pendingMessages.push(message);
    }

    return () => {
      this.subscriptions.delete(proofId);
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify({ type: 'unsubscribe', proof_id: proofId }));
      }
      // Close WebSocket if no more subscriptions
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

      // Flush pending messages
      for (const msg of this.pendingMessages) {
        this.ws?.send(msg);
      }
      this.pendingMessages = [];

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
        } else if (msg.type === 'batched') {
          console.log('[KTCS] Proof entered batching:', msg.proof_id);
          // Could emit an event or update state here if needed
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
        this.reconnectTimeout = setTimeout(() => {
          // Re-check if still needed before reconnecting
          if (this.subscriptions.size > 0) {
            this.ensureWebSocket();
          } else {
            this.reconnectAttempts = 0;
          }
        }, delay);
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
