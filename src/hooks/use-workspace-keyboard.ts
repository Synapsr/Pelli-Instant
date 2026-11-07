import { useEffect } from 'react'
import { isTypingTarget, leftToControl, resolveShortcut, type KeyTarget, type ShortcutAction } from '@/lib/review/keyboard'
import { t } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { useAppStore } from '@/store/app-store'
import { usePrefsStore } from '@/store/prefs-store'
import { useReviewStore } from '@/store/review-store'
import { COMPARE_PAGE_SIZE, useWorkspaceStore } from '@/store/workspace-store'

/** Ids compared with C: the multi-selection, else the focused group, else focus + next. */
export function compareCandidates(): string[] {
  const review = useReviewStore.getState()
  const selected = Object.keys(review.selection)
  if (selected.length >= 2) return review.visibleIds.filter((id) => review.selection[id])
  const focus = review.focusId
  if (!focus) return []
  const gid = review.photos[focus]?.assessment?.groupId
  if (gid !== null && gid !== undefined && review.groups[gid]) return review.groups[gid].photoIds
  const index = review.visibleIds.indexOf(focus)
  return review.visibleIds.slice(index, index + 2)
}

/** Zoom target of the loupe: the sharpest face, else the centre. */
export function zoomTarget(photoId: string | null): { x: number; y: number } {
  const photo = photoId ? useReviewStore.getState().photos[photoId] : undefined
  const face = [...(photo?.faces ?? [])].sort((a, b) => (b.sharpness ?? 0) * b.bbox.h - (a.sharpness ?? 0) * a.bbox.h)[0]
  return face ? { x: face.bbox.x + face.bbox.w / 2, y: face.bbox.y + face.bbox.h * 0.45 } : { x: 0.5, y: 0.5 }
}

export function pickCompareWinner(index: number): void {
  const ws = useWorkspaceStore.getState()
  const page = ws.compareIds.slice(ws.comparePage * COMPARE_PAGE_SIZE, ws.comparePage * COMPARE_PAGE_SIZE + COMPARE_PAGE_SIZE)
  const winner = page[index]
  if (!winner) return
  const others = ws.compareIds.filter((id) => id !== winner)
  const review = useReviewStore.getState()
  void review.markMany(
    [
      { ids: [winner], patch: { flag: 'pick' } },
      { ids: others, patch: { flag: 'reject' } },
    ],
    winner,
  )
  notify({
    title: t('compare.winner', { name: review.photos[winner]?.fileName ?? '' }),
    tone: 'success',
    action: { label: t('marks.undo'), run: () => void useReviewStore.getState().undo() },
  })
  ws.closeOverlay()
}

function run(action: ShortcutAction): void {
  const review = useReviewStore.getState()
  const ws = useWorkspaceStore.getState()
  const app = useAppStore.getState()
  switch (action.type) {
    case 'move':
      review.move(action.dir)
      if (ws.mode === 'loupe') ws.resetZoom()
      break
    case 'groupMove':
      review.moveInGroup(action.delta)
      if (ws.mode === 'loupe') ws.resetZoom()
      break
    case 'flag':
      void review.mark({ flag: action.flag })
      break
    case 'rating':
      void review.mark({ rating: action.rating })
      break
    case 'label':
      void review.mark({ label: action.label })
      break
    case 'loupe':
      if (review.focusId) ws.toggleLoupe()
      break
    case 'escape':
      if (ws.mode !== 'grid') ws.closeOverlay()
      else if (review.selectionCount) review.clearSelection()
      else if (review.search.hits) review.clearSearch()
      break
    case 'compare': {
      const ids = compareCandidates()
      if (ids.length >= 2) ws.openCompare(ids)
      break
    }
    case 'group': {
      const gid = review.focusId ? review.photos[review.focusId]?.assessment?.groupId : null
      if (gid !== null && gid !== undefined) {
        if (!review.filters.stackGroups) review.setStackGroups(true)
        review.toggleGroup(gid)
      }
      break
    }
    case 'selectAll':
      review.selectAllVisible()
      break
    case 'search':
      ws.requestSearchFocus()
      break
    case 'export':
      app.openDialog('export')
      break
    case 'zoom':
      if (ws.mode === 'grid') {
        if (!review.focusId) break
        ws.openLoupe()
      }
      ws.setZoom(useWorkspaceStore.getState().zoom.active ? { active: false } : { active: true, ...zoomTarget(review.focusId) })
      break
    case 'undo':
      void review.undo()
      break
    case 'help':
      app.openDialog('shortcuts')
      break
    case 'winner':
      pickCompareWinner(action.index)
      break
    case 'page':
      ws.setComparePage(ws.comparePage + action.delta)
      break
  }
}

/** Global keyboard of the review workspace (Lightroom/Photo Mechanic keys). */
export function useWorkspaceKeyboard(): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return
      const app = useAppStore.getState()
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault()
        app.openDialog('settings')
        return
      }
      if (app.dialog || app.openError || app.screen !== 'workspace') return
      // ⌘+ / ⌘− : bigger or smaller thumbnails (the grid has no slider).
      if ((e.metaKey || e.ctrlKey) && !e.altKey && ['=', '+', '-'].includes(e.key) && useWorkspaceStore.getState().mode === 'grid') {
        e.preventDefault()
        const prefs = usePrefsStore.getState()
        prefs.setThumbSize(prefs.thumbSize + (e.key === '-' ? -40 : 40))
        return
      }
      if (isTypingTarget(e.target) || leftToControl(e.key, e.target as KeyTarget | null)) return
      if (document.querySelector('[data-slot="base"][role="dialog"], [role="menu"]')) return
      const action = resolveShortcut(e, useWorkspaceStore.getState().mode)
      if (!action) return
      e.preventDefault()
      run(action)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])
}
