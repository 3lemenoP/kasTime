import { Outlet, NavLink } from 'react-router-dom'
import { Clock, FileCheck, Activity } from 'lucide-react'

function Layout() {
  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header className="border-b border-[var(--border-subtle)] bg-[var(--bg-secondary)]">
        <div className="max-w-7xl mx-auto px-6 h-16 flex items-center justify-between">
          {/* Logo */}
          <NavLink to="/" className="flex items-center gap-3 group">
            <div className="w-8 h-8 rounded bg-[var(--accent-primary)] flex items-center justify-center">
              <Clock className="w-5 h-5 text-[var(--text-inverse)]" />
            </div>
            <div>
              <div className="font-display text-sm font-semibold tracking-wider text-[var(--text-primary)] group-hover:text-[var(--accent-primary)] transition-colors">
                KTCS
              </div>
              <div className="text-[10px] text-[var(--text-tertiary)] tracking-wide">
                Kaspa Thermodynamic Clock
              </div>
            </div>
          </NavLink>

          {/* Navigation */}
          <nav className="flex items-center gap-1">
            <NavLink
              to="/"
              end
              className={({ isActive }) =>
                `px-4 py-2 text-sm font-medium tracking-wide transition-colors ${
                  isActive
                    ? 'text-[var(--accent-primary)]'
                    : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
                }`
              }
            >
              STAMP
            </NavLink>
            <NavLink
              to="/verify"
              className={({ isActive }) =>
                `px-4 py-2 text-sm font-medium tracking-wide transition-colors ${
                  isActive
                    ? 'text-[var(--accent-primary)]'
                    : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
                }`
              }
            >
              VERIFY
            </NavLink>
            <NavLink
              to="/network"
              className={({ isActive }) =>
                `px-4 py-2 text-sm font-medium tracking-wide transition-colors ${
                  isActive
                    ? 'text-[var(--accent-primary)]'
                    : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
                }`
              }
            >
              NETWORK
            </NavLink>

            {/* Status indicator */}
            <div className="ml-4 flex items-center gap-2 px-3 py-1.5 rounded bg-[var(--bg-tertiary)] border border-[var(--border-subtle)]">
              <span className="w-2 h-2 rounded-full bg-[var(--status-success)] animate-live-pulse" />
              <span className="text-xs text-[var(--text-tertiary)]">10 BPS</span>
            </div>
          </nav>
        </div>
      </header>

      {/* Main content */}
      <main className="flex-1">
        <Outlet />
      </main>

      {/* Footer */}
      <footer className="border-t border-[var(--border-subtle)] bg-[var(--bg-secondary)] py-4">
        <div className="max-w-7xl mx-auto px-6 flex items-center justify-between text-xs text-[var(--text-tertiary)]">
          <div>KTCS v0.1.0 - Kaspa Thermodynamic Clock Service</div>
          <div className="flex items-center gap-4">
            <a
              href="https://kaspa.org"
              target="_blank"
              rel="noopener noreferrer"
              className="hover:text-[var(--accent-primary)] transition-colors"
            >
              Kaspa Network
            </a>
            <a
              href="https://github.com"
              target="_blank"
              rel="noopener noreferrer"
              className="hover:text-[var(--accent-primary)] transition-colors"
            >
              GitHub
            </a>
          </div>
        </div>
      </footer>
    </div>
  )
}

export default Layout
