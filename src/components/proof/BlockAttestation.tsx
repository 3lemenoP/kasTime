import { useState } from 'react'
import { motion } from 'framer-motion'
import { Copy, ExternalLink, CheckCircle, ChevronDown, ChevronRight } from 'lucide-react'

interface BlockAttestationProps {
  blockHash: string
  daaScore: number
  blueScore: number
  timestamp: Date
  parentHashes: string[]
  txHash?: string
  isAnimated?: boolean
}

function BlockAttestation({
  blockHash,
  daaScore,
  blueScore,
  timestamp,
  parentHashes,
  txHash,
  isAnimated = true,
}: BlockAttestationProps): JSX.Element {
  const [copiedField, setCopiedField] = useState<string | null>(null)
  const [showParents, setShowParents] = useState(false)

  const copyToClipboard = (text: string, field: string) => {
    navigator.clipboard.writeText(text)
    setCopiedField(field)
    setTimeout(() => setCopiedField(null), 2000)
  }

  const truncateHash = (hash: string) => `${hash.slice(0, 12)}...${hash.slice(-8)}`

  const formatTimestamp = (date: Date) => {
    // Render actual UTC so the " UTC" label is truthful (previously this used
    // the viewer's local time zone while still appending " UTC").
    return date.toLocaleString('en-US', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
      timeZone: 'UTC',
    }) + ' UTC'
  }

  const getRelativeTime = (date: Date) => {
    const seconds = Math.floor((Date.now() - date.getTime()) / 1000)
    if (seconds < 60) return `${seconds}s ago`
    const minutes = Math.floor(seconds / 60)
    if (minutes < 60) return `${minutes}m ago`
    const hours = Math.floor(minutes / 60)
    if (hours < 24) return `${hours}h ago`
    const days = Math.floor(hours / 24)
    return `${days}d ago`
  }

  const explorerUrl = `https://explorer.kaspa.org/blocks/${blockHash}`

  const containerVariants = {
    hidden: { opacity: 0, y: 20 },
    visible: {
      opacity: 1,
      y: 0,
      transition: {
        duration: 0.4,
        ease: [0.16, 1, 0.3, 1],
        staggerChildren: 0.05,
      },
    },
  }

  const itemVariants = {
    hidden: { opacity: 0, x: -10 },
    visible: { opacity: 1, x: 0 },
  }

  return (
    <motion.div
      className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-sm overflow-hidden"
      variants={containerVariants}
      initial={isAnimated ? 'hidden' : 'visible'}
      animate="visible"
    >
      {/* Header */}
      <div className="p-4 border-b border-[var(--border-subtle)] bg-[var(--bg-tertiary)]">
        <h2 className="text-heading-md text-[var(--text-primary)] tracking-wide">
          BLOCK ATTESTATION
        </h2>
      </div>

      {/* Content */}
      <div className="p-6 space-y-5">
        {/* Block Hash */}
        <motion.div variants={itemVariants}>
          <div className="text-label mb-1.5">BLOCK HASH</div>
          <div className="flex items-center gap-2">
            <code className="font-data text-sm text-[var(--text-primary)] break-all">
              {truncateHash(blockHash)}
            </code>
            <button
              onClick={() => copyToClipboard(blockHash, 'blockHash')}
              className="p-1.5 rounded-sm hover:bg-[var(--bg-tertiary)] transition-colors flex-shrink-0"
              title="Copy block hash"
            >
              {copiedField === 'blockHash' ? (
                <CheckCircle className="w-4 h-4 text-[var(--status-success)]" />
              ) : (
                <Copy className="w-4 h-4 text-[var(--text-tertiary)] hover:text-white" />
              )}
            </button>
            <a
              href={explorerUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="p-1.5 rounded-sm hover:bg-[var(--bg-tertiary)] transition-colors flex-shrink-0"
              title="View on Kaspa Explorer"
            >
              <ExternalLink className="w-4 h-4 text-[var(--text-tertiary)] hover:text-white" />
            </a>
          </div>
        </motion.div>

        {/* Transaction Hash (if available) */}
        {txHash && (
          <motion.div variants={itemVariants}>
            <div className="text-label mb-1.5">TRANSACTION</div>
            <div className="flex items-center gap-2">
              <code className="font-data text-sm text-[var(--text-secondary)]">
                {truncateHash(txHash)}
              </code>
              <button
                onClick={() => copyToClipboard(txHash, 'txHash')}
                className="p-1.5 rounded-sm hover:bg-[var(--bg-tertiary)] transition-colors flex-shrink-0"
                title="Copy transaction hash"
              >
                {copiedField === 'txHash' ? (
                  <CheckCircle className="w-4 h-4 text-[var(--status-success)]" />
                ) : (
                  <Copy className="w-4 h-4 text-[var(--text-tertiary)] hover:text-white" />
                )}
              </button>
            </div>
          </motion.div>
        )}

        {/* Scores Grid */}
        <motion.div variants={itemVariants} className="grid grid-cols-2 gap-6">
          <div>
            <div className="text-label mb-1.5">DAA SCORE</div>
            <div className="text-display-md font-data text-[var(--text-primary)]">
              {daaScore.toLocaleString()}
            </div>
          </div>
          <div>
            <div className="text-label mb-1.5">BLUE SCORE</div>
            <div className="text-display-md font-data text-[var(--text-primary)]">
              {blueScore.toLocaleString()}
            </div>
          </div>
        </motion.div>

        {/* Timestamp */}
        <motion.div variants={itemVariants}>
          <div className="text-label mb-1.5">TIMESTAMP</div>
          <div className="flex items-baseline gap-3">
            <span className="text-body-lg text-[var(--text-primary)]">
              {formatTimestamp(timestamp)}
            </span>
            <span className="text-body-sm text-[var(--text-tertiary)]">
              ({getRelativeTime(timestamp)})
            </span>
          </div>
        </motion.div>

        {/* Parent Blocks */}
        {parentHashes.length > 0 && (
          <motion.div variants={itemVariants}>
            <button
              onClick={() => setShowParents(!showParents)}
              className="flex items-center gap-2 text-label hover:text-[var(--text-secondary)] transition-colors mb-2"
            >
              {showParents ? (
                <ChevronDown className="w-4 h-4" />
              ) : (
                <ChevronRight className="w-4 h-4" />
              )}
              PARENT BLOCKS ({parentHashes.length})
            </button>

            {showParents && (
              <motion.div
                className="pl-4 border-l border-[var(--border-subtle)] space-y-1.5"
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: 'auto' }}
                exit={{ opacity: 0, height: 0 }}
              >
                {parentHashes.map((hash, index) => (
                  <div key={hash} className="flex items-center gap-2">
                    <span className="text-[var(--text-tertiary)] font-data text-xs">
                      {index === parentHashes.length - 1 ? '└─' : '├─'}
                    </span>
                    <code className="font-data text-xs text-[var(--text-secondary)]">
                      {truncateHash(hash)}
                    </code>
                    <button
                      onClick={() => copyToClipboard(hash, `parent-${index}`)}
                      className="p-1 rounded-sm hover:bg-[var(--bg-tertiary)] transition-colors"
                    >
                      {copiedField === `parent-${index}` ? (
                        <CheckCircle className="w-3 h-3 text-[var(--status-success)]" />
                      ) : (
                        <Copy className="w-3 h-3 text-[var(--text-tertiary)] hover:text-white" />
                      )}
                    </button>
                  </div>
                ))}
              </motion.div>
            )}
          </motion.div>
        )}
      </div>
    </motion.div>
  )
}

export default BlockAttestation
