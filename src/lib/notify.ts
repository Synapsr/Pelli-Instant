/**
 * Toast notifications, decoupled from the UI kit so stores stay testable in
 * Node. The app plugs HeroUI's `addToast` in at start-up.
 */
export type ToastTone = 'default' | 'success' | 'warning' | 'danger'

export interface ToastAction {
  label: string
  run: () => void
}

export interface ToastInput {
  title: string
  description?: string
  tone?: ToastTone
  action?: ToastAction
  /** ms, default 4000 */
  timeout?: number
}

type Notifier = (toast: ToastInput) => void

let notifier: Notifier = () => {}

export function setNotifier(next: Notifier): void {
  notifier = next
}

export function notify(toast: ToastInput): void {
  notifier(toast)
}
