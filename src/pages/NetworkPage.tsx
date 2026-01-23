import { GitBranch } from 'lucide-react'

function NetworkPage() {
  return (
    <div className="max-w-7xl mx-auto px-6 py-12">
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-display-md font-display mb-2">
          <span className="text-[var(--text-primary)]">NETWORK </span>
          <span className="text-[var(--accent-primary)]">DASHBOARD</span>
        </h1>
        <p className="text-body-md text-[var(--text-secondary)]">
          Live Kaspa network status and KTCS metrics
        </p>
      </div>

      {/* Status Grid */}
      <div className="grid grid-cols-2 gap-6 mb-8">
        {/* Kaspa Network Status */}
        <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-6">
          <div className="flex items-center justify-between mb-6">
            <h2 className="text-heading-md text-[var(--text-primary)]">KASPA NETWORK</h2>
            <div className="flex items-center gap-2">
              <span className="w-2 h-2 rounded-full bg-[var(--status-success)] animate-live-pulse" />
              <span className="text-xs text-[var(--status-success)]">OPERATIONAL</span>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4 mb-6">
            <div className="p-4 bg-[var(--bg-tertiary)] rounded border-l-2 border-[var(--accent-primary)]">
              <div className="text-label mb-1">BLOCKS/SEC</div>
              <div className="text-2xl font-display text-[var(--text-primary)]">10</div>
            </div>
            <div className="p-4 bg-[var(--bg-tertiary)] rounded border-l-2 border-[var(--accent-secondary)]">
              <div className="text-label mb-1">DAA SCORE</div>
              <div className="text-2xl font-display text-[var(--text-primary)]">42.8M</div>
            </div>
          </div>

          <div>
            <div className="text-label mb-2">BLUE WORK (24h)</div>
            <div className="h-16 bg-[var(--bg-tertiary)] rounded flex items-end px-2 pb-2 gap-1">
              {/* Placeholder chart bars */}
              {Array.from({ length: 24 }).map((_, i) => (
                <div
                  key={i}
                  className="flex-1 bg-[var(--accent-primary)] rounded-t opacity-60"
                  style={{ height: `${30 + Math.random() * 70}%` }}
                />
              ))}
            </div>
            <div className="mt-2 text-mono-md text-[var(--accent-primary)]">1.23×10¹⁸ current</div>
          </div>
        </div>

        {/* KTCS Service Status */}
        <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-6">
          <div className="flex items-center justify-between mb-6">
            <h2 className="text-heading-md text-[var(--text-primary)]">KTCS SERVICE</h2>
            <div className="flex items-center gap-2">
              <span className="w-2 h-2 rounded-full bg-[var(--status-success)] animate-live-pulse" />
              <span className="text-xs text-[var(--status-success)]">OPERATIONAL</span>
            </div>
          </div>

          <div className="grid grid-cols-3 gap-4 mb-6">
            <div className="p-4 bg-[var(--bg-tertiary)] rounded border-l-2 border-[var(--accent-primary)]">
              <div className="text-label mb-1">TODAY</div>
              <div className="text-2xl font-display text-[var(--text-primary)]">1,247</div>
            </div>
            <div className="p-4 bg-[var(--bg-tertiary)] rounded border-l-2 border-[var(--accent-secondary)]">
              <div className="text-label mb-1">AVG LATENCY</div>
              <div className="text-2xl font-display text-[var(--text-primary)]">0.3s</div>
            </div>
            <div className="p-4 bg-[var(--bg-tertiary)] rounded border-l-2 border-[var(--status-success)]">
              <div className="text-label mb-1">UPTIME</div>
              <div className="text-2xl font-display text-[var(--text-primary)]">99.9%</div>
            </div>
          </div>

          <div>
            <div className="text-label mb-3">CALENDARS ONLINE</div>
            <div className="space-y-2">
              {['alpha.ktcs.kaspa.org', 'beta.ktcs.kaspa.org', 'gamma.ktcs.kaspa.org'].map((calendar) => (
                <div key={calendar} className="flex items-center gap-2 text-sm">
                  <span className="w-2 h-2 rounded-full bg-[var(--status-success)]" />
                  <span className="font-data text-[var(--text-secondary)]">{calendar}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Live DAG View Placeholder */}
      <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-6 mb-8">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-heading-md text-[var(--text-primary)]">LIVE DAG VIEW</h2>
          <button className="text-xs text-[var(--text-tertiary)] hover:text-[var(--accent-primary)] transition-colors">
            ⛶ Fullscreen
          </button>
        </div>
        <div className="h-64 bg-[var(--bg-tertiary)] rounded flex items-center justify-center text-[var(--text-tertiary)]">
          <div className="text-center">
            <GitBranch className="w-12 h-12 mx-auto mb-4 opacity-50" />
            <p className="text-sm">DAG Visualization</p>
            <p className="text-xs opacity-60">Coming soon</p>
          </div>
        </div>
      </div>

      {/* Activity Feed */}
      <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-6">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-heading-md text-[var(--text-primary)]">RECENT ACTIVITY</h2>
          <div className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-[var(--status-success)] animate-live-pulse" />
            <span className="text-xs text-[var(--text-tertiary)]">LIVE</span>
          </div>
        </div>
        <div className="font-data text-sm space-y-2">
          {[
            { time: '14:32:01.847', type: 'STAMP', hash: 'abc123...', status: 'confirmed', daa: '42847291' },
            { time: '14:32:01.652', type: 'BLOCK', hash: 'fedcba...', status: null, daa: '42501832' },
            { time: '14:31:58.234', type: 'STAMP', hash: 'def456...', status: 'confirmed', daa: '42847288' },
            { time: '14:31:57.891', type: 'STAMP', hash: '789abc...', status: 'confirmed', daa: '42847287' },
            { time: '14:31:55.123', type: 'BLOCK', hash: '012def...', status: null, daa: '42501829' },
          ].map((item, i) => (
            <div key={i} className="flex items-center gap-4 py-2 border-b border-[var(--border-subtle)] last:border-0">
              <span className="text-[var(--text-tertiary)] w-28">{item.time}</span>
              <span className={`px-2 py-0.5 rounded text-xs ${
                item.type === 'STAMP'
                  ? 'bg-[var(--accent-primary-dim)] text-[var(--accent-primary)]'
                  : 'bg-[var(--accent-secondary-dim)] text-[var(--accent-secondary)]'
              }`}>
                {item.type}
              </span>
              <span className="text-[var(--text-secondary)] flex-1">{item.hash}</span>
              {item.status && (
                <span className="text-[var(--status-success)]">● {item.status}</span>
              )}
              <span className="text-[var(--text-tertiary)]">daa:{item.daa}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

export default NetworkPage
