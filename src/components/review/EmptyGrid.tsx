import { Button } from '@heroui/react'
import { CircleCheck, Copy, Flag, FilterX, ImageOff, Loader, PartyPopper, SearchX } from 'lucide-react'
import { EmptyState } from '@/components/ui/EmptyState'
import { useT } from '@/lib/i18n'
import { useAnalysisStore } from '@/store/analysis-store'
import { useReviewStore } from '@/store/review-store'

/** Empty grid: per tab, per filter, per search, or photos still coming. */
export function EmptyGrid() {
  const t = useT()
  const tab = useReviewStore((s) => s.filters.tab)
  const filters = useReviewStore((s) => s.filters)
  const search = useReviewStore((s) => s.search)
  const clearFilters = useReviewStore((s) => s.clearFilters)
  const running = useAnalysisStore((s) => s.running)
  const filtered = filters.sceneId !== null || filters.reason !== null || filters.camera !== null

  const content = (() => {
    if (search.hits) return { icon: SearchX, title: t('review.search.none', { query: search.query }), body: undefined }
    if (filtered) return { icon: FilterX, title: t('review.empty.filtered'), body: undefined }
    if (running && tab !== 'all') return { icon: Loader, title: t('review.empty.pending'), body: undefined }
    switch (tab) {
      case 'selected':
        return { icon: Flag, title: t('review.empty.selected'), body: t('review.empty.selectedHint') }
      case 'toReview':
        return { icon: PartyPopper, title: t('review.empty.toReview'), body: t('review.empty.toReviewHint') }
      case 'rejected':
        return { icon: CircleCheck, title: t('review.empty.rejected'), body: t('review.empty.rejectedHint') }
      case 'duplicates':
        return { icon: Copy, title: t('review.empty.duplicates'), body: t('review.empty.duplicatesHint') }
      default:
        return { icon: ImageOff, title: t('review.empty.all'), body: undefined }
    }
  })()

  return (
    <div className="flex h-full items-center justify-center p-8">
      <EmptyState
        icon={content.icon}
        title={content.title}
        body={content.body}
        action={
          filtered || search.hits ? (
            <Button variant="flat" className="bg-[#5840f0]/10 text-[#5840f0] hover:bg-[#5840f0]/20 dark:text-[#a599f5]" onPress={clearFilters}>
              {t('review.empty.clearFilters')}
            </Button>
          ) : undefined
        }
      />
    </div>
  )
}
