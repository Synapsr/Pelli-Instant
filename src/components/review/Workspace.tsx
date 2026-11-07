import { Skeleton } from '@heroui/react'
import { lazy, Suspense } from 'react'
import { useWorkspaceKeyboard } from '@/hooks/use-workspace-keyboard'
import { useAppStore } from '@/store/app-store'
import { useReviewStore } from '@/store/review-store'
import { useWorkspaceStore } from '@/store/workspace-store'
import { EmptyGrid } from './EmptyGrid'
import { MarkFlash } from './MarkFlash'
import { PhotoGrid } from './PhotoGrid'
import { ReasonChips } from './ReasonChips'
import { TitleBar } from './TitleBar'

const Loupe = lazy(() => import('@/components/loupe/Loupe').then((m) => ({ default: m.Loupe })))
const CompareView = lazy(() => import('@/components/compare/CompareView').then((m) => ({ default: m.CompareView })))

function GridSkeleton() {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3 p-5">
      {Array.from({ length: 18 }, (_, i) => (
        <Skeleton key={i} className="aspect-[4/3] rounded-xl" />
      ))}
    </div>
  )
}

function GridArea() {
  const loading = useAppStore((s) => s.loadingProject)
  const empty = useReviewStore((s) => s.visibleIds.length === 0)
  if (loading) return <GridSkeleton />
  if (empty) return <EmptyGrid />
  return <PhotoGrid />
}

/**
 * The review: one bar (decision tabs, search, sensitivity, export) and the
 * photos. Neutral grey around the photos so colours are judged without a cast.
 */
export function Workspace() {
  useWorkspaceKeyboard()
  const mode = useWorkspaceStore((s) => s.mode)

  return (
    <div className="flex h-full flex-col bg-[#121212] animate-fade-in">
      <TitleBar />
      <main className="flex min-h-0 min-w-0 flex-1 flex-col">
        {mode === 'grid' ? (
          <>
            <ReasonChips />
            <div className="relative min-h-0 flex-1">
              <GridArea />
              <MarkFlash />
            </div>
          </>
        ) : (
          <Suspense fallback={<div className="flex-1 bg-[var(--loupe-bg)]" />}>{mode === 'loupe' ? <Loupe /> : <CompareView />}</Suspense>
        )}
      </main>
    </div>
  )
}
