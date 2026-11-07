import { beforeEach, describe, expect, it } from 'vitest'
import { useWorkspaceStore } from './workspace-store'

describe('workspace store', () => {
  beforeEach(() => useWorkspaceStore.getState().reset())

  it('closes the compare view back to the loupe it was opened from', () => {
    const ws = useWorkspaceStore.getState()
    ws.openLoupe()
    ws.openCompare(['a', 'b'])
    expect(useWorkspaceStore.getState().mode).toBe('compare')
    useWorkspaceStore.getState().closeOverlay()
    expect(useWorkspaceStore.getState().mode).toBe('loupe')
    // Then the loupe closes to the grid, as always.
    useWorkspaceStore.getState().closeOverlay()
    expect(useWorkspaceStore.getState().mode).toBe('grid')
  })

  it('closes the compare view opened from the grid back to the grid', () => {
    useWorkspaceStore.getState().openCompare(['a', 'b', 'c'])
    useWorkspaceStore.getState().closeOverlay()
    expect(useWorkspaceStore.getState()).toMatchObject({ mode: 'grid', compareIds: [] })
  })

  it('remembers the origin when another comparison opens from the compare view', () => {
    useWorkspaceStore.getState().openLoupe()
    useWorkspaceStore.getState().openCompare(['a', 'b'])
    useWorkspaceStore.getState().openCompare(['c', 'd'])
    useWorkspaceStore.getState().closeOverlay()
    expect(useWorkspaceStore.getState().mode).toBe('loupe')
  })
})
