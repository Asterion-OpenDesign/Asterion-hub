import { useEffect } from 'react'
import { hasHost } from './api'
import { AppDetail } from './components/AppDetail'
import { ConfirmDialog, Sidebar, Toasts } from './components/Shell'
import { Wordmark } from './components/Logo'
import { Apps, History, Home, Installed, SettingsView, Tasks, Updates } from './views'
import { useHub } from './store'
import { describeError } from './copy'

export default function App() {
  const { init, booted, bootError, page } = useHub()
  useEffect(() => {
    if (hasHost()) void init()
  }, [init])

  if (!hasHost()) {
    return (
      <div className="splash">
        <Wordmark />
        <h1>Esta página necesita el motor del Hub</h1>
        <p>
          El motor (Rust) es quien instala, verifica y detecta aplicaciones; esta interfaz solo lo muestra. Ábrela con <code>pnpm tauri dev</code> y no se mostrará ningún dato inventado.
        </p>
      </div>
    )
  }
  if (!booted) {
    return (
      <div className="splash" aria-busy>
        <Wordmark />
      </div>
    )
  }
  if (bootError) {
    const c = describeError(bootError)
    return (
      <div className="splash">
        <Wordmark />
        <h1>ASTERION Hub no pudo arrancar</h1>
        <p>
          {c.title}. {c.hint}
        </p>
        <pre className="selectable">{bootError.detail}</pre>
      </div>
    )
  }

  return (
    <div className="app">
      <Sidebar />
      <main className="main" id="main">
        <div className="main__inner">
          {page === 'home' && <Home />}
          {page === 'apps' && <Apps />}
          {page === 'installed' && <Installed />}
          {page === 'updates' && <Updates />}
          {page === 'tasks' && <Tasks />}
          {page === 'history' && <History />}
          {page === 'settings' && <SettingsView />}
        </div>
      </main>
      <AppDetail />
      <ConfirmDialog />
      <Toasts />
    </div>
  )
}
