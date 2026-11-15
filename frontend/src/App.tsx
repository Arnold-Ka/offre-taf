import { useEffect, useState } from 'react'

type Job = {
  id: string
  title: string
  company: string
  location: string | null
  work_mode: string | null
  contract_type: string
  relocation_support: boolean
  eligibility_confidence: string
  source_url: string
  source_name: string
  posted_at: string | null
  collected_at: string
  is_read: boolean
  is_favorite: boolean
  mentions_java: boolean
  remote_scope: string
}

type Summary = {
  total: number
  unread: number
  burkina: number
  remote: number
  remote_africa: number
  relocation: number
  favorites: number
  java: number
}

type Tab = 'all' | 'burkina' | 'remote' | 'relocation' | 'favorites'

const API_URL = import.meta.env.VITE_API_URL ?? 'http://127.0.0.1:3000'

const TABS: { id: Tab; label: string; count: (summary: Summary) => number; hint: string }[] = [
  { id: 'burkina', label: 'Burkina Faso', count: (s) => s.burkina, hint: 'Offres IT publiées par les entreprises installées au Burkina, grandes ou petites.' },
  { id: 'remote', label: 'Télétravail', count: (s) => s.remote, hint: 'Postes à distance, en Afrique ou hors d’Afrique. La zone autorisée est indiquée sur chaque offre.' },
  { id: 'relocation', label: 'Relocalisation', count: (s) => s.relocation, hint: 'Offres qui mentionnent une relocalisation ou un visa. À confirmer avec l’employeur.' },
  { id: 'favorites', label: '★ Favoris', count: (s) => s.favorites, hint: 'Les offres Java sont ajoutées automatiquement ; ajoute ou retire les autres avec l’étoile.' },
  { id: 'all', label: 'Toutes', count: (s) => s.total, hint: 'Toutes les offres collectées.' },
]

const SCOPE_LABELS: Record<string, { label: string; open: boolean }> = {
  worldwide: { label: 'Monde entier', open: true },
  africa: { label: 'Depuis l’Afrique', open: true },
  emea: { label: 'EMEA (Afrique incluse)', open: true },
  europe: { label: 'Europe uniquement', open: false },
  americas: { label: 'Amériques uniquement', open: false },
  apac: { label: 'Asie-Pacifique uniquement', open: false },
  unknown: { label: 'Zone à vérifier', open: false },
}

const EMPTY_SUMMARY: Summary = { total: 0, unread: 0, burkina: 0, remote: 0, remote_africa: 0, relocation: 0, favorites: 0, java: 0 }

