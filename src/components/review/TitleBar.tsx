import { Button, Dropdown, DropdownItem, DropdownMenu, DropdownTrigger, Tooltip } from '@heroui/react'
import { AnimatePresence, motion } from 'framer-motion'
import { ChevronLeft, Ellipsis, FolderSearch, Keyboard, Lock, Microscope, RefreshCw, Search, Share } from 'lucide-react'
import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react'
import { UploadPill } from '@/components/export/UploadPill'
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
import { usePlusStore } from '@/store/plus-store'
import { useReviewStore } from '@/store/review-store'
import { useWorkspaceStore } from '@/store/workspace-store'
import { SearchField } from './SearchField'
import { SensitivityPopover } from './SensitivityPopover'

/** "All" first: the whole shoot, then its three piles. */
const TABS: ('all' | 'selected' | 'toReview' | 'rejected')[] = ['all', 'selected', 'toReview', 'rejected']

const EASE = [0.22, 1, 0.36, 1] as const

/**
 * AI search is part of Plus: without it, the search icon carries a small
 * lock in its corner and opens the Plus dialog (⌘F too).
 */
function LockedSearch() {
  const t = useT()
  const openDialog = usePlusStore((s) => s.openDialog)
  const focusTick = useWorkspaceStore((s) => s.searchFocusTick)
  const seen = useRef(focusTick)
  useEffect(() => {
    if (focusTick === seen.current) return
    seen.current = focusTick
    openDialog('search')
  }, [focusTick, openDialog])
  return (
    <Tooltip content={t('review.search.locked')} delay={300} closeDelay={0}>
      <Button
        isIconOnly
        size="sm"
        variant="light"
        radius="full"
        aria-label={t('review.search.locked')}
        onPress={() => openDialog('search')}
        className="relative h-8 w-8 min-w-8 text-default-500 hover:text-foreground"
      >
        <Search className="size-4" />
        <span aria-hidden className="absolute right-[3px] top-[3px] flex size-3 items-center justify-center rounded-full bg-[#5840f0] text-white ring-2 ring-[#161616]">
          <Lock className="size-[7px]" strokeWidth={3} />
        </span>
      </Button>
    </Tooltip>
  )
}

/** The room between `start` and the centred tabs, so nothing on the left runs under them. */
function useRoomBeforeTabs(start: RefObject<HTMLElement | null>, tabs: RefObject<HTMLElement | null>): number {
  const [room, setRoom] = useState(260)
  useLayoutEffect(() => {
    const measure = () => {
      const from = start.current?.getBoundingClientRect()
      const to = tabs.current?.getBoundingClientRect()
      if (from && to) setRoom(Math.floor(to.left - from.left - 16))
    }
    measure()
    const observer = new ResizeObserver(measure)
    if (tabs.current) observer.observe(tabs.current)
    window.addEventListener('resize', measure)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', measure)
    }
  }, [start, tabs])
  return room
}

/**
 * The project name, and the AI search next to it. Opened (⌘F or the icon),
 * the field takes the place of the name, never wider than the room left of
 * the tabs; it stays open while a search is shown.
 */
function TitleAndSearch({ tabs, plus }: { tabs: RefObject<HTMLDivElement | null>; plus: boolean }) {
  const t = useT()
  const project = useReviewStore((s) => s.project)
  const total = useReviewStore((s) => s.dateOrder.length)
  const active = useReviewStore((s) => s.search.hits !== null || s.search.status === 'loading' || s.search.query !== '')
  const focusTick = useWorkspaceStore((s) => s.searchFocusTick)
  const seen = useRef(focusTick)
  const [open, setOpen] = useState(false)
  const box = useRef<HTMLDivElement>(null)
  const room = useRoomBeforeTabs(box, tabs)
  useEffect(() => {
    if (focusTick === seen.current) return
    seen.current = focusTick
    if (plus) setOpen(true)
  }, [focusTick, plus])
  const shown = plus && (open || active)

  return (
    <div ref={box} className="flex min-w-0 items-center pl-0.5" data-tauri-drag-region>
      <AnimatePresence initial={false} mode="wait">
        {shown ? (
          <motion.div
            key="field"
            initial={{ opacity: 0, width: 32 }}
            animate={{ opacity: 1, width: Math.max(180, Math.min(300, room)) }}
            exit={{ opacity: 0, width: 32, transition: { duration: 0.15 } }}
            transition={{ duration: 0.28, ease: EASE }}
            onBlur={(e) => {
              if (!active && !e.currentTarget.contains(e.relatedTarget as Node)) setOpen(false)
            }}
          >
            <SearchField autoFocus={!active} />
          </motion.div>
        ) : (
          <motion.div
            key="title"
            className="flex min-w-0 items-center gap-1.5"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            data-tauri-drag-region
          >
            <div className="min-w-0" style={{ maxWidth: Math.max(72, Math.min(260, room - 40)) }} data-tauri-drag-region>
              <h1 className="truncate text-[13px] font-semibold leading-tight text-foreground" data-tauri-drag-region>
                {project?.name}
              </h1>
              <p className="tabular truncate text-[11px] leading-tight text-default-400" data-tauri-drag-region>
                {t('common.photos', { count: total })}
              </p>
            </div>
            {plus ? (
              <Tooltip content={`${t('review.search.badge')} (${modLabel()} F)`} delay={300} closeDelay={0}>
                <Button
                  isIconOnly
                  size="sm"
                  variant="light"
                  radius="full"
                  aria-label={t('review.search.badge')}
                  onPress={() => setOpen(true)}
                  className="h-8 w-8 min-w-8 text-default-500 hover:text-foreground"
                >
                  <Search className="size-4" />
                </Button>
              </Tooltip>
            ) : (
              <LockedSearch />
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

/** The workspace bar, in the window title area: back, decision tabs, tools, export. */
export function TitleBar() {
  const t = useT()
  const project = useReviewStore((s) => s.project)
  const plus = usePlusStore((s) => s.status.active)
  const refine = useAppStore((s) => s.settings?.precision === 'maximum' && s.appInfo?.capabilities.maximum === true) && plus && project?.precision === 'standard'
  const tab = useReviewStore((s) => s.filters.tab)
  const counts = useReviewStore((s) => s.tabCounts)
  const setTab = useReviewStore((s) => s.setTab)
  const goHome = useAppStore((s) => s.goHome)
  const openDialog = useAppStore((s) => s.openDialog)
  const current: FilterTab = TABS.includes(tab as (typeof TABS)[number]) ? tab : 'all'
  // A narrow window keeps the tabs clear of the tools: no dots, no count on "all".
  const roomy = useMediaQuery('(min-width: 1280px)')
  const tabsRef = useRef<HTMLDivElement>(null)

  return (
    <WindowBar className="border-b border-white/[0.06] bg-[#161616]">
      <Tooltip content={t('review.backToProjects')} delay={400} closeDelay={0}>
        <Button isIconOnly size="sm" variant="light" radius="full" aria-label={t('review.backToProjects')} onPress={goHome} className="h-8 w-8 min-w-8 text-default-500 hover:text-foreground">
          <ChevronLeft className="size-[18px]" />
        </Button>
      </Tooltip>
      <TitleAndSearch tabs={tabsRef} plus={plus} />

      <div className="pointer-events-none absolute inset-x-0 flex justify-center" data-tauri-drag-region>
        <div ref={tabsRef} className="pointer-events-auto">
        <SegmentedControl
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
      </div>

      <div className="relative ml-auto flex items-center gap-1">
        <UploadPill size={roomy ? 'count' : 'icon'} className="mr-1" />
        <UpdateNotice compact />
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
