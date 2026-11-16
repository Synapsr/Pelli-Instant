import { useEffect } from 'react'
import { currentTransport } from '@/lib/api'
import { useWorkspaceStore } from '@/store/workspace-store'

/** The window (desktop app) or the page (browser, the mock and the bridge) in full screen. */
interface FullscreenTarget {
  isFullscreen: () => Promise<boolean>
  setFullscreen: (on: boolean) => Promise<void>
  /** called when the window may have entered or left full screen */
  onChange: (run: () => void) => Promise<() => void>
}

async function fullscreenTarget(): Promise<FullscreenTarget> {
  if (currentTransport() === 'tauri') {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const win = getCurrentWindow()
    return {
      isFullscreen: () => win.isFullscreen(),
      setFullscreen: (on) => win.setFullscreen(on),
      // macOS's green button and its menu resize the window.
      onChange: (run) => win.onResized(() => run()),
    }
  }
  return {
    isFullscreen: async () => document.fullscreenElement !== null,
    setFullscreen: async (on) => {
      if (on && !document.fullscreenElement) await document.documentElement.requestFullscreen()
      else if (!on && document.fullscreenElement) await document.exitFullscreen()
    },
    onChange: async (run) => {
      document.addEventListener('fullscreenchange', run)
      return () => document.removeEventListener('fullscreenchange', run)
    },
  }
}

/**
 * Puts the window in full screen while the workspace's full-screen mode is on
 * (F), and turns the mode off when the window leaves full screen by itself
 * (macOS's green button, Esc in a browser). A window that was already in full
 * screen before F stays so afterwards.
 */
export function useWindowFullscreen(): void {
  const on = useWorkspaceStore((s) => s.fullscreen)

  useEffect(() => {
    if (!on) return
    let cancelled = false
    // Set once this mode put the window in full screen: only then does leaving it undo that.
    let entered = false
    let unlisten = () => {}
    const ready = (async () => {
      try {
        const target = await fullscreenTarget()
        const wasFullscreen = await target.isFullscreen()
        if (cancelled) return target
        // Only trust "not in full screen" once the window was seen in it: it
        // resizes while it gets there.
        let seen = wasFullscreen
        unlisten = await target.onChange(() => {
          void target.isFullscreen().then((now) => {
            if (now) seen = true
            else if (seen) useWorkspaceStore.getState().exitFullscreen()
          })
        })
        if (!wasFullscreen && !cancelled) {
          entered = true
          await target.setFullscreen(true)
        }
        return target
      } catch {
        // No full screen here (a browser refusing it): the photo alone in the window.
        return null
      }
    })()
    return () => {
      cancelled = true
      void ready.then((target) => {
        unlisten()
        if (target && entered) void target.setFullscreen(false).catch(() => {})
      })
    }
  }, [on])
}
