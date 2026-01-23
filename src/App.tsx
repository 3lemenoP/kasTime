import { Routes, Route } from 'react-router-dom'
import Layout from './components/Layout'
import HomePage from './pages/HomePage'
import VerifyPage from './pages/VerifyPage'
import NetworkPage from './pages/NetworkPage'
import ProofPage from './pages/ProofPage'

function App() {
  return (
    <Routes>
      <Route path="/" element={<Layout />}>
        <Route index element={<HomePage />} />
        <Route path="verify" element={<VerifyPage />} />
        <Route path="network" element={<NetworkPage />} />
        <Route path="proof/:id" element={<ProofPage />} />
      </Route>
    </Routes>
  )
}

export default App
