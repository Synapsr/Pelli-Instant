import { Modal, ModalBody, ModalContent, ModalHeader } from '@heroui/react'
import { Toggle } from '@/components/ui/Toggle'
import { Keyboard } from 'lucide-react'
import type { ReactNode } from 'react'
import { IconBox } from '@/components/ui/IconBox'
import { useT, type TKey } from '@/lib/i18n'
import { modLabel } from '@/lib/platform'
import { useAppStore } from '@/store/app-store'
import { usePrefsStore } from '@/store/prefs-store'

function Key({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-flex h-6 min-w-6 items-center justify-center rounded-md border border-default-200 bg-default-50 px-1.5 font-sans text-[11px] font-semibold text-default-600 shadow-[0_1px_0_rgba(0,0,0,0.06)] dark:border-white/10 dark:bg-white/[0.04] dark:text-default-600">
      {children}
    </kbd>
  )
}

type Row = { label: TKey; keys: ReactNode[]; alt?: ReactNode[] }

/** "?" overlay listing every shortcut of the workspace. */
export function ShortcutsDialog() {
  const t = useT()
  const open = useAppStore((s) => s.dialog === 'shortcuts')
  const close = useAppStore((s) => s.closeDialog)
  const autoAdvance = usePrefsStore((s) => s.autoAdvance)
  const setAutoAdvance = usePrefsStore((s) => s.setAutoAdvance)
  const mod = modLabel()

  const groups: { title: TKey; rows: Row[] }[] = [
    {
      title: 'shortcuts.navigation',
      rows: [
        { label: 'shortcuts.navigate', keys: ['←', '→'] },
        { label: 'shortcuts.rows', keys: ['↑', '↓'] },
        { label: 'shortcuts.selectAll', keys: [mod, 'A'] },
        { label: 'shortcuts.search', keys: [mod, 'F'] },
        { label: 'shortcuts.escape', keys: ['Esc'] },
      ],
    },
    {
      title: 'shortcuts.marking',
      rows: [
        { label: 'shortcuts.pick', keys: ['↑'], alt: ['P'] },
        { label: 'shortcuts.reject', keys: ['↓'], alt: ['X'] },
        { label: 'shortcuts.unflag', keys: ['U'] },
        { label: 'shortcuts.stars', keys: ['0', '…', '5'] },
        { label: 'shortcuts.labels', keys: ['6', '7', '8', '9'] },
        { label: 'shortcuts.undo', keys: [mod, 'Z'] },
      ],
    },
    {
      title: 'shortcuts.views',
      rows: [
        { label: 'shortcuts.loupe', keys: [t('shortcuts.space')] },
        { label: 'shortcuts.zoom', keys: ['Z'] },
        { label: 'shortcuts.compare', keys: ['C'] },
        { label: 'shortcuts.group', keys: ['G'] },
        { label: 'shortcuts.export', keys: [mod, 'E'] },
        { label: 'shortcuts.help', keys: ['?'] },
      ],
    },
  ]

  return (
    <Modal isOpen={open} onClose={close} size="3xl" radius="lg" backdrop="blur" classNames={{ base: 'glass-strong rounded-[24px]', backdrop: 'bg-black/50' }}>
      <ModalContent>
        <ModalHeader className="flex items-center gap-3 pb-2 pt-5">
          <IconBox icon={Keyboard} size="sm" />
          <span className="text-base font-semibold">{t('shortcuts.title')}</span>
        </ModalHeader>
        <ModalBody className="pb-6">
          <div className="grid grid-cols-3 gap-6">
            {groups.map((g) => (
              <section key={g.title}>
                <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-default-400">{t(g.title)}</h3>
                <ul className="space-y-1">
                  {g.rows.map((r) => (
                    <li key={r.label} className="flex items-center justify-between gap-3 rounded-lg py-1">
                      <span className="text-[13px] text-default-700">{t(r.label)}</span>
                      <span className="flex shrink-0 items-center gap-1">
                        {r.keys.map((k, i) => (
                          <Key key={i}>{k}</Key>
                        ))}
                        {r.alt && <span className="px-0.5 text-[11px] text-default-400">{t('home.or')}</span>}
                        {r.alt?.map((k, i) => (
                          <Key key={`alt${i}`}>{k}</Key>
                        ))}
                      </span>
                    </li>
                  ))}
                </ul>
                {g.title === 'shortcuts.marking' && <p className="mt-2 text-pretty text-[11px] leading-snug text-default-400">{t('shortcuts.arrowsHint')}</p>}
              </section>
            ))}
          </div>
          <div className="mt-4 flex items-center justify-between rounded-xl border border-[#5840f0]/10 bg-gradient-to-r from-[#5840f0]/[0.04] to-transparent px-4 py-3 dark:border-[#5840f0]/20 dark:from-[#5840f0]/[0.1]">
            <span className="text-[13px] text-default-700">{t('shortcuts.autoAdvance')}</span>
            <Toggle isSelected={autoAdvance} onValueChange={setAutoAdvance} aria-label={t('shortcuts.autoAdvance')} />
          </div>
        </ModalBody>
      </ModalContent>
    </Modal>
  )
}
