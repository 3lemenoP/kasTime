import { useState } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { CheckCircle, XCircle, Loader2, FileCheck, Link2, Unlink2 } from 'lucide-react'
import FileDropZone from '../components/ui/FileDropZone'
import Button from '../components/ui/Button'
import { verifyProof, parseProof, isWasmInitialized } from '../lib/wasm'
import { createKaspaClient, KASPA_PUBLIC_ENDPOINTS } from '../api/kaspa'
import { verifyOnBlockchain, calculateSecurityMetrics, type BlockchainVerificationResult } from '../lib/blockchainVerify'
import type { VerificationResult } from '../types/proof'

function VerifyPage(): JSX.Element {
  const [proofFile, setProofFile] = useState<File | null>(null)
  const [proofBytes, setProofBytes] = useState<Uint8Array | null>(null)
  const [originalFile, setOriginalFile] = useState<File | null>(null)
  const [originalBytes, setOriginalBytes] = useState<Uint8Array | null>(null)
  const [isVerifying, setIsVerifying] = useState(false)
  const [result, setResult] = useState<VerificationResult | null>(null)
  const [chainResult, setChainResult] = useState<BlockchainVerificationResult | null>(null)
  const [isVerifyingChain, setIsVerifyingChain] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleProofDrop = async (file: File) => {
    setProofFile(file)
    setResult(null)
    setError(null)
    try {
      const buffer = await file.arrayBuffer()
      const bytes = new Uint8Array(buffer)
      setProofBytes(bytes)

      if (isWasmInitialized()) {
        parseProof(bytes)
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
    setChainResult(null)
    setError(null)

    try {
      if (!isWasmInitialized()) {
        throw new Error('WASM module not initialized')
      }
      // Step 1: WASM cryptographic verification
      const verificationResult = verifyProof(proofBytes, originalBytes || undefined)
      setResult(verificationResult)

      // Step 2: Blockchain verification (if cryptographic verification passed)
      if (verificationResult.valid && verificationResult.attestations.length > 0) {
        // Find Kaspa attestations
        const kaspaAtt = verificationResult.attestations.find(
          att => att.attestationType === 'kaspa' && att.complete && att.blockHash && att.txHash
        )

        if (kaspaAtt && kaspaAtt.blockHash && kaspaAtt.txHash) {
          setIsVerifyingChain(true)
          try {
            const kaspaClient = createKaspaClient(KASPA_PUBLIC_ENDPOINTS.mainnet[0].url)
            await kaspaClient.connect()

            const blockchainResult = await verifyOnBlockchain(
              kaspaClient,
              kaspaAtt.blockHash,
              kaspaAtt.txHash,
              verificationResult.computedCommitment,
              kaspaAtt.daaScore
            )
            setChainResult(blockchainResult)

            kaspaClient.disconnect()
          } catch (chainErr) {
            console.warn('Blockchain verification failed:', chainErr)
            setChainResult({
              blockExists: false,
              transactionInBlock: false,
              commitmentVerified: false,
              error: chainErr instanceof Error ? chainErr.message : 'Connection failed',
            })
          } finally {
            setIsVerifyingChain(false)
          }
        }
      }
    } catch (err) {
      console.error('Verification error:', err)
      setError(err instanceof Error ? err.message : 'Verification failed')
    } finally {
      setIsVerifying(false)
    }
  }

  const handleReset = () => {
    setProofFile(null)
    setProofBytes(null)
    setOriginalFile(null)
    setOriginalBytes(null)
    setResult(null)
    setChainResult(null)
    setError(null)
  }

  const truncateHash = (hash: string) => `${hash.slice(0, 16)}...${hash.slice(-8)}`

  return (
    <div className="min-h-[80vh] flex flex-col items-center justify-center px-6 py-16">
      {/* Hero Title */}
      <motion.div
        className="text-center mb-12"
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
      >
        <h1 className="text-display-xl font-display tracking-tight mb-3 text-white">
          VERIFY
        </h1>
        <p className="text-body-lg text-[var(--text-secondary)]">
          Validate a timestamp proof independently
        </p>
      </motion.div>

      {/* Main Content */}
      <motion.div
        className="w-full max-w-xl"
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
      >
        <AnimatePresence mode="wait">
          {/* Result Display */}
          {result && (
            <motion.div
              key="result"
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.95 }}
              className="mb-8"
            >
              {/* Result Banner */}
              <div
                className={`rounded-sm p-8 text-center ${
                  result.valid
                    ? 'bg-[var(--status-success)]/10 border border-[var(--status-success)]/30'
                    : 'bg-[var(--status-error)]/10 border border-[var(--status-error)]/30'
                }`}
              >
                <motion.div
                  initial={{ scale: 0 }}
                  animate={{ scale: 1 }}
                  transition={{ type: 'spring', damping: 15, stiffness: 200 }}
                >
                  {result.valid ? (
                    <CheckCircle className="w-16 h-16 mx-auto mb-4 text-[var(--status-success)]" />
                  ) : (
                    <XCircle className="w-16 h-16 mx-auto mb-4 text-[var(--status-error)]" />
                  )}
                </motion.div>

                <h2 className={`text-display-md font-display mb-2 ${
                  result.valid ? 'text-[var(--status-success)]' : 'text-[var(--status-error)]'
                }`}>
                  {result.valid ? 'VALID' : 'INVALID'}
                </h2>

                {result.valid && result.digest && (
                  <p className="text-sm text-[var(--text-secondary)] mb-4">
                    Document hash verified
                  </p>
                )}

                {result.error && (
                  <p className="text-sm text-[var(--status-error)]">{result.error}</p>
                )}
              </div>

              {/* Details */}
              {result.valid && (
                <motion.div
                  className="mt-6 bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-sm p-6 space-y-4"
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ delay: 0.2 }}
                >
                  {result.digest && (
                    <div>
                      <div className="text-label mb-1">DOCUMENT DIGEST</div>
                      <code className="font-data text-sm text-[var(--accent-kaspa)] break-all">
                        {truncateHash(result.digest)}
                      </code>
                    </div>
                  )}

                  {result.attestations && result.attestations.length > 0 && (
                    <div>
                      <div className="text-label mb-2">ATTESTATIONS</div>
                      <div className="space-y-2">
                        {result.attestations.map((att, idx) => (
                          <div
                            key={idx}
                            className="flex items-center gap-2 text-sm text-[var(--text-secondary)]"
                          >
                            <span className="px-2 py-0.5 rounded-sm text-xs font-medium bg-[var(--bg-tertiary)] text-white border border-[var(--border-default)]">
                              {att.attestationType}
                            </span>
                            {att.complete && att.blockHash && (
                              <code className="font-data text-xs text-[var(--text-tertiary)]">
                                {truncateHash(att.blockHash)}
                              </code>
                            )}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Blockchain Verification Results */}
                  <div className="pt-4 border-t border-[var(--border-subtle)]">
                      <div className="text-label mb-2">BLOCKCHAIN VERIFICATION</div>
                      {isVerifyingChain ? (
                        <div className="flex items-center gap-2 text-sm text-[var(--text-secondary)]">
                          <Loader2 className="w-4 h-4 animate-spin" />
                          <span>Verifying on Kaspa blockchain...</span>
                        </div>
                      ) : chainResult ? (
                        <div className="space-y-2">
                          <div className="flex items-center gap-2 text-sm">
                            {chainResult.blockExists ? (
                              <Link2 className="w-4 h-4 text-[var(--status-success)]" />
                            ) : (
                              <Unlink2 className="w-4 h-4 text-[var(--status-error)]" />
                            )}
                            <span className={chainResult.blockExists ? 'text-[var(--status-success)]' : 'text-[var(--status-error)]'}>
                              Block {chainResult.blockExists ? 'exists' : 'not found'} on chain
                            </span>
                          </div>
                          {chainResult.blockExists && (
                            <>
                              <div className="flex items-center gap-2 text-sm">
                                {chainResult.transactionInBlock ? (
                                  <CheckCircle className="w-4 h-4 text-[var(--status-success)]" />
                                ) : (
                                  <XCircle className="w-4 h-4 text-[var(--status-error)]" />
                                )}
                                <span className={chainResult.transactionInBlock ? 'text-[var(--status-success)]' : 'text-[var(--status-error)]'}>
                                  Transaction {chainResult.transactionInBlock ? 'verified' : 'not found'} in block
                                </span>
                              </div>
                              {chainResult.transactionInBlock && (
                                <div className="flex items-center gap-2 text-sm">
                                  {chainResult.commitmentVerified ? (
                                    <CheckCircle className="w-4 h-4 text-[var(--status-success)]" />
                                  ) : (
                                    <XCircle className="w-4 h-4 text-[var(--status-error)]" />
                                  )}
                                  <span className={chainResult.commitmentVerified ? 'text-[var(--status-success)]' : 'text-[var(--status-error)]'}>
                                    Commitment {chainResult.commitmentVerified ? 'verified' : 'not found'} in transaction
                                  </span>
                                </div>
                              )}
                            </>
                          )}
                          {chainResult.blocksSince !== undefined && (
                            <div className="mt-3 pt-3 border-t border-[var(--border-subtle)]">
                              <div className="text-xs text-[var(--text-tertiary)] mb-1">THERMODYNAMIC SECURITY</div>
                              <div className="text-sm text-[var(--text-secondary)]">
                                <span className="font-data">{chainResult.blocksSince.toLocaleString()}</span> blocks since attestation
                              </div>
                              {(() => {
                                const metrics = calculateSecurityMetrics(chainResult.blocksSince)
                                return metrics.btcEquivalentConfirmations >= 0.1 ? (
                                  <div className="text-sm text-[var(--text-secondary)]">
                                    ~<span className="font-data">{metrics.btcEquivalentConfirmations.toFixed(2)}</span> BTC confirmations equivalent
                                  </div>
                                ) : null
                              })()}
                            </div>
                          )}
                          {chainResult.error && (
                            <div className="text-xs text-[var(--status-error)] mt-2">
                              {chainResult.error}
                            </div>
                          )}
                        </div>
                      ) : (
                        <div className="text-sm text-[var(--text-tertiary)]">
                          No Kaspa attestation found for chain verification
                        </div>
                      )}
                    </div>
                </motion.div>
              )}

              {/* Try Again Button */}
              <div className="mt-6 text-center">
                <Button variant="secondary" onClick={handleReset}>
                  Verify Another
                </Button>
              </div>
            </motion.div>
          )}

          {/* Verification Form */}
          {!result && (
            <motion.div
              key="form"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="space-y-6"
            >
              {/* Proof File */}
              <div>
                <div className="text-label mb-3">PROOF FILE (.kts)</div>
                {proofFile ? (
                  <div className="flex items-center gap-3 p-4 bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-sm">
                    <FileCheck className="w-5 h-5 text-[var(--status-success)]" />
                    <span className="font-data text-sm text-[var(--text-primary)] flex-1">
                      {proofFile.name}
                    </span>
                    <button
                      onClick={() => {
                        setProofFile(null)
                        setProofBytes(null)
                      }}
                      className="text-xs text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
                    >
                      Remove
                    </button>
                  </div>
                ) : (
                  <FileDropZone
                    onFileDrop={handleProofDrop}
                    accept=".kts"
                    description="Drop .kts proof file"
                  />
                )}
              </div>

              {/* Original Document (optional) */}
              <div>
                <div className="text-label mb-3">
                  ORIGINAL DOCUMENT{' '}
                  <span className="text-[var(--text-tertiary)] font-normal">(optional)</span>
                </div>
                {originalFile ? (
                  <div className="flex items-center gap-3 p-4 bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-sm">
                    <FileCheck className="w-5 h-5 text-[var(--status-success)]" />
                    <span className="font-data text-sm text-[var(--text-primary)] flex-1">
                      {originalFile.name}
                    </span>
                    <button
                      onClick={() => {
                        setOriginalFile(null)
                        setOriginalBytes(null)
                      }}
                      className="text-xs text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
                    >
                      Remove
                    </button>
                  </div>
                ) : (
                  <FileDropZone
                    onFileDrop={handleOriginalDrop}
                    description="For hash verification"
                  />
                )}
              </div>

              {/* Verify Button */}
              <Button
                onClick={handleVerify}
                disabled={!proofFile || isVerifying}
                className="w-full"
              >
                {isVerifying ? (
                  <>
                    <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                    VERIFYING...
                  </>
                ) : (
                  'VERIFY'
                )}
              </Button>

              {/* Error Display */}
              {error && (
                <motion.div
                  initial={{ opacity: 0, y: -10 }}
                  animate={{ opacity: 1, y: 0 }}
                  className="p-4 rounded-sm bg-[var(--status-error)]/10 border border-[var(--status-error)]/30"
                >
                  <div className="flex items-center gap-2">
                    <XCircle className="w-5 h-5 text-[var(--status-error)]" />
                    <span className="text-[var(--status-error)]">{error}</span>
                  </div>
                </motion.div>
              )}

              </motion.div>
          )}
        </AnimatePresence>
      </motion.div>
    </div>
  )
}

export default VerifyPage
