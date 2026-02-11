import { useState, useMemo } from 'react'
import { useSearchParams } from 'react-router-dom'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {
  Book,
  Rocket,
  Layers,
  Shield,
  Server,
  Terminal,
  Globe,
  FileCode,
  Cpu,
  ChevronRight,
  ChevronDown,
  FileText,
  Zap,
  MonitorSmartphone,
} from 'lucide-react'

// Import all docs as raw strings
import indexDoc from '../../docs/index.md?raw'
import installationDoc from '../../docs/getting-started/installation.md?raw'
import quickstartDoc from '../../docs/getting-started/quickstart.md?raw'
import architectureDoc from '../../docs/concepts/architecture.md?raw'
import proofFormatDoc from '../../docs/concepts/proof-format.md?raw'
import securityDoc from '../../docs/concepts/security.md?raw'
import coreDoc from '../../docs/components/core.md?raw'
import calendarDoc from '../../docs/components/calendar.md?raw'
import cliDoc from '../../docs/components/cli.md?raw'
import wasmDoc from '../../docs/components/wasm.md?raw'
import frontendDoc from '../../docs/components/frontend.md?raw'
import apiReferenceDoc from '../../docs/api/reference.md?raw'
import deploymentDoc from '../../docs/deployment/production.md?raw'
import proofSpecDoc from '../../docs/PROOF_FORMAT_SPECIFICATION.md?raw'

interface DocEntry {
  id: string
  title: string
  content: string
  icon: React.ReactNode
}

interface DocSection {
  id: string
  title: string
  icon: React.ReactNode
  entries: DocEntry[]
}

const sections: DocSection[] = [
  {
    id: 'overview',
    title: 'Overview',
    icon: <Book className="w-4 h-4" />,
    entries: [
      { id: 'index', title: 'Introduction', content: indexDoc, icon: <Book className="w-3.5 h-3.5" /> },
    ],
  },
  {
    id: 'getting-started',
    title: 'Getting Started',
    icon: <Rocket className="w-4 h-4" />,
    entries: [
      { id: 'installation', title: 'Installation', content: installationDoc, icon: <Zap className="w-3.5 h-3.5" /> },
      { id: 'quickstart', title: 'Quick Start', content: quickstartDoc, icon: <Rocket className="w-3.5 h-3.5" /> },
    ],
  },
  {
    id: 'concepts',
    title: 'Concepts',
    icon: <Layers className="w-4 h-4" />,
    entries: [
      { id: 'architecture', title: 'Architecture', content: architectureDoc, icon: <Layers className="w-3.5 h-3.5" /> },
      { id: 'proof-format', title: 'Proof Format', content: proofFormatDoc, icon: <FileText className="w-3.5 h-3.5" /> },
      { id: 'proof-spec', title: 'Proof Specification', content: proofSpecDoc, icon: <FileCode className="w-3.5 h-3.5" /> },
      { id: 'security', title: 'Security Model', content: securityDoc, icon: <Shield className="w-3.5 h-3.5" /> },
    ],
  },
  {
    id: 'components',
    title: 'Components',
    icon: <Cpu className="w-4 h-4" />,
    entries: [
      { id: 'core', title: 'Core Library', content: coreDoc, icon: <Cpu className="w-3.5 h-3.5" /> },
      { id: 'calendar', title: 'Calendar Server', content: calendarDoc, icon: <Server className="w-3.5 h-3.5" /> },
      { id: 'cli', title: 'CLI Tool', content: cliDoc, icon: <Terminal className="w-3.5 h-3.5" /> },
      { id: 'wasm', title: 'WASM Bindings', content: wasmDoc, icon: <Globe className="w-3.5 h-3.5" /> },
      { id: 'frontend', title: 'Web Frontend', content: frontendDoc, icon: <MonitorSmartphone className="w-3.5 h-3.5" /> },
    ],
  },
  {
    id: 'api',
    title: 'API',
    icon: <FileCode className="w-4 h-4" />,
    entries: [
      { id: 'reference', title: 'API Reference', content: apiReferenceDoc, icon: <FileCode className="w-3.5 h-3.5" /> },
    ],
  },
  {
    id: 'deployment',
    title: 'Deployment',
    icon: <Server className="w-4 h-4" />,
    entries: [
      { id: 'production', title: 'Production Guide', content: deploymentDoc, icon: <Server className="w-3.5 h-3.5" /> },
    ],
  },
]

// All doc entries flat for lookup
const allDocs = sections.flatMap(s => s.entries)

/**
 * Strip MkDocs-specific syntax from markdown content
 * so it renders cleanly with react-markdown.
 */
