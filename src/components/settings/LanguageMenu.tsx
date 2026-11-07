import { Dropdown, DropdownItem, DropdownMenu, DropdownTrigger } from '@heroui/react'
import { ChevronsUpDown } from 'lucide-react'
import type { Language } from '@/lib/contract'
import { LANGUAGE_NAMES, LANGUAGES } from '@/lib/i18n'

/** The app's language, picked from a menu (each language in its own words). */
export function LanguageMenu({ value, label, onChange }: { value: Language; label: string; onChange: (language: Language) => void }) {
  return (
    <Dropdown placement="bottom-end" radius="lg" classNames={{ content: 'min-w-[200px] p-1' }}>
      <DropdownTrigger>
        <button
          type="button"
          aria-label={label}
          className="focus-ring flex h-9 min-w-[172px] items-center justify-between gap-3 rounded-xl bg-default-100 pl-3.5 pr-2.5 text-[13px] font-medium text-foreground transition-[background-color,scale] duration-200 hover:bg-default-200 active:scale-[0.96] dark:bg-white/[0.06] dark:hover:bg-white/[0.09]"
        >
          <span lang={value}>{LANGUAGE_NAMES[value]}</span>
          <ChevronsUpDown className="size-3.5 text-default-400" aria-hidden />
        </button>
      </DropdownTrigger>
      <DropdownMenu
        aria-label={label}
        variant="flat"
        selectionMode="single"
        disallowEmptySelection
        selectedKeys={new Set([value])}
        onAction={(key) => {
          if (key !== value) onChange(key as Language)
        }}
      >
        {LANGUAGES.map((language) => (
          <DropdownItem key={language} textValue={LANGUAGE_NAMES[language]} lang={language}>
            {LANGUAGE_NAMES[language]}
          </DropdownItem>
        ))}
      </DropdownMenu>
    </Dropdown>
  )
}
