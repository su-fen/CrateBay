import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'

function applyInitialThemeClass() {
  try {
    const stored = localStorage.getItem('theme')
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
    const effective =
      stored === 'dark' ? 'dark' : stored === 'light' ? 'light' : prefersDark ? 'dark' : 'light'

    document.documentElement.classList.toggle('dark', effective === 'dark')
    document.documentElement.style.colorScheme = effective
  } catch {
    // ignore
  }
}

applyInitialThemeClass()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
