import { currentTransport } from '@/lib/api'
import { isMac } from '@/lib/platform'

/**
 * Width kept free at the left of the top bars for the macOS window buttons:
 * the desktop window draws its content under a transparent title bar
 * (`titleBarStyle: Overlay`). Zero in a browser and on Windows.
 */
export function windowButtonsInset(): number {
  return currentTransport() === 'tauri' && isMac() ? 76 : 0
}
