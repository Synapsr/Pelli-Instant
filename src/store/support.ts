import { api } from '@/lib/api'
import { resultEnterReviews, type KeyTarget } from '@/lib/review/keyboard'
import { isSupportVisible } from '@/lib/support'
import { useAnalysisStore } from './analysis-store'
import { useAppStore } from './app-store'
import { usePlusStore } from './plus-store'

function plus() {
  const { answered, status } = usePlusStore.getState()
  return { plusKnown: answered, plusActive: status.active }
}

/** Whether the support dialog is open now (outside React). */
export function supportDialogVisible(): boolean {
  return isSupportVisible({ offered: useAnalysisStore.getState().supportOpen, ...plus() })
}

/** Whether the result screen shows the support dialog: free version only, Plus checked live. */
export function useSupportDialogVisible(): boolean {
  const open = useAnalysisStore((s) => s.supportOpen)
  const plusKnown = usePlusStore((s) => s.answered)
  const plusActive = usePlusStore((s) => s.status.active)
  return isSupportVisible({ offered: open, plusKnown, plusActive })
}

/**
 * Review (or Enter) on the result of a sort: the first time, a free sort that
 * ended well opens the support dialog first; otherwise straight to the review.
 */
export function goToReview() {
  const analysis = useAnalysisStore.getState()
  const show = isSupportVisible({ offered: analysis.supportOffered, ...plus() })
  analysis.setSupportOpen(show)
  if (!show) useAppStore.getState().reviewResults()
}

/** Every way out of the dialog (×, Escape, a click outside, "See my sort") goes on to the review. */
export function closeSupportDialog() {
  useAnalysisStore.getState().setSupportOpen(false)
  useAppStore.getState().reviewResults()
}

/**
 * Plus activating while the dialog is open hides it: like every other way
 * out, the pending dialog is spent and the review opens, once. The dialog
 * never comes back for this sort, even if Plus is released.
 */
usePlusStore.subscribe((state, before) => {
  if (state.status.active && !before.status.active && useAnalysisStore.getState().supportOpen) closeSupportDialog()
})

/** Instagram or GitHub in the browser, and on to the review. */
export function followFromSupportDialog(url: string) {
  void api().openExternal(url)
  closeSupportDialog()
}

/** The way to Plus: on to the review, with the Plus dialog on its offer over it. */
export function getPlusFromSupportDialog() {
  closeSupportDialog()
  usePlusStore.getState().openDialog('support')
}

/**
 * A key on the result screen of a sort; true when it was handled. Enter goes
 * to the review only from the screen itself: a focused control keeps its
 * Enter, and so do the dialogs over it (support, Plus, the app's own), so a
 * key never acts twice.
 */
export function resultKeyDown(e: { key: string; target: KeyTarget | null; defaultPrevented: boolean }): boolean {
  if (e.defaultPrevented || useAppStore.getState().dialog || usePlusStore.getState().dialogOpen || supportDialogVisible()) return false
  if (!resultEnterReviews(e.key, e.target)) return false
  goToReview()
  return true
}
