import { useState, useCallback } from 'react'
import { Upload, ArrowRight, Shield, Zap, GitBranch } from 'lucide-react'
import FileDropZone from '../components/ui/FileDropZone'
import Button from '../components/ui/Button'
import MetricCard from '../components/ui/MetricCard'

function HomePage() {
  const [hash, setHash] = useState('')
  const [isHashing, setIsHashing] = useState(false)

  const handleFileDrop = useCallback(async (file: File) => {
    setIsHashing(true)
    try {
      // Compute SHA-256 hash of file
      const buffer = await file.arrayBuffer()
      const hashBuffer = await crypto.subtle.digest('SHA-256', buffer)
      const hashArray = Array.from(new Uint8Array(hashBuffer))
      const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('')
      setHash(hashHex)
    } catch (error) {
      console.error('Error hashing file:', error)
    } finally {
      setIsHashing(false)
    }
  }, [])

  const handleStamp = () => {
    if (!hash) return
    // TODO: Implement stamping logic
    console.log('Stamping hash:', hash)
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
            <Button onClick={handleStamp} disabled={!hash}>
              STAMP
              <ArrowRight className="w-4 h-4 ml-2" />
            </Button>
          </div>
          <p className="text-xs text-[var(--text-tertiary)] mt-2">
            File never leaves your device. Hash computed locally in browser.
          </p>
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