function preprocessMarkdown(raw: string): string {
  const lines = raw.split('\n')
  const out: string[] = []
  let i = 0

  while (i < lines.length) {
    const line = lines[i]

    // Convert admonitions: !!! type "Title" → blockquote
    const admonitionMatch = line.match(/^(!{3})\s+(\w+)\s*(?:"([^"]*)")?/)
    if (admonitionMatch) {
      const type = admonitionMatch[2]
      const title = admonitionMatch[3] || type.charAt(0).toUpperCase() + type.slice(1)
      out.push(`> **${title}**`)
      out.push('>')
      i++
      // Collect indented content
      while (i < lines.length && (lines[i].startsWith('    ') || lines[i].trim() === '')) {
        if (lines[i].trim() === '') {
          out.push('>')
        } else {
          out.push(`> ${lines[i].slice(4)}`)
        }
        i++
      }
      out.push('')
      continue
    }

    // Convert tabbed content: === "Title" → ### Title
    const tabMatch = line.match(/^===\s+"([^"]*)"/)
    if (tabMatch) {
      out.push(`### ${tabMatch[1]}`)
      out.push('')
      i++
      // Collect indented content (un-indent by 4 spaces)
      while (i < lines.length && (lines[i].startsWith('    ') || lines[i].trim() === '')) {
        if (lines[i].trim() === '') {
          out.push('')
        } else {
          out.push(lines[i].slice(4))
        }
        i++
      }
      out.push('')
      continue
    }

    // Strip Material icon references and MkDocs button syntax
    let processed = line
      .replace(/:material-[\w-]+:\{[^}]*\}/g, '')
      .replace(/:material-[\w-]+:/g, '')
      .replace(/:octicons-[\w-]+:/g, '')
      .replace(/\{[^}]*\.md-button[^}]*\}/g, '')
      .replace(/<div class="grid cards" markdown>/g, '')
      .replace(/<\/div>/g, '')
      .replace(/<p class="hero-subtitle">(.*?)<\/p>/g, '*$1*')

    out.push(processed)
    i++
  }

  return out.join('\n')
}

function DocsPage() {
  const [searchParams, setSearchParams] = useSearchParams()
  const activeDocId = searchParams.get('page') || 'index'
  const [expandedSections, setExpandedSections] = useState<Set<string>>(() => {
    // Expand the section containing the active doc
    const activeSectionId = sections.find(s => s.entries.some(e => e.id === activeDocId))?.id
    return new Set(activeSectionId ? [activeSectionId] : ['overview'])
  })

  const activeDoc = allDocs.find(d => d.id === activeDocId) || allDocs[0]
  const processedContent = useMemo(() => preprocessMarkdown(activeDoc.content), [activeDoc.content])

  const toggleSection = (sectionId: string) => {
    setExpandedSections(prev => {
      const next = new Set(prev)
      if (next.has(sectionId)) {
        next.delete(sectionId)
      } else {
        next.add(sectionId)
      }
      return next
    })
  }

  const navigateTo = (docId: string) => {
    setSearchParams({ page: docId })
    // Expand parent section
    const parentSection = sections.find(s => s.entries.some(e => e.id === docId))
    if (parentSection) {
      setExpandedSections(prev => new Set(prev).add(parentSection.id))
    }
    // Scroll to top
    window.scrollTo(0, 0)
  }

  return (
    <div className="max-w-[1400px] mx-auto flex min-h-[calc(100vh-8rem)]">
      {/* Sidebar */}
      <aside className="w-64 shrink-0 border-r border-[var(--border-subtle)] bg-[var(--bg-secondary)] overflow-y-auto sticky top-16 h-[calc(100vh-8rem)]">
        <nav className="py-4">
          {sections.map(section => {
            const isExpanded = expandedSections.has(section.id)
            return (
              <div key={section.id} className="mb-1">
                <button
                  onClick={() => toggleSection(section.id)}
                  className="w-full flex items-center gap-2 px-4 py-2 text-sm font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-tertiary)] transition-colors"
                >
                  <span className="text-[var(--text-tertiary)]">{section.icon}</span>
                  <span className="flex-1 text-left">{section.title}</span>
                  {isExpanded ? (
                    <ChevronDown className="w-3.5 h-3.5 text-[var(--text-tertiary)]" />
                  ) : (
                    <ChevronRight className="w-3.5 h-3.5 text-[var(--text-tertiary)]" />
                  )}
                </button>
                {isExpanded && (
                  <div className="ml-4">
                    {section.entries.map(entry => (
                      <button
                        key={entry.id}
                        onClick={() => navigateTo(entry.id)}
                        className={`w-full flex items-center gap-2 px-4 py-1.5 text-sm transition-colors ${
                          activeDocId === entry.id
                            ? 'text-white bg-white/10 border-l-2 border-white'
                            : 'text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)]'
                        }`}
                      >
                        {entry.icon}
                        <span>{entry.title}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )
          })}
        </nav>
      </aside>

      {/* Content */}
      <main className="flex-1 min-w-0 px-8 py-8 lg:px-12">
        <article className="docs-content max-w-4xl">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{processedContent}</ReactMarkdown>
        </article>
      </main>
    </div>
  )
}

export default DocsPage
