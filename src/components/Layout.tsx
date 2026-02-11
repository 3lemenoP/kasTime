import { Outlet, NavLink } from 'react-router-dom'

function Layout() {
  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header className="border-b border-[var(--border-subtle)]">
        <div className="max-w-[1400px] mx-auto px-6 h-16 flex items-center justify-between">
          {/* Logo */}
          <NavLink to="/" className="flex items-center group">
            <span className="text-lg font-bold tracking-widest text-white">
              KTCS
            </span>
          </NavLink>

          {/* Navigation */}
          <nav className="flex items-center gap-1">
            <NavLink
              to="/"
              end
              className={({ isActive }) =>
                `px-4 py-2 text-sm font-medium tracking-wide transition-colors ${
                  isActive
                    ? 'text-white'
                    : 'text-[var(--text-secondary)] hover:text-white'
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
                    ? 'text-white'
                    : 'text-[var(--text-secondary)] hover:text-white'
                }`
              }
            >
              VERIFY
            </NavLink>
            <NavLink
              to="/docs"
              className={({ isActive }) =>
                `px-4 py-2 text-sm font-medium tracking-wide transition-colors ${
                  isActive
                    ? 'text-white'
                    : 'text-[var(--text-secondary)] hover:text-white'
                }`
              }
            >
              DOCS
            </NavLink>
          </nav>
        </div>
      </header>

      {/* Main content */}
      <main className="flex-1">
        <Outlet />
      </main>

      {/* Footer */}
      <footer className="border-t border-[var(--border-subtle)] py-6">
        <div className="max-w-[1400px] mx-auto px-6 flex items-center justify-between text-xs text-[var(--text-tertiary)]">
          <div className="flex items-center gap-4">
            <span className="font-medium text-[var(--text-secondary)]">KTCS</span>
            <span>v0.1.0</span>
          </div>
          <div>Powered by Kaspa</div>
        </div>
      </footer>
    </div>
  )
}

export default Layout
