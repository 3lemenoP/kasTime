import { useState } from 'react'
import { CheckCircle } from 'lucide-react'
import FileDropZone from '../components/ui/FileDropZone'
import Button from '../components/ui/Button'

function VerifyPage() {
  const [proofFile, setProofFile] = useState<File | null>(null)
  const [originalFile, setOriginalFile] = useState<File | null>(null)
  const [verificationMode, setVerificationMode] = useState<'full' | 'light'>('full')

  const handleProofDrop = (file: File) => {
    setProofFile(file)
  }

  const handleOriginalDrop = (file: File) => {
    setOriginalFile(file)
  }

  const handleVerify = () => {
    if (!proofFile) return
    // TODO: Implement verification logic
    console.log('Verifying:', proofFile.name)
  }

  return (
    <div className="max-w-3xl mx-auto px-6 py-12">
      {/* Header */}
      <div className="text-center mb-12">
        <h1 className="text-display-md font-display mb-4 tracking-wide">
          <span className="text-[var(--text-primary)]">VERIFY </span>
          <span className="text-[var(--accent-primary)]">PROOF</span>
        </h1>
        <p className="text-body-lg text-[var(--text-secondary)]">
          Validate a timestamp proof independently
        </p>
      </div>

      {/* Verification Form */}
      <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-8">
        {/* Proof File */}
        <div className="mb-8">
          <label className="text-label block mb-3">PROOF FILE (.kts)</label>
          <FileDropZone
            onFileDrop={handleProofDrop}
            accept=".kts"
            description="Drop .kts proof file"
          />
          {proofFile && (
            <div className="mt-3 flex items-center gap-2 text-sm text-[var(--status-success)]">
              <CheckCircle className="w-4 h-4" />
              <span className="font-data">{proofFile.name}</span>
            </div>
          )}
        </div>

        {/* Divider */}
        <div className="flex items-center gap-4 my-8">
          <div className="flex-1 h-px bg-[var(--border-subtle)]" />
          <span className="text-xs text-[var(--text-tertiary)]">OR</span>
          <div className="flex-1 h-px bg-[var(--border-subtle)]" />
        </div>

        {/* Original Document */}
        <div className="mb-8">
          <label className="text-label block mb-3">
            ORIGINAL DOCUMENT <span className="text-[var(--text-tertiary)]">(optional, for hash verification)</span>
          </label>
          <FileDropZone
            onFileDrop={handleOriginalDrop}
            description="Drop original document"
          />
          {originalFile && (
            <div className="mt-3 flex items-center gap-2 text-sm text-[var(--status-success)]">
              <CheckCircle className="w-4 h-4" />
              <span className="font-data">{originalFile.name}</span>
            </div>
          )}
        </div>

        {/* Verify Button */}
        <Button onClick={handleVerify} disabled={!proofFile} className="w-full">
          VERIFY
        </Button>
      </div>

      {/* Verification Modes */}
      <div className="mt-6 bg-[var(--bg-tertiary)] border border-[var(--border-subtle)] rounded-lg p-6">
        <div className="flex items-center gap-2 mb-4">
          <span className="text-[var(--status-info)]">ℹ</span>
          <span className="text-label">VERIFICATION MODES</span>
        </div>

        <div className="space-y-3">
          <label className="flex items-start gap-3 cursor-pointer group">
            <input
              type="radio"
              name="mode"
              checked={verificationMode === 'full'}
              onChange={() => setVerificationMode('full')}
              className="mt-1 accent-[var(--accent-primary)]"
            />
            <div>
              <div className="text-sm font-medium text-[var(--text-primary)] group-hover:text-[var(--accent-primary)] transition-colors">
                FULL (Kaspa node)
              </div>
              <div className="text-xs text-[var(--text-tertiary)]">
                Trustless, queries blockchain directly
              </div>
            </div>
          </label>

          <label className="flex items-start gap-3 cursor-pointer group">
            <input
              type="radio"
              name="mode"
              checked={verificationMode === 'light'}
              onChange={() => setVerificationMode('light')}
              className="mt-1 accent-[var(--accent-primary)]"
            />
            <div>
              <div className="text-sm font-medium text-[var(--text-primary)] group-hover:text-[var(--accent-primary)] transition-colors">
                LIGHT (API)
              </div>
              <div className="text-xs text-[var(--text-tertiary)]">
                Fast, trusts verification service
              </div>
            </div>
          </label>
        </div>
      </div>
    </div>
  )
}

export default VerifyPage