function App() {
  const [jobs, setJobs] = useState<Job[]>([])
  const [summary, setSummary] = useState<Summary>(EMPTY_SUMMARY)
  const [tab, setTab] = useState<Tab>('burkina')
  const [search, setSearch] = useState('')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [unreadOnly, setUnreadOnly] = useState(false)
  const [javaOnly, setJavaOnly] = useState(false)
  const [africaOnly, setAfricaOnly] = useState(false)
  const [country, setCountry] = useState('')
  const [workMode, setWorkMode] = useState('')
  const [contractType, setContractType] = useState('')
  const [relocationOnly, setRelocationOnly] = useState(false)
  const [filtersOpen, setFiltersOpen] = useState(false)
  const [refreshKey, setRefreshKey] = useState(0)
  const [updatingJob, setUpdatingJob] = useState('')
  const activeFilterCount = [country.trim(), workMode, contractType.trim(), relocationOnly].filter(Boolean).length
  const currentTab = TABS.find((item) => item.id === tab) ?? TABS[0]

  useEffect(() => {
    const timer = window.setInterval(() => setRefreshKey((value) => value + 1), 60_000)
    return () => window.clearInterval(timer)
  }, [])

  useEffect(() => {
    const controller = new AbortController()
    const timer = window.setTimeout(async () => {
      setLoading(true)
      setError('')
      try {
        const params = new URLSearchParams()
        if (search.trim()) params.set('q', search.trim())
        if (country.trim()) params.set('country', country.trim())
        if (workMode) params.set('work_mode', workMode)
        if (contractType.trim()) params.set('contract_type', contractType.trim())
        if (relocationOnly) params.set('relocation', 'true')
        if (unreadOnly) params.set('unread', 'true')
        if (javaOnly) params.set('java', 'true')
        if (tab === 'burkina' && !country.trim()) params.set('country', 'Burkina')
        if (tab === 'remote') {
          params.set('work_mode', 'Télétravail')
          if (africaOnly) params.set('scope', 'africa')
        }
        if (tab === 'relocation') params.set('relocation', 'true')
        if (tab === 'favorites') params.set('favorite', 'true')
        const response = await fetch(`${API_URL}/api/jobs?${params}`, { signal: controller.signal })
        if (!response.ok) throw new Error('La recherche a échoué.')
        setJobs((await response.json()) as Job[])
        const summaryResponse = await fetch(`${API_URL}/api/jobs/summary`, { signal: controller.signal })
        if (summaryResponse.ok) setSummary((await summaryResponse.json()) as Summary)
      } catch (cause) {
        if (cause instanceof DOMException && cause.name === 'AbortError') return
        setError('Connexion à l’API impossible. Vérifie que le backend et PostgreSQL sont démarrés.')
      } finally {
        setLoading(false)
      }
    }, 250)

    return () => {
      window.clearTimeout(timer)
      controller.abort()
    }
  }, [search, country, workMode, contractType, relocationOnly, unreadOnly, javaOnly, africaOnly, tab, refreshKey])

  async function markAsRead(job: Job) {
    setUpdatingJob(job.id)
    try {
      const response = await fetch(`${API_URL}/api/jobs/${job.id}/read`, { method: 'PATCH' })
      if (!response.ok) throw new Error('Impossible de mettre à jour cette offre.')
      setJobs((current) => unreadOnly
        ? current.filter((item) => item.id !== job.id)
        : current.map((item) => item.id === job.id ? { ...item, is_read: true } : item))
      setSummary((current) => ({ ...current, unread: Math.max(0, current.unread - 1) }))
    } catch {
      setError('Impossible de marquer cette offre comme lue. Réessaie.')
    } finally {
      setUpdatingJob('')
    }
  }

  async function toggleFavorite(job: Job) {
    const favorite = !job.is_favorite
    setUpdatingJob(job.id)
    try {
      const response = await fetch(`${API_URL}/api/jobs/${job.id}/favorite`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ favorite }),
      })
      if (!response.ok) throw new Error('Impossible de mettre à jour cette offre.')
      setJobs((current) => tab === 'favorites' && !favorite
        ? current.filter((item) => item.id !== job.id)
        : current.map((item) => item.id === job.id ? { ...item, is_favorite: favorite } : item))
      setSummary((current) => ({ ...current, favorites: Math.max(0, current.favorites + (favorite ? 1 : -1)) }))
    } catch {
      setError('Impossible de modifier les favoris. Réessaie.')
    } finally {
      setUpdatingJob('')
    }
  }

  return (
    <main className="page-shell">
      <header className="topbar">
        <a className="brand" href="/" aria-label="Offre Taf, accueil">
          <span className="brand-mark">O</span>
          <span>offre<span className="brand-light">taf</span></span>
        </a>
        <button className={`alerts-button ${unreadOnly ? 'alerts-active' : ''}`} onClick={() => setUnreadOnly((value) => !value)} aria-pressed={unreadOnly}>
          Nouvelles offres <span className="alert-count">{summary.unread}</span>
        </button>
      </header>

      <section className="hero">
        <p className="eyebrow">DIRECTEMENT SUR LES SITES DES ENTREPRISES</p>
        <h1>Le bon poste.<br /><span>Au Burkina ou à distance.</span></h1>
        <p className="hero-copy">Les offres IT des entreprises burkinabè, les postes en télétravail ouverts depuis l’Afrique et les offres avec relocalisation, réunis au même endroit.</p>
        <label className="search-box">
          <span className="search-icon" aria-hidden="true">⌕</span>
          <input
            aria-label="Rechercher un poste ou une entreprise"
            placeholder="Java, Spring, backend, réseau…"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </label>
        <div className="search-hints">
          <span>Recherches populaires</span>
          {['Spring', 'Backend', 'Full-stack', 'DevOps', 'Réseau'].map((tag) => (
            <button key={tag} onClick={() => setSearch(tag)}>{tag}</button>
          ))}
        </div>
      </section>

      <section className="results-section">
        <nav className="tabs" aria-label="Catégories d’offres">
          {TABS.map((item) => (
            <button key={item.id} className={`tab ${tab === item.id ? 'tab-active' : ''}`} onClick={() => setTab(item.id)} aria-pressed={tab === item.id}>
              {item.label} <span className="tab-count">{item.count(summary)}</span>
            </button>
          ))}
        </nav>

        <div className="section-heading">
          <div>
            <p className="eyebrow">{unreadOnly ? 'NOUVELLES OFFRES SEULEMENT' : 'SÉLECTION POUR TOI'}</p>
            <h2>{currentTab.label.replace('★ ', '')} <span className="count">{jobs.length}</span></h2>
            <p className="tab-hint">{currentTab.hint}</p>
          </div>
          <div className="heading-actions">
            <label className="toggle">
              <input type="checkbox" checked={javaOnly} onChange={(event) => setJavaOnly(event.target.checked)} />
              <span>Java uniquement</span>
            </label>
            {tab === 'remote' && (
              <label className="toggle">
                <input type="checkbox" checked={africaOnly} onChange={(event) => setAfricaOnly(event.target.checked)} />
                <span>Accessible depuis l’Afrique ({summary.remote_africa})</span>
              </label>
            )}
            <button className={`filter-button ${filtersOpen ? 'filter-button-active' : ''}`} type="button" onClick={() => setFiltersOpen((value) => !value)} aria-expanded={filtersOpen}>
              <span aria-hidden="true">☷</span> Filtres {activeFilterCount > 0 && <span className="filter-count">{activeFilterCount}</span>}
            </button>
          </div>
        </div>

        {filtersOpen && (
          <div className="filter-panel">
            <label className="filter-field">
              <span>Pays ou région</span>
              <input value={country} onChange={(event) => setCountry(event.target.value)} placeholder="Ex. Sénégal, Côte d’Ivoire, Burkina Faso" />
            </label>
            <label className="filter-field">
              <span>Mode de travail</span>
              <select value={workMode} onChange={(event) => setWorkMode(event.target.value)}>
                <option value="">Tous les modes</option>
                <option value="Télétravail">Télétravail</option>
                <option value="Sur site">Sur site / hybride</option>
              </select>
            </label>
            <label className="filter-field">
              <span>Type de contrat</span>
              <input value={contractType} onChange={(event) => setContractType(event.target.value)} placeholder="Ex. CDI, Full-time, Stage" />
            </label>
            <label className="relocation-filter">
              <input type="checkbox" checked={relocationOnly} onChange={(event) => setRelocationOnly(event.target.checked)} />
              <span>Visa ou relocalisation mentionné</span>
            </label>
            <button className="clear-filters" type="button" onClick={() => { setCountry(''); setWorkMode(''); setContractType(''); setRelocationOnly(false) }} disabled={activeFilterCount === 0}>Effacer les filtres</button>
          </div>
        )}

        {loading ? (
          <div className="empty-state"><span className="loader" />Chargement des offres…</div>
        ) : error ? (
          <div className="empty-state error-state"><strong>Les offres ne sont pas accessibles.</strong><span>{error}</span></div>
        ) : jobs.length === 0 ? (
          <div className="empty-state"><span className="empty-icon">✳</span><strong>{unreadOnly ? 'Tout est à jour' : 'Aucune offre pour le moment'}</strong><span>{unreadOnly ? 'Les nouvelles offres apparaîtront ici dès leur collecte.' : 'Les sites des entreprises sont consultés automatiquement ; les nouvelles offres apparaîtront ici.'}</span></div>
        ) : (
          <div className="job-list">
            {jobs.map((job) => {
              const scope = SCOPE_LABELS[job.remote_scope]
              return (
                <article className={`job-card ${job.is_read ? '' : 'job-unread'} ${job.is_favorite ? 'job-favorite' : ''}`} key={job.id}>
                  <div className="company-avatar">{job.company.slice(0, 1).toUpperCase()}</div>
                  <div className="job-details">
                    <div className="job-heading"><h3>{job.title}</h3>{!job.is_read && <span className="new-badge">Nouveau</span>}<a href={job.source_url} target="_blank" rel="noreferrer" aria-label={`Voir l’offre chez ${job.company}`}>↗</a></div>
                    <p className="company-name">{job.company}</p>
                    <div className="job-tags">
                      {job.mentions_java && <span className="java-tag">Java</span>}
                      {job.location && <span>⌖ {job.location}</span>}
                      {job.work_mode && <span className="remote-tag">{job.work_mode}</span>}
                      {scope && <span className={scope.open ? 'africa-tag' : 'scope-tag'}>{scope.label}</span>}
                      {job.contract_type && <span>{job.contract_type}</span>}
                      {job.eligibility_confidence === 'yes' && <span className="africa-tag">Ouvert à l’Afrique</span>}
                      {job.eligibility_confidence === 'likely' && <span className="relocation-tag">Éligibilité Afrique à vérifier</span>}
                      {job.relocation_support && <span className="relocation-tag">Visa / relocalisation mentionné</span>}
                      {job.source_name && <span>{job.source_name}</span>}
                    </div>
                  </div>
                  <div className="job-actions">
                    <button className={`star-button ${job.is_favorite ? 'star-active' : ''}`} title={job.is_favorite ? 'Retirer des favoris' : 'Ajouter aux favoris'} aria-pressed={job.is_favorite} onClick={() => void toggleFavorite(job)} disabled={updatingJob === job.id}>{job.is_favorite ? '★' : '☆'}</button>
                    <time>{new Date(job.posted_at ?? job.collected_at).toLocaleDateString('fr-FR')}</time>
                    {!job.is_read && <button className="read-button" title="Marquer comme lue" onClick={() => void markAsRead(job)} disabled={updatingJob === job.id}>{updatingJob === job.id ? '…' : 'Lu'}</button>}
                  </div>
                </article>
              )
            })}
          </div>
        )}
      </section>
      <footer>Sources : sites carrière des entreprises (Greenhouse, Lever, Ashby, Workable) · sites officiels d’employeurs burkinabè (SONABEL, Coris, Moov Africa, Telecel, ANPTIC, 2iE, CILSS…) · <a href="https://sabmadigital.com/bf/carrieres/" target="_blank" rel="noreferrer">SABMA</a> · <a href="https://qualive-sarl.org/carriere" target="_blank" rel="noreferrer">Qualive</a> · <a href="https://www.bfemploi.com/" target="_blank" rel="noreferrer">BFemploi</a> · <a href="https://www.emploidakar.com/" target="_blank" rel="noreferrer">Emploi Dakar</a> · <a href="https://opentoafrica.com/" target="_blank" rel="noreferrer">OpenToAfrica</a> · <a href="https://remotive.com/remote-jobs" target="_blank" rel="noreferrer">Remotive</a> · <a href="https://www.arbeitnow.com/" target="_blank" rel="noreferrer">Arbeitnow</a> <span>✳</span></footer>
    </main>
  )
}

export default App
