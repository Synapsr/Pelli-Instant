import { api } from '@/lib/api'
import { isSupportCardVisible } from '@/lib/support'
import { useAnalysisStore } from './analysis-store'
import { usePlusStore } from './plus-store'

/** Whether the support card shows now (outside React). */
export function supportCardVisible(): boolean {
  const { status, loaded } = usePlusStore.getState()
  return isSupportCardVisible({ offered: useAnalysisStore.getState().supportCard, plusLoaded: loaded, plusActive: status.active })
}

/** Whether the result screen shows the support card: offered by the sort that just ended, free version only, Plus checked live. */
export function useSupportCardVisible(): boolean {
  const offered = useAnalysisStore((s) => s.supportCard)
  const plusLoaded = usePlusStore((s) => s.loaded)
  const plusActive = usePlusStore((s) => s.status.active)
  return isSupportCardVisible({ offered, plusLoaded, plusActive })
}

/** The ×: hidden for this sort; the next one offers it again. */
export function closeSupportCard() {
  useAnalysisStore.getState().closeSupportCard()
}

/** Instagram or GitHub, in the browser: that's enough for this sort, the card closes. */
export function followFromSupportCard(url: string) {
  closeSupportCard()
  void api().openExternal(url)
}

/** The discreet line to Plus: the usual Plus dialog, on its offer. The card stays until Plus is active. */
export function getPlusFromSupportCard() {
  usePlusStore.getState().openDialog('support')
}
