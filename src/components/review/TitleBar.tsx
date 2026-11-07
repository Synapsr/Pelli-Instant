import { Button, Dropdown, DropdownItem, DropdownMenu, DropdownTrigger, Tooltip } from '@heroui/react'
import { AnimatePresence, motion } from 'framer-motion'
import { ChevronLeft, Ellipsis, FolderSearch, Keyboard, Microscope, RefreshCw, Search, Share } from 'lucide-react'
import { useEffect, useState } from 'react'
import { SegmentedControl } from '@/components/ui/SegmentedControl'
import { WindowBar } from '@/components/ui/WindowBar'
import { UpdateNotice } from '@/components/update/UpdateNotice'
import { useMediaQuery } from '@/hooks/use-media-query'
import { api } from '@/lib/api'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { isMac, modLabel } from '@/lib/platform'
import type { FilterTab } from '@/lib/review/view'
import { DecisionDot } from '@/components/ui/Decision'
import { useAppStore } from '@/store/app-store'
import { useReviewStore } from '@/store/review-store'
import { useWorkspaceStore } from '@/store/workspace-store'
import { SearchField } from './SearchField'
import { SensitivityPopover } from './SensitivityPopover'

/** "All" first: the whole shoot, then its three piles. */
const TABS: ('all' | 'selected' | 'toReview' | 'rejected')[] = ['all', 'selected', 'toReview', 'rejected']

/** AI search folded into an icon until it is used (⌘F opens it). */
function SearchToggle() {
  const t = useT()
  const active = useReviewStore((s) => s.search.hits !== null || s.search.status === 'loading' || s.search.query !== '')
  const focusTick = useWorkspaceStore((s) => s.searchFocusTick)
  const [open, setOpen] = useState(false)
  useEffect(() => {
    if (focusTick > 0) setOpen(true)
  }, [focusTick])
  const shown = open || active
  return (
    <AnimatePresence initial={false} mode="wait">
      {shown ? (
        <motion.div
          key="field"
          initial={{ opacity: 0, width: 40 }}
          animate={{ opacity: 1, width: 248 }}
          exit={{ opacity: 0, width: 40 }}
          transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
          onBlur={(e) => {
            if (!active && !e.currentTarget.contains(e.relatedTarget as Node)) setOpen(false)
          }}
        >
          <SearchField autoFocus={open && !active} />
        </motion.div>
      ) : (
        <Tooltip key="icon" content={`${t('review.search.badge')} (${modLabel()} F)`} delay={400} closeDelay={0}>
          <Button
            isIconOnly
            size="sm"
            variant="light"
            radius="full"
            aria-label={t('review.search.badge')}
            onPress={() => {
              setOpen(true)
              useWorkspaceStore.getState().requestSearchFocus()
            }}
            className="h-8 w-8 min-w-8 text-default-500 hover:text-foreground"
          >
            <Search className="size-4" />
          </Button>
        </Tooltip>
      )}
    </AnimatePresence>
  )
}

/** The workspace bar, in the window title area: back, decision tabs, tools, export. */
export function TitleBar() {
  const t = useT()
  const project = useReviewStore((s) => s.project)
  const refine = useAppStore((s) => s.settings?.precision === 'maximum' && s.appInfo?.capabilities.maximum === true) && project?.precision === 'standard'
  const tab = useReviewStore((s) => s.filters.tab)
  const counts = useReviewStore((s) => s.tabCounts)
  const total = useReviewStore((s) => s.dateOrder.length)
  const setTab = useReviewStore((s) => s.setTab)
  const goHome = useAppStore((s) => s.goHome)
  const openDialog = useAppStore((s) => s.openDialog)
  const current: FilterTab = TABS.includes(tab as (typeof TABS)[number]) ? tab : 'all'
  // A narrow window keeps the tabs clear of the tools: no dots, no count on "all".
  const roomy = useMediaQuery('(min-width: 1280px)')

  return (
    <WindowBar className="border-b border-white/[0.06] bg-[#161616]">
      <Tooltip content={t('review.backToProjects')} delay={400} closeDelay={0}>
        <Button isIconOnly size="sm" variant="light" radius="full" aria-label={t('review.backToProjects')} onPress={goHome} className="h-8 w-8 min-w-8 text-default-500 hover:text-foreground">
          <ChevronLeft className="size-[18px]" />
        </Button>
      </Tooltip>
      <div className="min-w-0 max-w-[260px] pl-0.5" data-tauri-drag-region>
        <h1 className="truncate text-[13px] font-semibold leading-tight text-foreground" data-tauri-drag-region>
          {project?.name}
        </h1>
        <p className="tabular truncate text-[11px] leading-tight text-default-400" data-tauri-drag-region>
          {t('common.photos', { count: total })}
        </p>
      </div>

      <div className="pointer-events-none absolute inset-x-0 flex justify-center" data-tauri-drag-region>
        <SegmentedControl
          className="pointer-events-auto"
          label={t('review.tabsLabel')}
          value={current}
          onChange={(value) => setTab(value)}
          size="sm"
          options={TABS.map((value) => ({
            value,
            label:
              value === 'all' ? (
                t('review.tabs.all')
              ) : (
                <span className="inline-flex items-center gap-1.5">
                  {roomy && <DecisionDot decision={value} size={6} />}
                  {t(`review.tabs.${value}`)}
                </span>
              ),
            meta: value === 'all' && !roomy ? undefined : formatNumber(counts[value]),
          }))}
        />
      </div>

      <div className="relative ml-auto flex items-center gap-1">
        <UpdateNotice compact />
        <SearchToggle />
        <SensitivityPopover />
        <Dropdown placement="bottom-end">
          <DropdownTrigger>
            <Button isIconOnly size="sm" variant="light" radius="full" aria-label={t('review.moreActions')} className="h-8 w-8 min-w-8 text-default-500 hover:text-foreground">
              <Ellipsis className="size-4" />
            </Button>
          </DropdownTrigger>
          <DropdownMenu aria-label={t('review.moreActions')} variant="flat">
            {/* Analysed in Standard while Maximum is chosen and installed: the rerun is the finer pass. */}
            <DropdownItem
              key="rerun"
              startContent={refine ? <Microscope className="size-4" /> : <RefreshCw className="size-4" />}
              onPress={() => project && void useAppStore.getState().analyze(project)}
            >
              {refine ? t('analysis.rerunMaximum') : t('analysis.rerun')}
            </DropdownItem>
            <DropdownItem key="reveal" startContent={<FolderSearch className="size-4" />} onPress={() => project && void api().revealInFolder(project.rootPath)}>
              {isMac() ? t('common.revealInFinder') : t('common.revealInExplorer')}
            </DropdownItem>
            <DropdownItem key="shortcuts" startContent={<Keyboard className="size-4" />} shortcut="?" onPress={() => openDialog('shortcuts')}>
              {t('review.shortcuts')}
            </DropdownItem>
          </DropdownMenu>
        </Dropdown>
        <Button
          size="sm"
          radius="full"
          className="btn-brand ml-1.5 h-8 gap-1.5 px-4 text-[13px] font-semibold"
          startContent={<Share className="size-3.5" />}
          onPress={() => openDialog('export')}
          title={`${t('review.export')} (${modLabel()} E)`}
        >
          {t('review.export')}
        </Button>
      </div>
    </WindowBar>
  )
}
