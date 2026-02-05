import { Routes, Route } from 'react-router-dom'
import { ErrorBoundary } from 'react-error-boundary'
import Layout from './components/Layout'
import HomePage from './pages/HomePage'
import VerifyPage from './pages/VerifyPage'
import ProofPage from './pages/ProofPage'

function ErrorFallback() {
  return (
    <div className="min-h-screen flex items-center justify-center bg-[var(--bg-primary)]">
      <div className="text-center p-8">
        <h1 className="text-2xl font-bold text-[var(--text-primary)] mb-4">Something went wrong</h1>
        <p className="text-[var(--text-secondary)] mb-6">Please refresh the page to try again.</p>
        <button
          onClick={() => window.location.reload()}
          className="px-4 py-2 bg-[var(--accent-primary)] text-black rounded font-medium hover:opacity-90 transition-opacity"
        >
          Refresh Page
        </button>
      </div>
    </div>
  )
}

function App() {
  return (
    <ErrorBoundary FallbackComponent={ErrorFallback}>
      <Routes>
        <Route path="/" element={<Layout />}>
          <Route index element={<HomePage />} />
          <Route path="verify" element={<VerifyPage />} />
          <Route path="proof/:id" element={<ProofPage />} />
        </Route>
      </Routes>
    </ErrorBoundary>
  )
}

export default App
