import { useState } from 'react'
import { CheckCircle, XCircle, Loader2 } from 'lucide-react'
import FileDropZone from '../components/ui/FileDropZone'
import Button from '../components/ui/Button'
import { verifyProof, parseProof, isWasmInitialized } from '../lib/wasm'
import { createCalendarClient } from '../api/calendar'
import type { VerificationResult, ProofInfo } from '../types/proof'
import type { VerifyResponse } from '../types/api'

const CALENDAR_URL = import.meta.env.VITE_CALENDAR_URL || 'http://localhost:3001'

/** Convert API verification response to client-side VerificationResult format */
function convertApiResponse(apiResult: VerifyResponse): VerificationResult {
  return {
    valid: apiResult.valid,
    digest: apiResult.digest,
    computedCommitment: apiResult.digest,
    attestations: apiResult.attestations.map(att => ({
      attestationType: att.type,
      complete: att.type !== 'pending',
      blockHash: att.block_hash,
      daaScore: att.daa_score ? parseInt(att.daa_score, 10) : undefined,
      blueScore: att.blue_score ? parseInt(att.blue_score, 10) : undefined,
    })),
    error: apiResult.error,
  }
}

function VerifyPage(): JSX.Element {
  const [proofFile, setProofFile] = useState<File | null>(null)
  const [proofBytes, setProofBytes] = useState<Uint8Array | null>(null)
  const [originalFile, setOriginalFile] = useState<File | null>(null)
  const [originalBytes, setOriginalBytes] = useState<Uint8Array | null>(null)
  const [verificationMode, setVerificationMode] = useState<'full' | 'light'>('full')
  const [isVerifying, setIsVerifying] = useState(false)
  const [result, setResult] = useState<VerificationResult | null>(null)
  const [_proofInfo, setProofInfo] = useState<ProofInfo | null>(null)
  const [error, setError] = useState<string | null>(null)

  const handleProofDrop = async (file: File) => {
    setProofFile(file)
    setResult(null)
    setError(null)
    setProofInfo(null)
    try {
      const buffer = await file.arrayBuffer()
      const bytes = new Uint8Array(buffer)
      setProofBytes(bytes)

      // Try to parse the proof for preview
      if (isWasmInitialized()) {
        const info = parseProof(bytes)
        setProofInfo(info)
      }
    } catch (err) {
      console.error('Error reading proof file:', err)
      setError('Failed to read proof file')
    }
  }

  const handleOriginalDrop = async (file: File) => {
    setOriginalFile(file)
    try {
      const buffer = await file.arrayBuffer()
      setOriginalBytes(new Uint8Array(buffer))
    } catch (err) {
      console.error('Error reading original file:', err)
    }
  }

  const handleVerify = async () => {
    if (!proofFile || !proofBytes) return

    setIsVerifying(true)
    setResult(null)
    setError(null)

    try {
      if (verificationMode === 'full') {
        // Full verification: client-side WASM (trustless)
        if (!isWasmInitialized()) {
          throw new Error('WASM module not initialized')
        }
        const verificationResult = verifyProof(proofBytes, originalBytes || undefined)
        setResult(verificationResult)
      } else {
        // Light verification: server-side via API (faster but trusts server)
        const client = createCalendarClient(CALENDAR_URL)
        const apiResult = await client.verify(proofBytes)
        setResult(convertApiResponse(apiResult))
      }
    } catch (err) {
      console.error('Verification error:', err)
      setError(err instanceof Error ? err.message : 'Verification failed')
    } finally {
      setIsVerifying(false)
    }
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
        <Button onClick={handleVerify} disabled={!proofFile || isVerifying} className="w-full">
          {isVerifying ? (
            <>
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
              VERIFYING...
            </>
          ) : (
            'VERIFY'
          )}
        </Button>

        {/* Verification Result */}
        {result && (
          <div className={`mt-6 p-4 rounded-lg border ${
            result.valid
              ? 'bg-green-500/10 border-green-500/30'
              : 'bg-red-500/10 border-red-500/30'
          }`}>
            <div className="flex items-center gap-3 mb-3">
              {result.valid ? (
                <>
                  <CheckCircle className="w-6 h-6 text-green-500" />
                  <span className="text-lg font-medium text-green-500">PROOF VALID</span>
                </>
              ) : (
                <>
                  <XCircle className="w-6 h-6 text-red-500" />
                  <span className="text-lg font-medium text-red-500">PROOF INVALID</span>
                </>
              )}
            </div>
            {result.digest && (
              <div className="mt-2">
                <span className="text-label text-xs">DIGEST</span>
                <p className="font-data text-sm text-[var(--text-secondary)] break-all">
                  {result.digest}
                </p>
              </div>
            )}
            {result.attestations && result.attestations.length > 0 && (
              <div className="mt-3">
                <span className="text-label text-xs">ATTESTATIONS</span>
                <div className="mt-1 space-y-2">
                  {result.attestations.map((att, idx) => (
                    <div key={idx} className="text-sm text-[var(--text-secondary)]">
                      <span className="text-[var(--accent-primary)]">{att.attestationType}</span>
                      {att.complete && att.blockHash && (
                        <span className="font-data text-xs ml-2 break-all">
                          Block: {att.blockHash}
                        </span>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}
            {result.error && (
              <p className="mt-2 text-sm text-red-400">{result.error}</p>
            )}
          </div>
        )}

        {/* Error Display */}
        {error && (
          <div className="mt-6 p-4 rounded-lg border bg-red-500/10 border-red-500/30">
            <div className="flex items-center gap-3">
              <XCircle className="w-5 h-5 text-red-500" />
              <span className="text-red-400">{error}</span>
            </div>
          </div>
        )}
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
