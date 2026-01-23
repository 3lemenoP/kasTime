interface MetricCardProps {
  label: string
  value: string
  sublabel?: string
  status?: 'success' | 'warning' | 'error'
}

function MetricCard({ label, value, sublabel, status }: MetricCardProps) {
  const statusColors = {
    success: 'var(--status-success)',
    warning: 'var(--status-warning)',
    error: 'var(--status-error)',
  }

  return (
    <div className="bg-[var(--bg-secondary)] border border-[var(--border-subtle)] rounded-lg p-5">
      <div className="text-label mb-2">{label}</div>
      <div className="flex items-center gap-2">
        {status && (
          <span
            className="w-2 h-2 rounded-full animate-live-pulse"
            style={{ backgroundColor: statusColors[status] }}
          />
        )}
        <span
          className="text-xl font-display"
          style={{ color: status ? statusColors[status] : 'var(--text-primary)' }}
        >
          {value}
        </span>
      </div>
      {sublabel && (
        <div className="text-xs text-[var(--text-tertiary)] mt-1 font-data">{sublabel}</div>
      )}
    </div>
  )
}

export default MetricCard
