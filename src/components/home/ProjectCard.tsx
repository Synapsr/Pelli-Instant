import { Dropdown, DropdownItem, DropdownMenu, DropdownTrigger } from '@heroui/react'
import { motion } from 'framer-motion'
import { Ellipsis, FolderSearch, ImageIcon, Trash2 } from 'lucide-react'
import { memo, useState } from 'react'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { DECISION_BG } from '@/components/ui/Decision'
import type { FinalDecision, ProjectSummary } from '@/lib/contract'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { formatNumber, formatRelative } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { isMac } from '@/lib/platform'
import { useAppStore } from '@/store/app-store'

function Count({ decision, value, label }: { decision: FinalDecision; value: number; label: string }) {
  return (
    <span className="inline-flex items-center gap-1" title={label}>
      <span className={cn('size-1.5 rounded-full', DECISION_BG[decision])} />
      <span className="tabular text-[11px] text-white/60">{formatNumber(value)}</span>
    </span>
  )
}

/** A recent shoot on the home screen: cover, name, sorting at a glance. */
export const ProjectCard = memo(function ProjectCard({ project, index = 0, delay = 0 }: { project: ProjectSummary; index?: number; delay?: number }) {
  const t = useT()
  const openProject = useAppStore((s) => s.openProject)
  const removeProject = useAppStore((s) => s.removeProject)
  const [confirm, setConfirm] = useState(false)
  const analyzed = project.analyzedAt !== null

  return (
    <motion.article
      initial={{ opacity: 0, y: 12, filter: 'blur(4px)' }}
      animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
      transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1], delay: delay + 0.3 + index * 0.08 }}
      className="glass glass-hover group relative w-[208px] shrink-0 overflow-hidden rounded-2xl hover:-translate-y-0.5"
    >
      <button type="button" onClick={() => void openProject(project)} className="focus-ring absolute inset-0 z-10 rounded-2xl" aria-label={`${t('home.open')} ${project.name}`} />
      <div className="relative m-1.5 aspect-[16/10] overflow-hidden rounded-[10px] bg-white/[0.04] outline outline-1 -outline-offset-1 outline-white/10">
        {project.coverThumbUrl ? (
          <img src={project.coverThumbUrl} alt="" draggable={false} className="h-full w-full object-cover transition-transform duration-500 ease-out group-hover:scale-[1.04]" />
        ) : (
          <div className="flex h-full items-center justify-center text-white/20">
            <ImageIcon className="size-7" strokeWidth={1.4} />
          </div>
        )}
        <div className="absolute right-1.5 top-1.5 z-20 opacity-0 transition-opacity duration-200 group-focus-within:opacity-100 group-hover:opacity-100">
          <Dropdown placement="bottom-end">
            <DropdownTrigger>
              <button type="button" aria-label={t('home.projectMenu')} className="focus-ring flex size-7 items-center justify-center rounded-lg bg-black/55 text-white backdrop-blur-md hover:bg-black/75">
                <Ellipsis className="size-4" />
              </button>
            </DropdownTrigger>
            <DropdownMenu aria-label={t('home.projectMenu')} variant="flat">
              <DropdownItem key="reveal" startContent={<FolderSearch className="size-4" />} onPress={() => void api().revealInFolder(project.rootPath)}>
                {isMac() ? t('common.revealInFinder') : t('common.revealInExplorer')}
              </DropdownItem>
              <DropdownItem key="forget" startContent={<Trash2 className="size-4" />} className="text-danger" color="danger" onPress={() => setConfirm(true)}>
                {t('home.forget')}
              </DropdownItem>
            </DropdownMenu>
          </Dropdown>
        </div>
      </div>
      <div className="px-3 pb-3 pt-1.5 text-left">
        <h3 className="truncate text-[13px] font-semibold text-white">{project.name}</h3>
        <div className="mt-1 flex items-center justify-between gap-2">
          <span className="truncate text-[11px] text-white/40">
            {t('common.photos', { count: project.photoCount })} · {formatRelative(project.lastOpenedAt)}
          </span>
          {analyzed && (
            <span className="flex shrink-0 items-center gap-2">
              <Count decision="selected" value={project.counts.selected} label={t('review.tabs.selected')} />
              <Count decision="toReview" value={project.counts.toReview} label={t('review.tabs.toReview')} />
            </span>
          )}
        </div>
      </div>
      <ConfirmDialog
        open={confirm}
        title={t('home.forgetTitle', { name: project.name })}
        body={t('home.forgetBody')}
        confirmLabel={t('common.remove')}
        danger
        onConfirm={() => void removeProject(project.id)}
        onClose={() => setConfirm(false)}
      />
    </motion.article>
  )
})
