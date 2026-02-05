import { useParams, Link } from 'react-router-dom'
import { useEffect, useState, useRef } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { Download, Share2, ExternalLink, Loader2, Wifi, WifiOff } from 'lucide-react'
import Button from '../components/ui/Button'
import ConfirmationHero from '../components/proof/ConfirmationHero'
import BlockAttestation from '../components/proof/BlockAttestation'
import { useStampStore } from '../stores/stamp'
import { createCalendarClient, CalendarClient } from '../api/calendar'
import type { StampResponse, WsConfirmedMessage } from '../types/api'

const CALENDAR_URL = import.meta.env.VITE_CALENDAR_URL || 'http://localhost:3001'

function ProofPage(): JSX.Element {
  const { id } = useParams()
  const isDirectMode = id === 'direct'

  const {
    stampResponse,
    setStampResponse,
    setConfirmedProof,
    confirmedProof: directConfirmedProof,
    blockInfo: directBlockInfo,
    transactionId: directTxId,
    hash: directHash,
    fileName: directFileName,
  } = useStampStore()

  const [error, setError] = useState<string | null>(null)
  const [response, setResponse] = useState<StampResponse | null>(() => {
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
  const [showDetails, setShowDetails] = useState(false)

  const clientRef = useRef<CalendarClient | null>(null)
  const unsubscribeRef = useRef<(() => void) | null>(null)
  const responseRef = useRef<StampResponse | null>(response)
  const isSubscribedRef = useRef(false)

  responseRef.current = response

  useEffect(() => {
    if (!id || isDirectMode) return

    if (!clientRef.current) {
      clientRef.current = createCalendarClient(CALENDAR_URL)
    }
    const client = clientRef.current

    let pollTimeout: ReturnType<typeof setTimeout> | null = null
    let isMounted = true

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

        if (data.status === 'confirmed') {
          isSubscribedRef.current = false
          return
        }

        if (!isSubscribedRef.current && isMounted) {
          console.log('[KTCS] Subscribing to WebSocket for:', id)
          setWsConnected(true)
          isSubscribedRef.current = true
          unsubscribeRef.current = client.subscribeToConfirmation(id, handleWsConfirmation)
        }

        if (isMounted) {
          pollTimeout = setTimeout(fetchStamp, 5000)
        }
      } catch (err) {
        if (!isMounted) return
        console.error('[KTCS] Failed to fetch stamp:', err)
        setError(err instanceof Error ? err.message : 'Failed to fetch stamp')

        if (isMounted) {
          pollTimeout = setTimeout(fetchStamp, 10000)
        }
      }
    }

    if (stampResponse && stampResponse.id === id && stampResponse.status === 'confirmed') {
      setResponse(stampResponse)
      return
    }

    fetchStamp()

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
  }, [id, isDirectMode])

  // Handle missing direct proof data
  if (isDirectMode && !response) {
    return (
      <div className="min-h-[80vh] flex flex-col items-center justify-center px-6 py-12 text-center">
        <h1 className="text-display-lg font-display mb-4">
          <span className="text-[var(--text-primary)]">PROOF </span>
          <span className="text-[var(--accent-primary)]">NOT FOUND</span>
        </h1>
        <p className="text-body-lg text-[var(--text-secondary)] mb-8 max-w-md">
          No direct stamp proof found. The proof data may have been cleared.
        </p>
        <Link to="/">
          <Button variant="secondary">Return to Home</Button>
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
    const baseName = isDirectMode && directFileName
      ? directFileName.replace(/\.[^/.]+$/, '')
      : id
    a.download = `${baseName}.kts`
    a.click()
    URL.revokeObjectURL(url)
  }

  const daaScoreNum = response?.daa_score ? parseInt(response.daa_score, 10) : 0
  const blueScoreNum = response?.blue_score ? parseInt(response.blue_score, 10) : 0
  const documentHash = isDirectMode && directHash ? directHash : (response?.block_hash?.slice(0, 64) || '')
  const explorerUrl = response?.block_hash ? `https://explorer.kaspa.org/blocks/${response.block_hash}` : '#'

  return (
    <div className="min-h-screen">
      <AnimatePresence mode="wait">
        {isPending && !isDirectMode ? (
          // PENDING STATE
          <motion.div
            key="pending"
            className="min-h-[80vh] flex flex-col items-center justify-center px-6 py-16 bg-[var(--bg-void)]"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
          >
            {/* Subtle grid pattern */}
            <div
              className="absolute inset-0 opacity-[0.02]"
              style={{
                backgroundImage: `
                  linear-gradient(var(--text-primary) 1px, transparent 1px),
                  linear-gradient(90deg, var(--text-primary) 1px, transparent 1px)
                `,
                backgroundSize: '40px 40px',
              }}
            />

            <div className="relative z-10 text-center">
              <motion.h1
                className="text-display-xl font-display text-[var(--text-primary)] mb-2"
                animate={{ opacity: [0.5, 1, 0.5] }}
                transition={{ duration: 2, repeat: Infinity, ease: 'easeInOut' }}
              >
                AWAITING
              </motion.h1>
              <motion.h2
                className="text-display-lg font-display text-[var(--accent-primary)] mb-12"
                animate={{ opacity: [0.7, 1, 0.7] }}
                transition={{ duration: 2, repeat: Infinity, ease: 'easeInOut', delay: 0.2 }}
              >
                CONFIRMATION
              </motion.h2>

              <motion.div
                className="mb-8"
                animate={{ rotate: 360 }}
                transition={{ duration: 3, repeat: Infinity, ease: 'linear' }}
              >
                <Loader2 className="w-12 h-12 text-[var(--accent-primary)]" />
              </motion.div>

              <p className="text-body-lg text-[var(--text-secondary)] mb-2">
                Kaspa confirmation typically takes 1-10 seconds
              </p>

              {/* WebSocket indicator */}
              <div className="flex items-center justify-center gap-2 mt-6">
                {wsConnected ? (
                  <>
                    <Wifi className="w-4 h-4 text-[var(--status-success)]" />
                    <span className="text-sm text-[var(--text-tertiary)]">Real-time updates active</span>
                  </>
                ) : (
                  <>
                    <WifiOff className="w-4 h-4 text-[var(--text-tertiary)]" />
                    <span className="text-sm text-[var(--text-tertiary)]">Polling for updates</span>
                  </>
                )}
              </div>
            </div>
          </motion.div>
        ) : (
          // CONFIRMED STATE
          <motion.div
            key="confirmed"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
          >
            {/* Hero Section */}
            <ConfirmationHero
              timestamp={response?.confirmed_at ? new Date(response.confirmed_at) : new Date()}
              fileName={isDirectMode ? (directFileName || undefined) : undefined}
              hash={documentHash}
              isDirectMode={isDirectMode}
              onAnimationComplete={() => setShowDetails(true)}
            />

            {/* Details Section */}
            <motion.div
              className="max-w-3xl mx-auto px-6 py-12"
              initial={{ opacity: 0, y: 30 }}
              animate={{ opacity: showDetails || !isPending ? 1 : 0, y: showDetails || !isPending ? 0 : 30 }}
              transition={{ duration: 0.5, delay: 0.2 }}
            >
              {/* Block Attestation */}
              <BlockAttestation
                blockHash={response?.block_hash || ''}
                daaScore={daaScoreNum}
                blueScore={blueScoreNum}
                timestamp={response?.confirmed_at ? new Date(response.confirmed_at) : new Date()}
                parentHashes={response?.parent_hashes || []}
                txHash={response?.tx_hash}
                isAnimated={true}
              />

              {/* Actions */}
              <motion.div
                className="flex items-center justify-center gap-4 mt-10"
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: 0.4 }}
              >
                <Button onClick={handleDownload} disabled={isPending}>
                  <Download className="w-4 h-4 mr-2" />
                  DOWNLOAD .kts
                </Button>

                {!isDirectMode && (
                  <Button
                    variant="secondary"
                    onClick={() => navigator.clipboard.writeText(window.location.href)}
                  >
                    <Share2 className="w-4 h-4 mr-2" />
                    COPY LINK
                  </Button>
                )}

                <a href={explorerUrl} target="_blank" rel="noopener noreferrer">
                  <Button variant="secondary">
                    <ExternalLink className="w-4 h-4 mr-2" />
                    EXPLORER
                  </Button>
                </a>
              </motion.div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Error Display */}
      {error && (
        <motion.div
          className="fixed bottom-6 left-1/2 -translate-x-1/2 px-6 py-3 rounded-lg bg-[var(--status-error)]/20 border border-[var(--status-error)]/40"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
        >
          <p className="text-[var(--status-error)]">{error}</p>
        </motion.div>
      )}
    </div>
  )
}

export default ProofPage
