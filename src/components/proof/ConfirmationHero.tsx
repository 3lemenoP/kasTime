import { motion } from 'framer-motion'
import { Wallet, Copy, CheckCircle } from 'lucide-react'
import { useState } from 'react'

interface ConfirmationHeroProps {
  timestamp: Date
  fileName?: string
  hash: string
  isDirectMode?: boolean
  onAnimationComplete?: () => void
}

function ConfirmationHero({
  timestamp,
  fileName,
  hash,
  isDirectMode = false,
  onAnimationComplete,
}: ConfirmationHeroProps): JSX.Element {
  const [copied, setCopied] = useState(false)

  const handleCopyHash = () => {
    navigator.clipboard.writeText(hash)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const formatDate = (date: Date) => {
    return date.toLocaleDateString('en-US', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
    })
  }

  const formatTime = (date: Date) => {
    return date.toLocaleTimeString('en-US', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    }) + ' UTC'
  }

  const truncateHash = (h: string) => `${h.slice(0, 16)}...${h.slice(-8)}`

  return (
    <motion.div
      className="min-h-[60vh] flex flex-col items-center justify-center px-6 py-16 bg-[var(--bg-void)] relative overflow-hidden"
      initial={{ backgroundColor: 'var(--bg-void)' }}
      animate={{ backgroundColor: ['var(--bg-void)', 'rgba(255,255,255,0.05)', 'var(--bg-void)'] }}
      transition={{ duration: 0.4, times: [0, 0.5, 1] }}
    >
      {/* Content */}
      <div className="relative text-center max-w-2xl mx-auto">
        {/* Direct mode badge */}
        {isDirectMode && (
          <motion.div
            className="inline-flex items-center gap-2 px-3 py-1.5 mb-6 rounded-sm bg-[var(--bg-tertiary)] border border-[var(--border-default)]"
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.2 }}
          >
            <Wallet className="w-4 h-4 text-white" />
            <span className="text-xs font-medium text-white tracking-wide">DIRECT WALLET</span>
          </motion.div>
        )}

        {/* CONFIRMED title */}
        <motion.h1
          className="text-hero text-[var(--text-primary)] mb-2"
          initial={{ opacity: 0, scale: 0.8, filter: 'blur(10px)' }}
          animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
          transition={{ delay: 0.3, duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
        >
          CONFIRMED
        </motion.h1>

        {/* Timestamp */}
        <motion.div
          className="mb-10"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.6, duration: 0.4 }}
        >
          <div className="text-display-lg text-[var(--text-primary)] font-display">
            {formatDate(timestamp)}
          </div>
          <div className="text-display-md text-[var(--text-secondary)] font-display mt-1">
            {formatTime(timestamp)}
          </div>
        </motion.div>

        {/* Document info */}
        <motion.div
          className="space-y-3"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 1.8, duration: 0.4 }}
          onAnimationComplete={onAnimationComplete}
        >
          {fileName && (
            <div className="text-body-lg text-[var(--text-secondary)]">
              {fileName}
            </div>
          )}

          <div className="flex items-center justify-center gap-2">
            <code className="font-data text-sm text-[var(--text-tertiary)]">
              sha256:{truncateHash(hash)}
            </code>
            <button
              onClick={handleCopyHash}
              className="p-1.5 rounded-sm hover:bg-[var(--bg-tertiary)] transition-colors"
              title="Copy full hash"
            >
              {copied ? (
                <CheckCircle className="w-4 h-4 text-[var(--status-success)]" />
              ) : (
                <Copy className="w-4 h-4 text-[var(--text-tertiary)] hover:text-white" />
              )}
            </button>
          </div>
        </motion.div>
      </div>
    </motion.div>
  )
}

export default ConfirmationHero
