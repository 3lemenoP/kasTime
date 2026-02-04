# KTCS Security Model

This document describes the security properties, trust assumptions, and recommendations for the Kaspa Thermodynamic Clock Service.

## Trust Model

### Zero-Trust Verification

KTCS proofs can be verified by anyone with access to a Kaspa node. No trust in calendars, APIs, or third parties is required for verification.

**Verification is trustless because:**
1. The proof contains all information needed to verify
2. The commitment is cryptographically bound to the document hash
3. The Kaspa blockchain is publicly verifiable
4. Blue work calculations are deterministic from chain data

### What Calendars Can and Cannot Do

**Calendars CAN:**
- Delay batching (temporary denial of service)
- Refuse service entirely
- See which digests are being timestamped (unless pre-hashed)

**Calendars CANNOT:**
- Forge timestamps (requires redoing all PoW)
- Backdate proofs (DAG structure is immutable)
- Tamper with proofs (cryptographic binding)
- Steal funds (calendar wallet is separate from user funds)

**Mitigations:**
- Use multiple independent calendar servers
- Fall back to direct stamping if calendars fail
- Pre-hash sensitive data before submission

## Cryptographic Security

### Hash Functions

| Algorithm | Usage | Security Level |
|-----------|-------|----------------|
| SHA256 | Document hashing, Merkle trees | 128-bit (collision) |
| RIPEMD160 | Alternative (compatibility) | 80-bit (collision) |
| Keccak256 | Alternative (Ethereum compat) | 128-bit (collision) |

**Recommendation:** Use SHA256 for all new timestamps.

### P2PK Commitment Security

Commitments use provably unspendable P2PK outputs:

```
Script: 0x20 <32-byte commitment> 0xac
```

**Security properties:**
- No private key exists for arbitrary 32-byte values
- The commitment acts as a Schnorr public key
- OP_CHECKSIG verification will always fail
- Output is provably unspendable → funds are burned

**Burn amount:** 0.2 KAS (20,000,000 sompi) per commitment

### Merkle Tree Security

Calendar aggregation uses SHA256 Merkle trees:

- Position-dependent hashing (prepend/append distinguishes left/right)
- Standard binary Merkle tree construction
- Individual proofs verifiable without revealing other leaves

**Collision resistance:** Breaking a Merkle proof requires finding a SHA256 collision (computationally infeasible).

## Thermodynamic Security

### Blue Work Measurement

Security is measured in cumulative proof-of-work (blue work):

```
Security = Σ(block_difficulty) for all blue blocks since attestation
```

The `blue_work` field in Kaspa attestations is a 256-bit big-endian integer representing total PoW.

### Security Levels

| Time Since Stamp | Approximate Blue Work | Bitcoin Equivalent |
|------------------|----------------------|-------------------|
| 1 minute | ~10^17 | ~0.1 confirmations |
| 1 hour | ~10^18 | ~1 confirmation |
| 1 day | ~10^19 | ~6 confirmations |
| 1 week | ~10^20 | ~42 confirmations |

**Note:** These are approximate equivalences. Kaspa's security model differs from Bitcoin due to the DAG structure.

### Reorg Resistance

Kaspa's GHOSTDAG consensus provides fast finality:

- **Blue blocks** are in the selected chain (high confidence)
- **Red blocks** are outside the selected chain
- Deep reorgs require controlling majority of network hashrate
- Blue work accumulates regardless of reorg attempts

**Recommendation:** Wait for sufficient blue work accumulation based on value at risk:

| Use Case | Recommended Wait |
|----------|------------------|
| Low-value records | 1 minute |
| General documents | 10 minutes |
| Legal documents | 1 hour |
| High-value IP | 24 hours |
| Archival | + Bitcoin cross-anchor |

## Implementation Security

### Private Key Handling

**In ktcs-core:**
- Private keys use `secrecy::Secret<T>` wrapper
- Keys are zeroized on drop (memory cleared)
- No serialization of private keys in normal operations
- Constant-time signature operations

**Best practices:**
- Never pass private keys as command-line arguments
- Use `--wallet-file` or `--wallet-stdin` in CLI
- Store wallet files with restrictive permissions (chmod 600)

### API Security

**Authentication:**
- API key authentication via `X-API-Key` header
- Constant-time key comparison (prevents timing attacks)
- Minimum key length: 16 characters

**Rate limiting:**
- Per-IP rate limiting via tower-governor
- Default: 100 requests/second, burst 200
- Prevents abuse and DoS

