import { useState, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { ArrowRight, Shield, Zap, GitBranch } from 'lucide-react'

import FileDropZone from '../components/ui/FileDropZone'
import Button from '../components/ui/Button'
import MetricCard from '../components/ui/MetricCard'
import { computeSha256Hex, isWasmInitialized } from '../lib/wasm'
import { useStampStore } from '../stores/stamp'
import { createCalendarClient } from '../api/calendar'
import type { BatchMode } from '../types/proof'

const CALENDAR_URL = import.meta.env.VITE_CALENDAR_URL || 'http://localhost:3001'

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
}

const BATCH_MODES: BatchMode[] = ['instant', 'standard', 'economic']

interface BatchModeButtonProps {
  mode: BatchMode
  isSelected: boolean
  onClick: () => void
}

function BatchModeButton({ mode, isSelected, onClick }: BatchModeButtonProps): JSX.Element {
  const { timing } = BATCH_MODE_CONFIG[mode]
  const baseClasses = 'px-3 py-1.5 rounded text-sm font-medium transition-colors'
  const selectedClasses = 'bg-[var(--accent-primary)] text-black'
  const unselectedClasses = 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]'

  return (
    <button
      onClick={onClick}
      className={`${baseClasses} ${isSelected ? selectedClasses : unselectedClasses}`}
    >
      {mode.toUpperCase()}
      <span className="text-xs ml-1 opacity-70">({timing})</span>
    </button>
  )
}

function HomePage(): JSX.Element {
  const navigate = useNavigate()
  const [hash, setHash] = useState('')
  const [isHashing, setIsHashing] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const { setFile, setHash: setStoreHash, setStampResponse, setError: setStoreError, batchMode, setBatchMode } = useStampStore()

  const handleFileDrop = useCallback(async (file: File) => {
    setIsHashing(true)
    setError(null)
    try {
      const buffer = await file.arrayBuffer()
      const data = new Uint8Array(buffer)

      // Use WASM module for hashing if available, fallback to Web Crypto
      let hashHex: string
      if (isWasmInitialized()) {
        hashHex = computeSha256Hex(data)
        console.log('Hash computed using KTCS WASM module')
      } else {
        // Fallback to Web Crypto API
        const hashBuffer = await crypto.subtle.digest('SHA-256', buffer)
        const hashArray = Array.from(new Uint8Array(hashBuffer))
        hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('')
        console.log('Hash computed using Web Crypto API (WASM not ready)')
      }

      setHash(hashHex)
      setFile(file)
      setStoreHash(hashHex)
    } catch (err) {
      console.error('Error hashing file:', err)
      setError('Failed to hash file')
    } finally {
      setIsHashing(false)
    }
  }, [setFile, setStoreHash])

  const handleStamp = async () => {
    if (!hash) return

    setIsSubmitting(true)
    setError(null)

    try {
      const client = createCalendarClient(CALENDAR_URL)
      const response = await client.stamp(hash, batchMode)

      setStampResponse(response)

      // Navigate to the proof page
      navigate(`/proof/${response.id}`)
    } catch (err) {
      console.error('Error submitting stamp:', err)
      const errorMsg = err instanceof Error ? err.message : 'Failed to submit timestamp'
      setError(errorMsg)
      setStoreError(errorMsg)
    } finally {
      setIsSubmitting(false)
    }
  }

  return (
    <div className="max-w-4xl mx-auto px-6 py-12">
      {/* Hero Section */}
      <div className="text-center mb-12">
        <h1 className="text-display-lg font-display mb-4 tracking-tight">
          <span className="text-[var(--text-primary)]">TRUSTLESS </span>
          <span className="text-[var(--accent-primary)]">TIMESTAMPING</span>
        </h1>
        <p className="text-body-lg text-[var(--text-secondary)] max-w-2xl mx-auto">
          Prove data existed. Backed by physics. Sub-second confirmation with Kaspa's
          10 blocks per second BlockDAG.
        </p>
      </div>

      {/* Stamp Interface */}
      <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-8 mb-8">
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
            <Button onClick={handleStamp} disabled={!hash || isSubmitting}>
              {isSubmitting ? 'SUBMITTING...' : 'STAMP'}
              {!isSubmitting && <ArrowRight className="w-4 h-4 ml-2" />}
            </Button>
          </div>
          <p className="text-xs text-[var(--text-tertiary)] mt-2">
            File never leaves your device. Hash computed locally in browser.
          </p>

          {/* Batch Mode Selector */}
          <div className="mt-4 pt-4 border-t border-[var(--border-subtle)]">
            <label className="text-label block mb-2">BATCH MODE</label>
            <div className="flex gap-2">
              {BATCH_MODES.map((mode) => (
                <BatchModeButton
                  key={mode}
                  mode={mode}
                  isSelected={batchMode === mode}
                  onClick={() => setBatchMode(mode)}
                />
              ))}
            </div>
            <p className="text-xs text-[var(--text-tertiary)] mt-1">
              {BATCH_MODE_CONFIG[batchMode].description}
            </p>
          </div>

          {error && (
            <p className="text-xs text-red-500 mt-2">
              {error}
            </p>
          )}
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
        <MetricCard
          label="LAST STAMP"
          value="0.3s ago"
          sublabel="abc123..."
        />
        <MetricCard
          label="SECURITY"
          value="1.23 × 10¹⁸"
          sublabel="≈ 6 BTC confirmations"
        />
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
  )
}

export default HomePage
