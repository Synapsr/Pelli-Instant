import { addToast, Button } from '@heroui/react'
import { useEffect } from 'react'
import { AnalysisScreen } from './components/analysis/AnalysisScreen'
import { BootScreen } from './components/boot/BootScreen'
import { ExportDialog } from './components/export/ExportDialog'
import { HomeScreen } from './components/home/HomeScreen'
import { Onboarding } from './components/onboarding/Onboarding'
import { OpenErrorDialog } from './components/home/OpenErrorDialog'
import { PreAnalysisDialog } from './components/criteria/PreAnalysisDialog'
import { ShortcutsDialog } from './components/review/ShortcutsDialog'
import { Workspace } from './components/review/Workspace'
import { SettingsDialog } from './components/settings/SettingsDialog'
import { SignInScreen } from './components/signin/SignInScreen'
import { useApplyTheme } from './hooks/use-theme'
import { api } from './lib/api'
import { setNotifier } from './lib/notify'
import { unlockAudioOnFirstGesture } from './lib/sound'
import { useAppStore } from './store/app-store'
import { connectEvents } from './store/events'

let started = false

function startOnce(): void {
  if (started) return
  started = true
  unlockAudioOnFirstGesture()
  setNotifier(({ title, description, tone = 'default', action, timeout = 4000 }) => {
    addToast({
      title,
      description,
      color: tone,
      timeout,
      shouldShowTimeoutProgress: false,
      endContent: action ? (
        <Button size="sm" variant="flat" onPress={action.run} className="h-7 min-w-0 px-2.5 text-xs font-semibold">
          {action.label}
        </Button>
      ) : undefined,
    })
  })
  void useAppStore
    .getState()
    .boot()
    .then(() => {
      if (useAppStore.getState().phase === 'ready') connectEvents(api())
    })
}

export function App() {
  const phase = useAppStore((s) => s.phase)
  const screen = useAppStore((s) => s.screen)
  // One immersive dark look (photos are judged on a dark surround).
  useApplyTheme('dark')

  useEffect(startOnce, [])

  if (phase !== 'ready') return <BootScreen />

  return (
    <div className="h-full bg-background text-foreground">
      {screen === 'signIn' && <SignInScreen />}
      {screen === 'onboarding' && <Onboarding />}
      {screen === 'home' && <HomeScreen />}
      {screen === 'analysis' && <AnalysisScreen />}
      {screen === 'workspace' && <Workspace />}
      <OpenErrorDialog />
      <PreAnalysisDialog />
      <SettingsDialog />
      <ExportDialog />
      <ShortcutsDialog />
    </div>
  )
}
