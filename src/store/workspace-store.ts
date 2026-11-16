import { create } from 'zustand'

export type WorkspaceMode = 'grid' | 'loupe' | 'compare'

/** Normalised zoom state shared by the loupe and the compare panes. */
export interface ZoomState {
  active: boolean
  /** point of the image kept at the centre of the view, 0..1 */
  x: number
  y: number
}

interface WorkspaceState {
  mode: WorkspaceMode
  /** where the compare view was opened from: closing it goes back there */
  compareFrom: 'grid' | 'loupe'
  compareIds: string[]
  comparePage: number
  zoom: ZoomState
  /** the loupe's photo alone, the window in full screen (F); only in the loupe */
  fullscreen: boolean
  searchFocusTick: number
  openLoupe: () => void
  closeOverlay: () => void
  toggleLoupe: () => void
  openCompare: (ids: string[]) => void
  setComparePage: (page: number) => void
  setZoom: (zoom: Partial<ZoomState>) => void
  resetZoom: () => void
  /** F: enters full screen (opening the loupe from the grid) or leaves it */
  toggleFullscreen: () => void
  /** leaves full screen (Esc, or the window left it: the green button) */
  exitFullscreen: () => void
  requestSearchFocus: () => void
  reset: () => void
}

const NO_ZOOM: ZoomState = { active: false, x: 0.5, y: 0.5 }

export const COMPARE_PAGE_SIZE = 4

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  mode: 'grid',
  compareFrom: 'grid',
  compareIds: [],
  comparePage: 0,
  zoom: NO_ZOOM,
  fullscreen: false,
  searchFocusTick: 0,
  openLoupe: () => set({ mode: 'loupe', zoom: NO_ZOOM }),
  // Compare opened from the loupe goes back to it (same photo); anything else to the grid.
  closeOverlay: () => {
    const { mode, compareFrom } = get()
    set({ mode: mode === 'compare' && compareFrom === 'loupe' ? 'loupe' : 'grid', compareFrom: 'grid', zoom: NO_ZOOM, compareIds: [], fullscreen: false })
  },
  toggleLoupe: () => set({ mode: get().mode === 'loupe' ? 'grid' : 'loupe', zoom: NO_ZOOM, fullscreen: false }),
  openCompare: (ids) => {
    if (ids.length < 2) return
    const { mode, compareFrom } = get()
    set({ mode: 'compare', compareFrom: mode === 'compare' ? compareFrom : mode, compareIds: ids, comparePage: 0, zoom: NO_ZOOM, fullscreen: false })
  },
  setComparePage: (page) => {
    const pages = Math.max(1, Math.ceil(get().compareIds.length / COMPARE_PAGE_SIZE))
    set({ comparePage: Math.min(pages - 1, Math.max(0, page)) })
  },
  setZoom: (zoom) => set({ zoom: { ...get().zoom, ...zoom } }),
  resetZoom: () => set({ zoom: NO_ZOOM }),
  toggleFullscreen: () => {
    const { mode, fullscreen } = get()
    if (fullscreen) set({ fullscreen: false })
    else if (mode === 'loupe') set({ fullscreen: true })
    else if (mode === 'grid') set({ mode: 'loupe', zoom: NO_ZOOM, fullscreen: true })
  },
  exitFullscreen: () => {
    if (get().fullscreen) set({ fullscreen: false })
  },
  requestSearchFocus: () => set({ searchFocusTick: get().searchFocusTick + 1 }),
  reset: () => set({ mode: 'grid', compareFrom: 'grid', compareIds: [], comparePage: 0, zoom: NO_ZOOM, fullscreen: false }),
}))