**Request validation:**
- Maximum body size configurable (default 10MB)
- Digest format validation (64 hex characters)
- Origin validation for WebSocket connections

**CORS:**
- Configurable allowed origins
- Wildcard (`*`) only for development
- Production requires explicit origin list

### Calendar Server Hardening

**Required for production (`KTCS_ENVIRONMENT=production`):**
- `REQUIRE_API_KEY=true` must be set
- `API_KEY` must be at least 16 characters
- `KTCS_MOCK_MODE=false` (mock mode blocked)

**Recommended:**
- HTTPS with valid TLS certificate
- Reverse proxy (nginx, Caddy) in front
- Firewall restricting direct access
- Separate wallets for STAMP and RETURN
- Monitoring and alerting

### Wallet Security

**Dual-wallet architecture:**
- **STAMP wallet**: Receives funding, creates commitment TXs
- **RETURN wallet**: Receives change, recycles to STAMP

**Benefits:**
- Change never accumulates in STAMP wallet
- UTXO fragmentation is managed automatically
- Wallet keys can be rotated independently

**Key storage:**
- Store wallet keys encrypted at rest
- Use hardware security modules (HSM) for high-value deployments
- Implement key rotation procedures

## Privacy Considerations

### On-Chain Data

**What's public:**
- The 32-byte commitment (in P2PK output)
- Transaction timing and block placement
- Calendar server address (from funding patterns)

**What's private:**
- Original document content (only hash is derived)
- The nonce used to create commitment
- Relationship between commitment and document

### Privacy-Preserving Commitments

The commitment scheme hides the document hash:

```
commitment = SHA256(nonce || document_hash)
```

- **nonce**: 16 random bytes (128-bit entropy)
- Without the nonce, commitment cannot be linked to document
- Nonce stored only in the proof file

**For maximum privacy:**
1. Pre-hash sensitive documents locally
2. Use fresh random nonces
3. Store proof files securely
4. Don't submit recognizable patterns

## Attack Scenarios

### 1. Timestamp Forgery

**Attack:** Create a proof for data that didn't exist at the claimed time.

**Defense:** Impossible without redoing all proof-of-work since the alleged timestamp. Cost scales with network hashrate × time elapsed.

### 2. Calendar Compromise

**Attack:** Attacker controls calendar server.

**Impact:**
- Can delay/deny service
- Cannot forge timestamps
- Cannot backdate proofs

**Mitigation:**
- Use multiple calendars
- Direct stamping fallback
- Calendar reputation systems

### 3. Reorg Attack

**Attack:** Reorganize the blockchain to remove a timestamp.

**Defense:**
- Wait for sufficient blue work
- Kaspa's GHOSTDAG provides fast finality
- Deep reorgs require majority hashrate
- Cost increases exponentially with depth

### 4. Timing Attack on API Key

**Attack:** Measure response time to guess API key.

**Defense:** Constant-time comparison using `subtle::ConstantTimeEq`.

### 5. WebSocket Origin Bypass

**Attack:** Connect from unauthorized origin.

**Defense:** Origin header validation before WebSocket upgrade.

## Security Checklist

### For Users

- [ ] Keep .kts proof files - they're your evidence
- [ ] Verify proofs independently with your own node
- [ ] Use direct stamping for high-value documents
- [ ] Wait for sufficient blue work before relying on timestamp
- [ ] Store wallet keys securely (encrypted, restrictive permissions)
- [ ] Back up proof files in multiple locations

### For Calendar Operators

- [ ] Enable API key authentication (`REQUIRE_API_KEY=true`)
- [ ] Use strong API keys (32+ characters, random)
- [ ] Configure HTTPS with valid certificates
- [ ] Restrict CORS to known origins
- [ ] Set up rate limiting appropriate to your use case
- [ ] Use separate STAMP and RETURN wallets
- [ ] Monitor wallet balances and transaction success
- [ ] Implement logging and alerting
- [ ] Regular security audits
- [ ] Incident response plan

### For Developers

- [ ] Never trust calendar responses - always verify proofs
- [ ] Use WASM for client-side verification when possible
- [ ] Hash files client-side (never upload file contents)
- [ ] Implement proper error handling
- [ ] Validate all inputs
- [ ] Keep dependencies updated
- [ ] Follow secure coding practices

## Reporting Security Issues

If you discover a security vulnerability:

1. **Do NOT** disclose publicly
2. Email details to [security contact]
3. Include:
   - Description of vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

## See Also

- [Architecture](architecture.md)
- [Proof Format](proof-format.md)
- [Deployment Guide](../deployment/production.md)
