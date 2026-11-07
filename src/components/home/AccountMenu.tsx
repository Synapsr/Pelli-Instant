import { Dropdown, DropdownItem, DropdownMenu, DropdownSection, DropdownTrigger } from '@heroui/react'
import { ChevronDown, CreditCard, ExternalLink, LogOut, Settings, WifiOff } from 'lucide-react'
import { PlanBadge } from '@/components/ui/PlanBadge'
import type { Account } from '@/lib/contract'
import { api } from '@/lib/api'
import { formatNumber } from '@/lib/format'
import { billingUrl, PELLI_LINKS } from '@/lib/plans'
import { useT } from '@/lib/i18n'
import { modLabel } from '@/lib/platform'
import { useAppStore } from '@/store/app-store'

export function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((p) => p[0]?.toUpperCase())
    .join('')
}

export function Avatar({ account, size = 28 }: { account: Account; size?: number }) {
  if (account.avatarUrl) return <img src={account.avatarUrl} alt="" className="shrink-0 rounded-full object-cover" style={{ width: size, height: size }} />
  return (
    <span
      aria-hidden
      className="inline-flex shrink-0 items-center justify-center rounded-full bg-[#5840f0] font-semibold text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.18)]"
      style={{ width: size, height: size, fontSize: size * 0.38 }}
    >
      {initials(account.name)}
    </span>
  )
}

/** "142 / 200 photos this month", or "Unlimited photos". */
export function usageLine(account: Account, t: ReturnType<typeof useT>): string {
  const { monthlyQuota, usedThisMonth } = account.entitlements
  if (monthlyQuota === null) return t('plans.unlimited')
  return t('account.usage', { used: formatNumber(usedThisMonth), quota: formatNumber(monthlyQuota) })
}

/** Account chip of the header: plan and usage, Pelli, settings, sign-out. */
export function AccountMenu() {
  const t = useT()
  const auth = useAppStore((s) => s.auth)
  const online = useAppStore((s) => s.online)
  const openDialog = useAppStore((s) => s.openDialog)
  const signOut = useAppStore((s) => s.signOut)
  const offers = useAppStore((s) => s.offers)
  if (auth.status !== 'signedIn') return null
  const { account } = auth

  return (
    <div className="flex items-center gap-2">
      {!online && (
        <span className="inline-flex items-center gap-1.5 text-xs font-medium text-warning">
          <WifiOff className="size-3.5" aria-hidden />
          {t('common.offline')}
        </span>
      )}
      <Dropdown placement="bottom-end" radius="lg" classNames={{ content: 'min-w-[260px] p-1' }}>
        <DropdownTrigger>
          <button
            type="button"
            className="focus-ring flex h-9 items-center gap-2 rounded-xl py-1 pl-1 pr-2 transition-colors duration-200 hover:bg-default-100 dark:hover:bg-white/[0.05]"
            aria-label={t('account.menu')}
          >
            <Avatar account={account} />
            <span className="max-w-40 truncate text-[13px] font-medium text-foreground">{account.name}</span>
            <PlanBadge plan={account.plan} label={account.planLabel} />
            <ChevronDown className="size-3.5 text-default-400" aria-hidden />
          </button>
        </DropdownTrigger>
        <DropdownMenu aria-label={t('account.menu')} variant="flat">
          <DropdownSection showDivider aria-label={account.email}>
            <DropdownItem key="profile" isReadOnly className="cursor-default opacity-100 data-[hover=true]:bg-transparent" textValue={account.email}>
              <div className="flex items-center gap-3 py-1">
                <Avatar account={account} size={36} />
                <div className="min-w-0">
                  <p className="truncate text-sm font-semibold text-foreground">{account.name}</p>
                  <p className="truncate text-xs text-default-500">{account.email}</p>
                  <p className="tabular mt-1 truncate text-xs text-[#a599f5]">{usageLine(account, t)}</p>
                </div>
              </div>
            </DropdownItem>
          </DropdownSection>
          <DropdownSection aria-label={t('account.menu')}>
            <DropdownItem key="plan" startContent={<CreditCard className="size-4" />} onPress={() => void api().openExternal(billingUrl(offers))}>
              {t('account.managePlan')}
            </DropdownItem>
            <DropdownItem key="pelli" startContent={<ExternalLink className="size-4" />} onPress={() => void api().openExternal(PELLI_LINKS.dashboard)}>
              {t('account.openPelli')}
            </DropdownItem>
            <DropdownItem key="settings" startContent={<Settings className="size-4" />} onPress={() => openDialog('settings', 'account')} shortcut={`${modLabel()},`}>
              {t('account.settings')}
            </DropdownItem>
            <DropdownItem key="signout" startContent={<LogOut className="size-4" />} className="text-danger" color="danger" onPress={() => void signOut()}>
              {t('account.signOut')}
            </DropdownItem>
          </DropdownSection>
        </DropdownMenu>
      </Dropdown>
    </div>
  )
}
