import { Button, Modal, ModalBody, ModalContent } from '@heroui/react'
import { Toggle } from '@/components/ui/Toggle'
import { CreditCard, Database, ExternalLink, FolderSearch, Github, Info, LogOut, PlayCircle, RotateCcw, ScanEye, SlidersHorizontal, User, Volume2, type LucideIcon } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { SensitivityPicker } from '@/components/analysis/SensitivityPicker'
import { CriteriaList } from '@/components/criteria/CriteriaList'
import { Avatar } from '@/components/home/AccountMenu'
import { UsageMeter } from '@/components/plans/UsageMeter'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { SettingRow as Row } from '@/components/ui/SettingRow'
import { PrecisionChoice } from '@/components/precision/PrecisionChoice'
import { InstantLogo } from '@/components/ui/Logo'
import { PlanBadge } from '@/components/ui/PlanBadge'
import { UpdateSettings } from '@/components/update/UpdateSettings'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { DEFAULT_SETTINGS, type CullSettings } from '@/lib/contract'
import { normalizeCriteria, sameCriteria } from '@/lib/criteria'
import { formatBytes, formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { LanguageMenu } from './LanguageMenu'
import { LICENCES } from '@/lib/licences'
import { billingUrl, GITHUB_URL, PELLI_URL } from '@/lib/plans'
import { playSound } from '@/lib/sound'
import { isMac } from '@/lib/platform'
import { useAppStore, type SettingsTab } from '@/store/app-store'
import { usePackStore } from '@/store/pack-store'
import { usePrefsStore } from '@/store/prefs-store'

const NAV: { tab: SettingsTab; icon: LucideIcon }[] = [
  { tab: 'account', icon: User },
  { tab: 'general', icon: SlidersHorizontal },
  { tab: 'analysis', icon: ScanEye },
  { tab: 'cache', icon: Database },
  { tab: 'about', icon: Info },
]

export function SettingsDialog() {
  const open = useAppStore((s) => s.dialog === 'settings')
  const close = useAppStore((s) => s.closeDialog)
  const tab = useAppStore((s) => s.settingsTab)
  return (
    <Modal isOpen={open} onClose={close} size="4xl" radius="lg" backdrop="blur" classNames={{ base: 'glass-strong rounded-[24px] h-[min(600px,calc(100vh-48px))]', backdrop: 'bg-black/50' }}>
      <ModalContent>
        <ModalBody className="flex-row gap-0 overflow-hidden p-0">
          <SettingsNav current={tab} />
          <div className="min-w-0 flex-1 overflow-y-auto px-8 py-7">
            {tab === 'account' && <AccountPanel />}
            {tab === 'general' && <GeneralPanel />}
            {tab === 'analysis' && <AnalysisPanel />}
            {tab === 'cache' && <CachePanel />}
            {tab === 'about' && <AboutPanel />}
          </div>
        </ModalBody>
      </ModalContent>
    </Modal>
  )
}

function SettingsNav({ current }: { current: SettingsTab }) {
  const t = useT()
  // A build without account has no account tab.
  const noAccount = useAppStore((s) => s.appInfo?.noAccount ?? false)
  const nav = noAccount ? NAV.filter((n) => n.tab !== 'account') : NAV
  return (
    <nav aria-label={t('settings.title')} className="flex w-56 shrink-0 flex-col gap-0.5 border-r border-white/[0.06] bg-white/[0.02] p-3">
      <h2 className="px-2.5 pb-3 pt-2 text-[15px] font-semibold text-foreground">{t('settings.title')}</h2>
      {nav.map(({ tab, icon: Icon }) => (
        <button
          key={tab}
          type="button"
          aria-current={tab === current ? 'page' : undefined}
          onClick={() => useAppStore.setState({ settingsTab: tab })}
          className={cn(
            'focus-ring flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-[13px] transition-colors duration-200',
            tab === current
              ? 'bg-gradient-to-r from-[#5840f0]/12 to-[#5840f0]/[0.04] font-medium text-foreground ring-1 ring-[#5840f0]/15 dark:from-[#5840f0]/25 dark:to-[#5840f0]/[0.08] dark:ring-[#5840f0]/30'
              : 'text-default-600 hover:bg-default-100 dark:hover:bg-white/[0.04]',
          )}
        >
          <Icon className={cn('size-4', tab === current ? 'text-[#5840f0] dark:text-[#a599f5]' : 'text-default-400')} />
          {t(`settings.nav.${tab}`)}
        </button>
      ))}
    </nav>
  )
}

function PanelTitle({ children }: { children: ReactNode }) {
  return <h3 className="mb-2 text-lg font-semibold tracking-tight text-foreground">{children}</h3>
}

function AccountPanel() {
  const t = useT()
  const auth = useAppStore((s) => s.auth)
  const signOut = useAppStore((s) => s.signOut)
  const refreshAccount = useAppStore((s) => s.refreshAccount)
  const offers = useAppStore((s) => s.offers)
  const [confirm, setConfirm] = useState(false)
  if (auth.status !== 'signedIn') return null
  const { account } = auth
  return (
    <div>
      <PanelTitle>{t('settings.nav.account')}</PanelTitle>
      <div className="mt-4 flex items-center gap-4 rounded-2xl border border-[#5840f0]/10 bg-gradient-to-br from-[#5840f0]/[0.05] via-transparent to-[#5840f0]/[0.03] p-5 dark:border-[#5840f0]/20 dark:from-[#5840f0]/[0.1]">
        <Avatar account={account} size={52} />
        <div className="min-w-0 flex-1">
          <p className="flex items-center gap-2 text-base font-semibold text-foreground">
            {account.name}
            <PlanBadge plan={account.plan} label={account.planLabel} />
          </p>
          <p className="truncate text-[13px] text-default-500">{account.email}</p>
          <p className="truncate text-xs text-default-400">{account.slug}.pelli.io</p>
        </div>
        <Button size="sm" variant="flat" radius="lg" startContent={<CreditCard className="size-4" />} onPress={() => void api().openExternal(billingUrl(offers))}>
          {t('settings.account.manage')}
        </Button>
      </div>
      <div className="mt-3 rounded-2xl border border-white/[0.07] p-4">
        <p className="mb-3 text-[13px] font-medium text-foreground">{t('settings.account.usage')}</p>
        <UsageMeter account={account} />
      </div>
      <div className="mt-3">
        <Row label={t('settings.account.signOut')} hint={t('settings.account.signOutBody')}>
          <div className="flex gap-2">
            <Button size="sm" variant="light" radius="lg" onPress={() => void refreshAccount()}>
              {t('settings.account.refresh')}
            </Button>
            <Button size="sm" variant="flat" color="danger" radius="lg" startContent={<LogOut className="size-4" />} onPress={() => setConfirm(true)}>
              {t('settings.account.signOut')}
            </Button>
          </div>
        </Row>
      </div>
      <ConfirmDialog
        open={confirm}
        title={t('settings.account.signOutTitle')}
        body={t('settings.account.signOutBody')}
        confirmLabel={t('settings.account.signOut')}
        danger
        onConfirm={() => void signOut()}
        onClose={() => setConfirm(false)}
      />
    </div>
  )
}


function GeneralPanel() {
  const t = useT()
  const settings = useAppStore((s) => s.settings)
  const update = useAppStore((s) => s.updateSettings)
  const info = useAppStore((s) => s.appInfo)
  const prefs = usePrefsStore()
  if (!settings) return null
  // Memory is sold in binary gigabytes: 16 GiB reads "16 Go".
  const machine = info?.memoryBytes
    ? t('settings.general.machine', {
        cores: info.cpuCores,
        memory: formatNumber(Math.round(info.memoryBytes / 1024 ** 3), undefined, { style: 'unit', unit: 'gigabyte', unitDisplay: 'short' }),
      })
    : info
      ? t('settings.general.machineCores', { cores: info.cpuCores })
      : null
  return (
    <div>
      <PanelTitle>{t('settings.nav.general')}</PanelTitle>
      <Row
        label={t('settings.general.background.title')}
        hint={
          <>
            {t('settings.general.background.hint')}
            {machine && <span className="mt-1 block text-[11px] text-default-400">{machine}</span>}
          </>
        }
      >
        <Toggle isSelected={settings.background} onValueChange={(background) => void update({ background })} aria-label={t('settings.general.background.title')} />
      </Row>
      <Row label={t('settings.appearance.language')}>
        <LanguageMenu label={t('settings.appearance.language')} value={settings.language} onChange={(language) => void update({ language })} />
      </Row>
      <Row label={t('settings.general.sounds')} hint={t('settings.general.soundsHint')}>
        <div className="flex items-center gap-3">
          <Button size="sm" variant="light" radius="lg" startContent={<Volume2 className="size-4" />} onPress={() => playSound('analysisDone')} isDisabled={!prefs.sounds}>
            {t('settings.general.soundsTry')}
          </Button>
          <Toggle isSelected={prefs.sounds} onValueChange={prefs.setSounds} aria-label={t('settings.general.sounds')} />
        </div>
      </Row>
      <Row label={t('settings.appearance.autoAdvance')} hint={t('settings.appearance.autoAdvanceHint')}>
        <Toggle isSelected={prefs.autoAdvance} onValueChange={prefs.setAutoAdvance} aria-label={t('settings.appearance.autoAdvance')} />
      </Row>
      <Row label={t('home.includeSubfolders')} hint={t('settings.general.subfoldersHint')}>
        <Toggle isSelected={prefs.includeSubfolders} onValueChange={prefs.setIncludeSubfolders} aria-label={t('home.includeSubfolders')} />
      </Row>
    </div>
  )
}

/** Default criteria: what the AI checks in every new shoot. */
function AnalysisPanel() {
  const t = useT()
  const stored = useAppStore((s) => s.settings?.cull)
  const update = useAppStore((s) => s.updateSettings)
  const pack = usePackStore((s) => s.pack)
  if (!stored) return null
  const cull = normalizeCriteria(stored)
  const change = (patch: Partial<CullSettings>) => void update({ cull: { ...cull, ...patch } })
  const factory = sameCriteria(cull, DEFAULT_SETTINGS)
  return (
    <div>
      <PanelTitle>{t('settings.nav.analysis')}</PanelTitle>
      <p className="mb-5 max-w-[520px] text-pretty text-[13px] leading-snug text-default-500">{t('settings.analysis.intro')}</p>
      {pack && (
        <div className="mb-6">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-default-400">{t('settings.analysis.precision.title')}</h4>
          <PrecisionChoice />
        </div>
      )}
      <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-default-400">{t('preAnalysis.sensitivity')}</h4>
      <SensitivityPicker value={cull.sensitivity} onChange={(sensitivity) => change({ sensitivity })} />
      <div className="mt-4">
        <CriteriaList value={cull} onChange={change} />
      </div>
      <div className="mt-4 flex justify-end border-t border-default-200/60 pt-4 dark:border-white/[0.06]">
        <Button size="sm" variant="flat" radius="lg" startContent={<RotateCcw className="size-3.5" />} isDisabled={factory} onPress={() => void update({ cull: { ...DEFAULT_SETTINGS } })}>
          {t('settings.analysis.reset')}
        </Button>
      </div>
    </div>
  )
}

function CachePanel() {
  const t = useT()
  const settings = useAppStore((s) => s.settings)
  const clearCache = useAppStore((s) => s.clearCache)
  const [confirm, setConfirm] = useState(false)
  const [clearing, setClearing] = useState(false)
  if (!settings) return null
  return (
    <div>
      <PanelTitle>{t('settings.nav.cache')}</PanelTitle>
      <Row label={t('settings.cache.location')} hint={<span className="selectable font-mono text-[11px]">{settings.cacheDir}</span>}>
        <Button size="sm" variant="light" radius="lg" startContent={<FolderSearch className="size-4" />} onPress={() => void api().revealInFolder(settings.cacheDir)}>
          {isMac() ? t('common.revealInFinder') : t('common.revealInExplorer')}
        </Button>
      </Row>
      <Row label={t('settings.cache.size')} hint={t('settings.cache.clearHint')}>
        <div className="flex items-center gap-3">
          <span className="tabular text-sm font-semibold text-foreground">{formatBytes(settings.cacheSizeBytes)}</span>
          <Button size="sm" variant="flat" radius="lg" isLoading={clearing} isDisabled={settings.cacheSizeBytes === 0} onPress={() => setConfirm(true)}>
            {t('settings.cache.clear')}
          </Button>
        </div>
      </Row>
      <ConfirmDialog
        open={confirm}
        title={t('settings.cache.clear')}
        body={t('settings.cache.clearHint')}
        confirmLabel={t('settings.cache.clear')}
        onConfirm={() => {
          setClearing(true)
          void clearCache().finally(() => setClearing(false))
        }}
        onClose={() => setConfirm(false)}
      />
    </div>
  )
}

function AboutPanel() {
  const t = useT()
  const info = useAppStore((s) => s.appInfo)
  const startOnboarding = useAppStore((s) => s.startOnboarding)
  if (!info) return null
  const platform = { macos: 'macOS', windows: 'Windows', linux: 'Linux', web: 'Web' }[info.platform]
  return (
    <div>
      <div className="flex items-center justify-between">
        <InstantLogo size="md" />
        <Button size="sm" variant="light" radius="lg" endContent={<ExternalLink className="size-3.5" />} onPress={() => void api().openExternal(PELLI_URL)}>
          {t('settings.about.website')}
        </Button>
      </div>
      <div className="mt-5 rounded-2xl border border-[#5840f0]/20 bg-[#5840f0]/[0.08] p-4">
        <p className="text-pretty text-[13px] leading-snug text-default-700">{t('settings.about.openSource')}</p>
        <div className="mt-3 flex flex-wrap gap-2">
          <Button size="sm" variant="flat" radius="lg" startContent={<Github className="size-4" />} onPress={() => void api().openExternal(GITHUB_URL)}>
            {t('settings.about.github')}
          </Button>
          <Button size="sm" variant="light" radius="lg" startContent={<PlayCircle className="size-4" />} onPress={startOnboarding}>
            {t('settings.about.replayTour')}
          </Button>
        </div>
      </div>
      <dl className="mt-6 grid grid-cols-3 gap-4 rounded-2xl border border-default-200/70 p-4 dark:border-white/[0.07]">
        <div>
          <dt className="text-[11px] text-default-400">{t('settings.about.version')}</dt>
          <dd className="tabular text-[13px] font-medium text-foreground">{info.version}</dd>
        </div>
        <div>
          <dt className="text-[11px] text-default-400">{t('settings.about.platform')}</dt>
          <dd className="text-[13px] font-medium text-foreground">
            {platform} · {info.arch}
          </dd>
        </div>
        <div>
          <dt className="text-[11px] text-default-400">{t('settings.about.accelerator')}</dt>
          <dd className="text-[13px] font-medium text-foreground">{info.accelerator}</dd>
        </div>
      </dl>
      <div className="mt-3 rounded-2xl border border-default-200/70 px-4 dark:border-white/[0.07]">
        <UpdateSettings />
      </div>
      <h4 className="mt-7 text-[13px] font-semibold text-foreground">{t('settings.about.licences')}</h4>
      <p className="mt-0.5 text-xs text-default-500">{t('settings.about.licencesHint')}</p>
      <ul className="mt-3 divide-y divide-default-200/60 overflow-hidden rounded-xl border border-default-200/70 dark:divide-white/[0.05] dark:border-white/[0.07]">
        {LICENCES.map((l) => (
          <li key={l.name}>
            <button type="button" onClick={() => void api().openExternal(l.url)} className="focus-ring flex w-full items-center gap-3 px-3.5 py-2.5 text-left transition-colors hover:bg-default-50 dark:hover:bg-white/[0.03]">
              <span className="min-w-0 flex-1">
                <span className="block text-[13px] font-medium text-foreground">{l.name}</span>
                <span className="block text-[11px] text-default-500">{l.author}</span>
              </span>
              <span className="rounded-md bg-default-100 px-1.5 py-0.5 font-mono text-[10px] text-default-600 dark:bg-white/[0.06]">{l.licence}</span>
            </button>
          </li>
        ))}
      </ul>
      <p className="mt-5 text-center text-xs text-default-400">{t('settings.about.madeIn')}</p>
    </div>
  )
}
