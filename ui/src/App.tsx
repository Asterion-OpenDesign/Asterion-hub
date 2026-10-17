import { useEffect } from 'react'
import { hasHost } from './api'
import { Wordmark } from './components/Logo'
import { Palette } from './components/Palette'
import { ConfirmDialog, Rail, Toasts, TopBar } from './components/Shell'
import { describeError } from './copy'
import { useHub } from './store'
import { AppPage } from './views/AppPage'
import { Catalog } from './views/Catalog'
import { Home } from './views/Home'
import { History, Installed, Tasks, Updates } from './views/Lists'
import { SettingsView } from './views/Settings'

export default function App() {
  const { init, booted, bootError, route } = useHub()
  useEffect(() => {
    if (hasHost()) void init()
  }, [init])

  if (!hasHost()) {
    return (
      <div className="splash">
        <Wordmark />
        <h1>Esta página necesita el motor del Hub</h1>
        <p>
          El motor (Rust) es quien instala, verifica y detecta aplicaciones; esta interfaz solo lo muestra. Ábrela con <code>tauri dev</code> y no se mostrará ningún dato inventado.
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
      <Rail />
      <div className="col">
        <TopBar />
        <main className="scroll" id="main" key={route.page + (route.appId ?? '')}>
          <div className="view">
            {route.appId ? (
              <AppPage />
            ) : (
              <>
                {route.page === 'home' && <Home />}
                {route.page === 'apps' && <Catalog />}
                {route.page === 'installed' && <Installed />}
                {route.page === 'updates' && <Updates />}
                {route.page === 'tasks' && <Tasks />}
                {route.page === 'history' && <History />}
                {route.page === 'settings' && <SettingsView />}
              </>
            )}
          </div>
        </main>
      </div>
      <Palette />
      <ConfirmDialog />
      <Toasts />
    </div>
  )
}