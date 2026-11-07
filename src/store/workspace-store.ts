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
  searchFocusTick: number
  openLoupe: () => void
  closeOverlay: () => void
  toggleLoupe: () => void
  openCompare: (ids: string[]) => void
  setComparePage: (page: number) => void
  setZoom: (zoom: Partial<ZoomState>) => void
  resetZoom: () => void
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
  searchFocusTick: 0,
  openLoupe: () => set({ mode: 'loupe', zoom: NO_ZOOM }),
  // Compare opened from the loupe goes back to it (same photo); anything else to the grid.
  closeOverlay: () => {
    const { mode, compareFrom } = get()
    set({ mode: mode === 'compare' && compareFrom === 'loupe' ? 'loupe' : 'grid', compareFrom: 'grid', zoom: NO_ZOOM, compareIds: [] })
  },
  toggleLoupe: () => set({ mode: get().mode === 'loupe' ? 'grid' : 'loupe', zoom: NO_ZOOM }),
  openCompare: (ids) => {
    if (ids.length < 2) return
    const { mode, compareFrom } = get()
    set({ mode: 'compare', compareFrom: mode === 'compare' ? compareFrom : mode, compareIds: ids, comparePage: 0, zoom: NO_ZOOM })
  },
  setComparePage: (page) => {
    const pages = Math.max(1, Math.ceil(get().compareIds.length / COMPARE_PAGE_SIZE))
    set({ comparePage: Math.min(pages - 1, Math.max(0, page)) })
  },
  setZoom: (zoom) => set({ zoom: { ...get().zoom, ...zoom } }),
  resetZoom: () => set({ zoom: NO_ZOOM }),
  requestSearchFocus: () => set({ searchFocusTick: get().searchFocusTick + 1 }),
  reset: () => set({ mode: 'grid', compareFrom: 'grid', compareIds: [], comparePage: 0, zoom: NO_ZOOM }),
}))
