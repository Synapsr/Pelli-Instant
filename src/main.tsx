import { HeroUIProvider, ToastProvider } from '@heroui/react'
import { MotionConfig } from 'framer-motion'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import { localeOf, useI18nStore } from './lib/i18n'
import './styles/globals.css'

function Root() {
  const language = useI18nStore((s) => s.language)
  return (
    <HeroUIProvider locale={localeOf(language)} reducedMotion="user" validationBehavior="native">
      <MotionConfig reducedMotion="user">
        <ToastProvider placement="bottom-center" maxVisibleToasts={3} toastOffset={20} toastProps={{ variant: 'flat', radius: 'lg' }} />
        <App />
      </MotionConfig>
    </HeroUIProvider>
  )
}

createRoot(document.getElementById('root') as HTMLElement).render(
  <StrictMode>
    <Root />
  </StrictMode>,
)
