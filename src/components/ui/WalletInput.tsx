/**
 * Wallet Input Component
 *
 * Secure input for Kaspa wallet private keys.
 * Features:
 * - Password-style masked input
 * - Show/hide toggle
 * - Real-time validation (64 hex chars)
 * - Address derivation display
 * - Security warnings
 * - Never persists to localStorage
 */

import { useState, useCallback, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';

interface WalletInputProps {
  value: string;
  onChange: (key: string) => void;
  network: 'mainnet' | 'testnet';
  address: string | null;
  onAddressChange: (address: string | null) => void;
  disabled?: boolean;
}

export default function WalletInput({
  value,
  onChange,
  network,
  address,
  onAddressChange,
  disabled = false,
}: WalletInputProps) {
  const [showKey, setShowKey] = useState(false);
  const [isFocused, setIsFocused] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deriving, setDeriving] = useState(false);

  // Best-effort cleanup on unmount - clear the input value
  useEffect(() => {
    return () => {
      // Only runs on unmount, not on onChange changes
      onChange('');
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentionally only run on unmount
  }, []);

  // Derive address using WASM - defined before useEffect that calls it
  const deriveAddress = useCallback(
    async (key: string, net: string) => {
      setDeriving(true);
      try {
        // Dynamic import to avoid loading WASM until needed
        const wasm = await import('../../lib/wasm');
        const addr = wasm.getWalletAddress(key, net);
        onAddressChange(addr);
      } catch (e) {
        // Never log the full error - may contain key material
        console.error('Address derivation failed');
        setError('Invalid private key');
        onAddressChange(null);
      } finally {
        setDeriving(false);
      }
    },
    [onAddressChange]
  );

  // Validate and derive address when key changes
  useEffect(() => {
    if (!value) {
      setError(null);
      // Only call onAddressChange if address isn't already null to prevent infinite loops
      if (address !== null) {
        onAddressChange(null);
      }
      return;
    }

    // Validate hex format
    if (!/^[0-9a-fA-F]*$/.test(value)) {
      setError('Invalid characters (hex only)');
      if (address !== null) {
        onAddressChange(null);
      }
      return;
    }

    if (value.length < 64) {
      setError(`${64 - value.length} more characters needed`);
      if (address !== null) {
        onAddressChange(null);
      }
      return;
    }

    if (value.length > 64) {
      setError('Too many characters');
      if (address !== null) {
        onAddressChange(null);
      }
      return;
    }

    // Valid key - derive address
    setError(null);
    deriveAddress(value, network);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- address is checked but not a trigger
  }, [value, network, deriveAddress]);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    // Only allow hex characters
    const newValue = e.target.value.replace(/[^0-9a-fA-F]/g, '').toLowerCase();
    onChange(newValue);
  };

  const handlePaste = (e: React.ClipboardEvent<HTMLInputElement>) => {
    e.preventDefault();
    const pastedText = e.clipboardData.getData('text');
    // Clean pasted text - remove spaces, 0x prefix, etc.
    const cleaned = pastedText
      .replace(/^0x/i, '')
      .replace(/\s/g, '')
      .replace(/[^0-9a-fA-F]/g, '')
      .toLowerCase();
    onChange(cleaned);
  };

  const handleClear = () => {
    onChange('');
    onAddressChange(null);
  };

  const isValid = value.length === 64 && address !== null;

  return (
    <div className="space-y-3">
      {/* Security Warning */}
      <div className="flex items-start gap-2 p-3 rounded-sm bg-[var(--status-warning)]/10 border border-[var(--status-warning)]/30">
        <svg
          className="w-5 h-5 text-[var(--status-warning)] flex-shrink-0 mt-0.5"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.072 16.5c-.77.833.192 2.5 1.732 2.5z"
          />
        </svg>
        <div className="text-xs text-[var(--text-secondary)]">
          <p className="font-medium text-[var(--status-warning)]">Private Key Security</p>
          <p className="mt-1">
            Your key is processed locally and never sent to any server. Make sure you trust this
            device and connection.
          </p>
        </div>
      </div>

      {/* Input Field */}
      <div className="relative">
        <label className="block text-xs text-[var(--text-secondary)] mb-1.5 uppercase tracking-wider">
          Wallet Private Key
        </label>
        <div
          className={`
            relative flex items-center gap-2 p-3 rounded-sm
            bg-[var(--bg-tertiary)] border
            transition-all duration-200
            ${
              isFocused
                ? 'border-white'
                : error
                  ? 'border-[var(--status-error)]'
                  : isValid
                    ? 'border-[var(--status-success)]'
                    : 'border-[var(--border-default)]'
            }
            ${disabled ? 'opacity-50 cursor-not-allowed' : ''}
          `}
        >
          <input
            type={showKey ? 'text' : 'password'}
            value={value}
            onChange={handleChange}
            onPaste={handlePaste}
            onFocus={() => setIsFocused(true)}
            onBlur={() => setIsFocused(false)}
            disabled={disabled}
            placeholder="Enter 64-character hex private key"
            className={`
              flex-1 bg-transparent outline-none font-mono text-sm
              text-[var(--text-primary)] placeholder-[var(--text-tertiary)]
              ${disabled ? 'cursor-not-allowed' : ''}
            `}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck="false"
          />

          {/* Character count */}
          <span
            className={`
            text-xs font-mono
            ${value.length === 64 ? 'text-[var(--status-success)]' : 'text-[var(--text-tertiary)]'}
          `}
          >
            {value.length}/64
          </span>

          {/* Show/hide toggle */}
          <button
            type="button"
            onClick={() => setShowKey(!showKey)}
            className="p-1 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors"
            tabIndex={-1}
          >
            {showKey ? (
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21"
                />
              </svg>
            ) : (
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                />
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
                />
              </svg>
            )}
          </button>

          {/* Clear button */}
          {value && (
            <button
              type="button"
              onClick={handleClear}
              className="p-1 text-[var(--text-tertiary)] hover:text-[var(--status-error)] transition-colors"
              tabIndex={-1}
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </button>
          )}
        </div>

        {/* Error message */}
        <AnimatePresence>
          {error && (
            <motion.p
              initial={{ opacity: 0, y: -5 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -5 }}
              className="mt-1.5 text-xs text-[var(--status-error)]"
            >
              {error}
            </motion.p>
          )}
        </AnimatePresence>
      </div>

      {/* Derived Address Display */}
      <AnimatePresence>
        {(address || deriving) && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            className="overflow-hidden"
          >
            <div className="flex items-center gap-2 p-3 rounded-sm bg-[var(--bg-elevated)] border border-[var(--border-default)]">
              <div className="flex-1 min-w-0">
                <p className="text-xs text-[var(--text-tertiary)] mb-1">
                  {network === 'mainnet' ? 'Mainnet' : 'Testnet'} Address
                </p>
                {deriving ? (
                  <div className="flex items-center gap-2">
                    <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
                    <span className="text-sm text-[var(--text-secondary)]">Deriving...</span>
                  </div>
                ) : (
                  <p className="font-mono text-sm text-[var(--accent-kaspa)] truncate">
                    {address}
                  </p>
                )}
              </div>

              {/* Copy button */}
              {address && (
                <button
                  type="button"
                  onClick={() => navigator.clipboard.writeText(address)}
                  className="p-2 text-[var(--text-tertiary)] hover:text-white transition-colors"
                  title="Copy address"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                    />
                  </svg>
                </button>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
