import { useParams } from 'react-router-dom'
import { Download, RefreshCw, Share2, ExternalLink, Copy, CheckCircle } from 'lucide-react'
import Button from '../components/ui/Button'

function ProofPage() {
  const { id } = useParams()

  // Mock proof data
  const proof = {
    id,
    status: 'confirmed',
    digest: 'abc123def456789012345678901234567890123456789012345678901234',
    algorithm: 'SHA-256',
    size: 2847,
    block: {
      hash: 'fedcba9876543210fedcba9876543210fedcba9876543210fedcba98765432',
      daaScore: 42847291,
      blueScore: 42501832,
      timestamp: '2026-01-23T14:32:01.847Z',
      parents: [
        { hash: 'abc123...', daaScore: 42847290 },
        { hash: 'def456...', daaScore: 42847290 },
        { hash: '789abc...', daaScore: 42847289 },
      ],
    },
    tx: {
      hash: '123fed456abc789def012345678901234567890123456789012345678901234',
      index: 0,
    },
    thermodynamic: {
      blueWork: '2.47e17',
      btcEquivalent: 3.2,
    },
  }

  const truncateHash = (hash: string) => `${hash.slice(0, 12)}...${hash.slice(-4)}`

  return (
    <div className="max-w-4xl mx-auto px-6 py-12">
      {/* Header */}
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-display-md font-display mb-2">
            <span className="text-[var(--text-primary)]">TIMESTAMP </span>
            <span className="text-[var(--accent-primary)]">PROOF</span>
          </h1>
        </div>
        <div className="flex items-center gap-2 px-3 py-1.5 rounded bg-[var(--status-success)]/15 border border-[var(--status-success)]/30">
          <CheckCircle className="w-4 h-4 text-[var(--status-success)]" />
          <span className="text-sm font-medium text-[var(--status-success)]">CONFIRMED</span>
        </div>
      </div>

      {/* Main Grid */}
      <div className="grid grid-cols-2 gap-6 mb-8">
        {/* Document Info */}
        <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-6">
          <h2 className="text-label mb-4">DOCUMENT</h2>

          <div className="space-y-4">
            <div>
              <div className="text-label mb-1">HASH</div>
              <div className="flex items-center gap-2">
                <code className="font-data text-sm text-[var(--accent-primary)] break-all">
                  {truncateHash(proof.digest)}
                </code>
                <button className="p-1 hover:text-[var(--accent-primary)] transition-colors">
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
              <span className="text-[var(--text-tertiary)] ml-2">(42 minutes ago)</span>
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

      {/* Actions */}
      <div className="flex items-center justify-center gap-4">
        <Button variant="secondary">
          <Download className="w-4 h-4 mr-2" />
          DOWNLOAD .kts
        </Button>
        <Button variant="secondary">
          <RefreshCw className="w-4 h-4 mr-2" />
          VERIFY AGAIN
        </Button>
        <Button variant="secondary">
          <Share2 className="w-4 h-4 mr-2" />
          SHARE LINK
        </Button>
      </div>
    </div>
  )
}

export default ProofPage
