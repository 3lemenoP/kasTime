import { useState, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { motion, AnimatePresence } from 'framer-motion'
import { ChevronDown, ChevronUp, Loader2, CheckCircle, Wallet, Zap, Clock, Coins } from 'lucide-react'
import FileDropZone from '../components/ui/FileDropZone'
import WalletInput from '../components/ui/WalletInput'
import Button from '../components/ui/Button'
import { useStampStore } from '../stores/stamp'
import { computeSha256Hex, initWasm, isWasmInitialized } from '../lib/wasm'
import { createCalendarClient } from '../api/calendar'
import type { BatchMode } from '../types/proof'

const CALENDAR_URL = import.meta.env.VITE_CALENDAR_URL || 'http://localhost:3001'

const batchModes: { value: BatchMode; label: string; description: string; icon: typeof Zap }[] = [
  { value: 'instant', label: 'Instant', description: '~100ms', icon: Zap },
  { value: 'standard', label: 'Standard', description: '~1s', icon: Clock },
  { value: 'economic', label: 'Economic', description: '~10s', icon: Coins },
]

function HomePage(): JSX.Element {
  const navigate = useNavigate()

  const {
    mode,
    setMode,
    file,
    setFile,
    hash,
    setHash,
    setStep,
    batchMode,
    setBatchMode,
    setStampResponse,
    setError,
    error,
    walletKey,
    setWalletKey,
    walletAddress,
    network,
  } = useStampStore()

  const [showOptions, setShowOptions] = useState(false)
  const [isHashing, setIsHashing] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)

  // Handle file drop
  const handleFileDrop = useCallback(async (droppedFile: File) => {
    setFile(droppedFile)
    setIsHashing(true)
    setStep('hashing')

    try {
      // Ensure WASM is initialized
      if (!isWasmInitialized()) {
        await initWasm()
      }

      // Read file and compute hash
      const buffer = await droppedFile.arrayBuffer()
      const data = new Uint8Array(buffer)
      const hashHex = computeSha256Hex(data)

      setHash(hashHex)
      setStep('idle')
    } catch (err) {
      console.error('Hashing error:', err)
      setError(err instanceof Error ? err.message : 'Failed to hash file')
    } finally {
      setIsHashing(false)
    }
  }, [setFile, setHash, setStep, setError])

  // Handle stamp submission (calendar mode)
  const handleStamp = useCallback(async () => {
    if (!hash) return

    setIsSubmitting(true)
    setStep('submitting')

    try {
      const client = createCalendarClient(CALENDAR_URL)
      const response = await client.stamp(hash, batchMode)

      setStampResponse(response)

      // Navigate to proof page
      navigate(`/proof/${response.id}`)
    } catch (err) {
      console.error('Stamp error:', err)
      setError(err instanceof Error ? err.message : 'Failed to submit stamp')
      setStep('error')
    } finally {
      setIsSubmitting(false)
    }
  }, [hash, batchMode, setStep, setStampResponse, setError, navigate])

  // Handle wallet address derivation callback
  const handleAddressChange = useCallback((address: string | null) => {
    // This is a derived value, we just need the store to track it
    // The WalletInput component handles the derivation
    useStampStore.setState({ walletAddress: address })
  }, [])

  // Clear file selection
  const handleClear = () => {
    setFile(null)
    useStampStore.setState({ hash: null, step: 'idle', error: null })
  }

  const truncateHash = (h: string) => `${h.slice(0, 16)}...${h.slice(-8)}`

  const isCalendarMode = mode === 'calendar'
  const isDirectMode = mode === 'direct'
  const canStamp = hash && !isHashing && !isSubmitting && (isCalendarMode || (isDirectMode && walletAddress))

  return (
    <div className="min-h-[80vh] flex flex-col items-center justify-center px-6 py-16">
      {/* Hero Title */}
      <motion.div
        className="text-center mb-12"
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5 }}
      >
        <h1 className="text-display-xl font-display tracking-tight mb-3 text-white">
          TIMESTAMP
        </h1>
        <p className="text-body-lg text-[var(--text-secondary)]">
          Prove existence on Kaspa
        </p>
      </motion.div>

      {/* Main Content */}
      <motion.div
        className="w-full max-w-xl"
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, delay: 0.1 }}
      >
        {/* File Drop Zone or File Info */}
        <AnimatePresence mode="wait">
          {!file ? (
            <motion.div
              key="dropzone"
              initial={{ opacity: 0, scale: 0.98 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.98 }}
            >
              <FileDropZone
                onFileDrop={handleFileDrop}
                isLoading={isHashing}
                description="SHA-256 computed locally"
              />
            </motion.div>
          ) : (
            <motion.div
              key="fileinfo"
              className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-sm p-6"
              initial={{ opacity: 0, scale: 0.98 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.98 }}
            >
              {/* File Header */}
              <div className="flex items-center justify-between mb-4">
                <div className="flex items-center gap-3">
                  {isHashing ? (
                    <Loader2 className="w-5 h-5 text-[var(--text-secondary)] animate-spin" />
                  ) : (
                    <CheckCircle className="w-5 h-5 text-[var(--status-success)]" />
                  )}
                  <span className="text-body-lg text-[var(--text-primary)] font-medium">
                    {file.name}
                  </span>
                </div>
                <button
                  onClick={handleClear}
                  className="text-sm text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] transition-colors"
                >
                  Clear
                </button>
              </div>

              {/* Hash Display */}
              {hash && (
                <div className="mb-6">
                  <div className="text-label mb-1">SHA-256</div>
                  <code className="font-data text-sm text-[var(--accent-kaspa)]">
                    {truncateHash(hash)}
                  </code>
                </div>
              )}

              {/* Error Display */}
              {error && (
                <div className="mb-4 p-3 rounded-sm bg-[var(--status-error)]/10 border border-[var(--status-error)]/30">
                  <p className="text-sm text-[var(--status-error)]">{error}</p>
                </div>
              )}

              {/* Stamp Button */}
              <Button
                onClick={handleStamp}
                disabled={!canStamp}
                className="w-full"
              >
                {isSubmitting ? (
                  <>
                    <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                    SUBMITTING...
                  </>
                ) : (
                  'STAMP NOW'
                )}
              </Button>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Options Toggle */}
        <motion.div
          className="mt-6"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.2 }}
        >
          <button
            onClick={() => setShowOptions(!showOptions)}
            className="flex items-center gap-2 mx-auto text-sm text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] transition-colors"
          >
            {showOptions ? (
              <ChevronUp className="w-4 h-4" />
            ) : (
              <ChevronDown className="w-4 h-4" />
            )}
            Options
          </button>

          {/* Options Panel */}
          <AnimatePresence>
            {showOptions && (
              <motion.div
                className="mt-4 bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-sm p-6 space-y-6"
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: 'auto' }}
                exit={{ opacity: 0, height: 0 }}
              >
                {/* Mode Toggle */}
                <div>
                  <div className="text-label mb-3">STAMPING MODE</div>
                  <div className="flex gap-2">
                    <button
                      onClick={() => setMode('calendar')}
                      className={`flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-sm border transition-all ${
                        isCalendarMode
                          ? 'bg-white border-white text-black'
                          : 'bg-transparent border-[var(--border-default)] text-[var(--text-secondary)] hover:border-[var(--border-strong)]'
                      }`}
                    >
                      <Clock className="w-4 h-4" />
                      <span className="text-sm font-medium">Calendar</span>
                    </button>
                    <button
                      onClick={() => setMode('direct')}
                      className={`flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-sm border transition-all ${
                        isDirectMode
                          ? 'bg-white border-white text-black'
                          : 'bg-transparent border-[var(--border-default)] text-[var(--text-secondary)] hover:border-[var(--border-strong)]'
                      }`}
                    >
                      <Wallet className="w-4 h-4" />
                      <span className="text-sm font-medium">Direct</span>
                    </button>
                  </div>
                  <p className="mt-2 text-xs text-[var(--text-tertiary)]">
                    {isCalendarMode
                      ? 'Use shared calendar service (recommended)'
                      : 'Stamp directly with your own wallet'}
                  </p>
                </div>

                {/* Batch Mode (Calendar only) */}
                {isCalendarMode && (
                  <div>
                    <div className="text-label mb-3">BATCH MODE</div>
                    <div className="flex gap-2">
                      {batchModes.map(({ value, label, description, icon: Icon }) => (
                        <button
                          key={value}
                          onClick={() => setBatchMode(value)}
                          className={`flex-1 flex flex-col items-center gap-1 px-3 py-3 rounded-sm border transition-all ${
                            batchMode === value
                              ? 'bg-white border-white text-black'
                              : 'bg-transparent border-[var(--border-default)] text-[var(--text-secondary)] hover:border-[var(--border-strong)]'
                          }`}
                        >
                          <Icon className="w-4 h-4" />
                          <span className="text-sm font-medium">{label}</span>
                          <span className="text-xs opacity-70">{description}</span>
                        </button>
                      ))}
                    </div>
                  </div>
                )}

                {/* Wallet Input (Direct only) */}
                {isDirectMode && (
                  <WalletInput
                    value={walletKey || ''}
                    onChange={setWalletKey}
                    network={network}
                    address={walletAddress}
                    onAddressChange={handleAddressChange}
                  />
                )}
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      </motion.div>
    </div>
  )
}

export default HomePage
