import { useParams, Link } from 'react-router-dom'
import { useEffect, useState, useRef } from 'react'
import { Download, RefreshCw, Share2, ExternalLink, Copy, CheckCircle, Clock, Loader2, Wifi, WifiOff, Wallet } from 'lucide-react'
import Button from '../components/ui/Button'
import { useStampStore } from '../stores/stamp'
import { createCalendarClient, CalendarClient } from '../api/calendar'
import type { StampResponse, WsConfirmedMessage } from '../types/api'

const CALENDAR_URL = import.meta.env.VITE_CALENDAR_URL || 'http://localhost:3001'

// ParentBlock interface removed - not currently used

function ProofPage(): JSX.Element {
  const { id } = useParams()
  const isDirectMode = id === 'direct'

  const {
    stampResponse,
    setStampResponse,
    setConfirmedProof,
    // Direct mode state
    confirmedProof: directConfirmedProof,
    blockInfo: directBlockInfo,
    transactionId: directTxId,
    hash: directHash,
    fileName: directFileName,
  } = useStampStore()

  const [error, setError] = useState<string | null>(null)
  const [response, setResponse] = useState<StampResponse | null>(() => {
    // For direct mode, build response from store state
    if (isDirectMode && directConfirmedProof && directBlockInfo) {
      return {
        id: 'direct',
        status: 'confirmed',
        submitted_at: new Date().toISOString(),
        confirmed_at: new Date(Number(directBlockInfo.timestamp)).toISOString(),
        daa_score: String(directBlockInfo.daaScore),
        blue_score: String(directBlockInfo.blueScore),
        block_hash: directBlockInfo.hash,
        tx_hash: directTxId || undefined,
        proof: directConfirmedProof,
        parent_hashes: directBlockInfo.parentHashes,
      }
    }
    return stampResponse
  })
  const [wsConnected, setWsConnected] = useState(false)

  // Keep refs stable across renders - these don't trigger re-renders
  const clientRef = useRef<CalendarClient | null>(null)
  const unsubscribeRef = useRef<(() => void) | null>(null)
  const responseRef = useRef<StampResponse | null>(response)
  const isSubscribedRef = useRef(false)

  // Update responseRef when response changes (for use in callbacks)
  responseRef.current = response

  // Subscribe to WebSocket for real-time confirmation
  // IMPORTANT: Only depend on `id` to prevent re-subscription loops
  useEffect(() => {
    // Direct mode uses store data, no API fetch needed
    if (!id || isDirectMode) return

    // Create client once
    if (!clientRef.current) {
      clientRef.current = createCalendarClient(CALENDAR_URL)
    }
    const client = clientRef.current

    let pollTimeout: ReturnType<typeof setTimeout> | null = null
    let isMounted = true

    // Handle WebSocket confirmation - uses refs to avoid stale closures
    const handleWsConfirmation = (msg: WsConfirmedMessage) => {
      if (!isMounted) return
      console.log('[KTCS] WebSocket confirmation received:', msg.proof_id)

      const confirmedResponse: StampResponse = {
        id: msg.proof_id,
        status: 'confirmed',
        submitted_at: responseRef.current?.submitted_at || new Date().toISOString(),
        confirmed_at: new Date(msg.timestamp).toISOString(),
        daa_score: msg.daa_score,
        blue_score: msg.blue_score,
        block_hash: msg.block_hash,
        tx_hash: responseRef.current?.tx_hash,
        proof: msg.proof,
      }

      setResponse(confirmedResponse)
      setStampResponse(confirmedResponse)
      setConfirmedProof(msg.proof)
      setWsConnected(false)
      isSubscribedRef.current = false
    }

    // Fetch initial stamp status
    const fetchStamp = async () => {
      if (!isMounted) return

      try {
        const data = await client.getStamp(id)
        if (!isMounted) return

        setResponse(data)
        setStampResponse(data)

        if (data.proof) {
          setConfirmedProof(data.proof)
        }

        // If confirmed, we're done - no need to subscribe or poll
        if (data.status === 'confirmed') {
          isSubscribedRef.current = false
          return
        }

        // Subscribe to WebSocket for real-time confirmation (only once)
        if (!isSubscribedRef.current && isMounted) {
          console.log('[KTCS] Subscribing to WebSocket for:', id)
          setWsConnected(true)
          isSubscribedRef.current = true
          unsubscribeRef.current = client.subscribeToConfirmation(id, handleWsConfirmation)
        }

        // Fallback polling in case WebSocket fails (less frequent: 5s)
        // Note: If we reach here, status is not 'confirmed' (early return above)
        if (isMounted) {
          pollTimeout = setTimeout(fetchStamp, 5000)
        }
      } catch (err) {
        if (!isMounted) return
        console.error('[KTCS] Failed to fetch stamp:', err)
        setError(err instanceof Error ? err.message : 'Failed to fetch stamp')

        // Retry on error with longer delay (10s to avoid hammering)
        if (isMounted) {
          pollTimeout = setTimeout(fetchStamp, 10000)
        }
      }
    }

    // Check if we already have confirmed data in store
    if (stampResponse && stampResponse.id === id && stampResponse.status === 'confirmed') {
      setResponse(stampResponse)
      return // No cleanup needed, no subscriptions made
    }

    fetchStamp()

    // Cleanup function
    return () => {
      isMounted = false
      if (pollTimeout) {
        clearTimeout(pollTimeout)
      }
      if (unsubscribeRef.current) {
        console.log('[KTCS] Unsubscribing from WebSocket for:', id)
        unsubscribeRef.current()
        unsubscribeRef.current = null
        isSubscribedRef.current = false
      }
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, isDirectMode]) // Only re-run when id changes - use refs for everything else

  // Handle missing direct proof data (e.g., after clearing localStorage)
  if (isDirectMode && !response) {
    return (
      <div className="max-w-4xl mx-auto px-6 py-12 text-center">
        <h1 className="text-display-md font-display mb-4">
          <span className="text-[var(--text-primary)]">PROOF </span>
          <span className="text-[var(--accent-primary)]">NOT FOUND</span>
        </h1>
        <p className="text-[var(--text-secondary)] mb-6">
          No direct stamp proof found. The proof data may have been cleared.
        </p>
        <Link to="/">
          <Button variant="secondary">
            Return to Home
          </Button>
        </Link>
      </div>
    )
  }

  const isPending = !response || response.status !== 'confirmed'

  // Download proof file
  const handleDownload = () => {
    if (!response?.proof) return
    const bytes = Uint8Array.from(atob(response.proof), c => c.charCodeAt(0))
    const blob = new Blob([bytes], { type: 'application/octet-stream' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    // Use original filename for direct mode, or stamp ID for calendar mode
    const baseName = isDirectMode && directFileName
      ? directFileName.replace(/\.[^/.]+$/, '') // Remove extension
      : id
    a.download = `${baseName}.kts`
    a.click()
    URL.revokeObjectURL(url)
  }

  // Compute thermodynamic values with clearer logic
  function computeBlueWork(): string {
    if (response?.thermodynamic_weight?.blue_work_at_confirmation) {
      return response.thermodynamic_weight.blue_work_at_confirmation
    }
    if (response?.blue_score) {
      const blueScoreNum = parseInt(response.blue_score, 10)
      return `${(blueScoreNum * 1e9).toExponential(2)}`
    }
    return 'calculating...'
  }

  // ~1 BTC confirmation per hour of Kaspa (36000 blocks at 10 BPS)
  function computeBtcEquivalent(): string {
    if (!response?.blue_score) return '0'
    const blueScoreNum = parseInt(response.blue_score, 10)
    return (blueScoreNum / 36000).toFixed(1)
  }

  // Use real data if available, otherwise fallback to display values
  const daaScoreNum = response?.daa_score ? parseInt(response.daa_score, 10) : 0
  const blueScoreNum = response?.blue_score ? parseInt(response.blue_score, 10) : 0

  const proof = {
    id,
    status: response?.status || 'pending',
    digest: response?.block_hash?.slice(0, 64) || 'pending...',
    algorithm: 'SHA-256',
    size: response?.proof ? atob(response.proof).length : 0,
    block: {
      hash: response?.block_hash || 'pending...',
      daaScore: daaScoreNum,
      blueScore: blueScoreNum,
      timestamp: response?.confirmed_at || new Date().toISOString(),
      parents: (response?.parent_hashes || []).map((hash) => ({
        hash,
        daaScore: response?.daa_score ? String(parseInt(response.daa_score, 10) - 1) : '0',
      })),
    },
    tx: {
      hash: response?.tx_hash || 'pending...',
      index: 0,
    },
    thermodynamic: {
      blueWork: computeBlueWork(),
      btcEquivalent: computeBtcEquivalent(),
    },
  }

  function truncateHash(hash: string): string {
    return `${hash.slice(0, 12)}...${hash.slice(-4)}`
  }

  return (
    <div className="max-w-4xl mx-auto px-6 py-12">
      {/* Header */}
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-display-md font-display mb-2">
            <span className="text-[var(--text-primary)]">TIMESTAMP </span>
            <span className="text-[var(--accent-primary)]">PROOF</span>
          </h1>
          {isDirectMode && (
            <div className="flex items-center gap-2 mt-1">
              <Wallet className="w-4 h-4 text-[var(--accent-primary)]" />
              <span className="text-sm text-[var(--text-secondary)]">Direct wallet stamp</span>
            </div>
          )}
        </div>
        {isPending ? (
          <div className="flex items-center gap-2 px-3 py-1.5 rounded bg-[var(--status-warning)]/15 border border-[var(--status-warning)]/30">
            <Clock className="w-4 h-4 text-[var(--status-warning)]" />
            <span className="text-sm font-medium text-[var(--status-warning)]">PENDING</span>
          </div>
        ) : (
          <div className="flex items-center gap-2 px-3 py-1.5 rounded bg-[var(--status-success)]/15 border border-[var(--status-success)]/30">
            <CheckCircle className="w-4 h-4 text-[var(--status-success)]" />
            <span className="text-sm font-medium text-[var(--status-success)]">CONFIRMED</span>
          </div>
        )}
      </div>

      {/* Main Grid */}
      <div className="grid grid-cols-2 gap-6 mb-8">
        {/* Document Info */}
        <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-6">
          <h2 className="text-label mb-4">DOCUMENT</h2>

          <div className="space-y-4">
            <div>
              <div className="text-label mb-1">DOCUMENT HASH</div>
              <div className="flex items-center gap-2">
                <code className="font-data text-sm text-[var(--accent-primary)] break-all">
                  {isDirectMode && directHash
                    ? truncateHash(directHash)
                    : truncateHash(proof.digest)}
                </code>
                <button
                  className="p-1 hover:text-[var(--accent-primary)] transition-colors"
                  onClick={() => navigator.clipboard.writeText(isDirectMode && directHash ? directHash : proof.digest)}
                >
                  <Copy className="w-4 h-4" />
                </button>
              </div>
            </div>

            <div>
              <div className="text-label mb-1">SIZE</div>
              <div className="text-[var(--text-primary)]">{proof.size.toLocaleString()} bytes</div>
            </div>

            <div>
              <div className="text-label mb-1">ALGORITHM</div>
              <div className="text-[var(--text-primary)]">{proof.algorithm}</div>
            </div>
          </div>
        </div>

        {/* Thermodynamic Security */}
        <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-6">
          <h2 className="text-label mb-4">THERMODYNAMIC SECURITY</h2>

          {/* Gauge placeholder */}
          <div className="flex justify-center mb-4">
            <div className="relative w-32 h-16">
              {/* Arc background */}
              <svg viewBox="0 0 100 50" className="w-full h-full">
                <path
                  d="M 10 50 A 40 40 0 0 1 90 50"
                  fill="none"
                  stroke="var(--border-default)"
                  strokeWidth="8"
                  strokeLinecap="round"
                />
                <path
                  d="M 10 50 A 40 40 0 0 1 65 15"
                  fill="none"
                  stroke="url(#thermoGradient)"
                  strokeWidth="8"
                  strokeLinecap="round"
                />
                <defs>
                  <linearGradient id="thermoGradient" x1="0%" y1="0%" x2="100%" y2="0%">
                    <stop offset="0%" stopColor="var(--thermo-cold)" />
                    <stop offset="50%" stopColor="var(--thermo-warm)" />
                    <stop offset="100%" stopColor="var(--thermo-hot)" />
                  </linearGradient>
                </defs>
              </svg>
            </div>
          </div>

          <div className="text-center">
            <div className="text-2xl font-display text-[var(--text-primary)] mb-1">
              {proof.thermodynamic.blueWork}
            </div>
            <div className="text-sm text-[var(--text-secondary)]">blue work</div>
            <div className="text-xs text-[var(--text-tertiary)] mt-2">
              ≈ {proof.thermodynamic.btcEquivalent} BTC confirmations
            </div>
          </div>
        </div>
      </div>

      {/* Block Attestation */}
      <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg mb-6">
        <div className="p-4 border-b border-[var(--border-subtle)] bg-[var(--bg-tertiary)] rounded-t-lg flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-[var(--accent-primary)]">▸</span>
            <span className="text-heading-md">BLOCK ATTESTATION</span>
          </div>
        </div>
        <div className="p-6 space-y-4">
          <div>
            <div className="text-label mb-1">BLOCK HASH</div>
            <div className="flex items-center gap-2">
              <code className="font-data text-sm text-[var(--text-primary)]">
                {truncateHash(proof.block.hash)}
              </code>
              <button className="p-1 hover:text-[var(--accent-primary)] transition-colors">
                <Copy className="w-4 h-4" />
              </button>
              <a href="#" className="p-1 hover:text-[var(--accent-primary)] transition-colors">
                <ExternalLink className="w-4 h-4" />
              </a>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <div className="text-label mb-1">DAA SCORE</div>
              <div className="text-lg font-display text-[var(--text-primary)]">
                {proof.block.daaScore.toLocaleString()}
              </div>
            </div>
            <div>
              <div className="text-label mb-1">BLUE SCORE</div>
              <div className="text-lg font-display text-[var(--text-primary)]">
                {proof.block.blueScore.toLocaleString()}
              </div>
            </div>
          </div>

          <div>
            <div className="text-label mb-1">TIMESTAMP</div>
            <div className="text-[var(--text-primary)]">
              {new Date(proof.block.timestamp).toLocaleString()} UTC
              {response?.confirmed_at && (
                <span className="text-[var(--text-tertiary)] ml-2">
                  ({Math.round((Date.now() - new Date(response.confirmed_at).getTime()) / 60000)} minutes ago)
                </span>
              )}
            </div>
          </div>

          <div>
            <div className="text-label mb-2">PARENT BLOCKS ({proof.block.parents.length})</div>
            <div className="space-y-1 font-data text-sm">
              {proof.block.parents.map((parent, i) => (
                <div key={i} className="text-[var(--text-secondary)]">
                  {i === proof.block.parents.length - 1 ? '└─' : '├─'} {parent.hash}
                  <span className="text-[var(--text-tertiary)] ml-2">daa:{parent.daaScore}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Loading Overlay - only show for calendar mode (direct stamps are always confirmed when reaching this page) */}
      {isPending && !isDirectMode && (
        <div className="mb-6 p-4 rounded-lg bg-[var(--status-warning)]/10 border border-[var(--status-warning)]/30">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Loader2 className="w-5 h-5 text-[var(--status-warning)] animate-spin" />
              <div>
                <p className="text-[var(--text-primary)]">Waiting for block confirmation...</p>
                <p className="text-sm text-[var(--text-tertiary)]">
                  This usually takes 1-10 seconds with Kaspa's 10 BPS
                </p>
              </div>
            </div>
            {/* WebSocket connection indicator */}
            <div className="flex items-center gap-1.5 px-2 py-1 rounded bg-[var(--bg-tertiary)]" title={wsConnected ? 'Real-time updates active' : 'Using polling fallback'}>
              {wsConnected ? (
                <>
                  <Wifi className="w-3.5 h-3.5 text-[var(--status-success)]" />
                  <span className="text-xs text-[var(--text-tertiary)]">Live</span>
                </>
              ) : (
                <>
                  <WifiOff className="w-3.5 h-3.5 text-[var(--text-tertiary)]" />
                  <span className="text-xs text-[var(--text-tertiary)]">Polling</span>
                </>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Error Display */}
      {error && (
        <div className="mb-6 p-4 rounded-lg bg-red-500/10 border border-red-500/30">
          <p className="text-red-400">{error}</p>
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center justify-center gap-4">
        <Button variant="secondary" onClick={handleDownload} disabled={isPending}>
          <Download className="w-4 h-4 mr-2" />
          DOWNLOAD .kts
        </Button>
        <Button variant="secondary" onClick={() => window.location.reload()}>
          <RefreshCw className="w-4 h-4 mr-2" />
          REFRESH
        </Button>
        {!isDirectMode && (
          <Button variant="secondary" onClick={() => navigator.clipboard.writeText(window.location.href)}>
            <Share2 className="w-4 h-4 mr-2" />
            COPY LINK
          </Button>
        )}
      </div>
    </div>
  )
}

export default ProofPage
