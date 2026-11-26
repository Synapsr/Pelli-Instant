import { createElement, type ReactNode } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { translate, useI18nStore } from '@/lib/i18n'
import { SupportDialog } from './SupportDialog'

// On the server, stores answer their initial state: whether the dialog shows is set here.
const visible = vi.hoisted(() => ({ value: true }))
vi.mock('@/store/support', async (original) => ({
  ...(await original<typeof import('@/store/support')>()),
  useSupportDialogVisible: () => visible.value,
}))

// HeroUI's dialog renders in a portal (nothing on the server): render it in
// place, as HeroUI wires it: the dialog is labelled by its ModalHeader's id.
vi.mock('@heroui/react', async (original) => {
  const actual = await original<typeof import('@heroui/react')>()
  type Props = { children?: ReactNode; isOpen?: boolean; [key: string]: unknown }
  return {
    ...actual,
    Modal: ({ isOpen, children }: Props) => (isOpen ? createElement('div', null, children) : null),
    ModalContent: ({ children }: Props) => createElement('section', { role: 'dialog', 'aria-labelledby': 'modal-header' }, children),
    ModalHeader: ({ children, as, className }: Props) => createElement(typeof as === 'string' ? as : 'header', { id: 'modal-header', className }, children),
    ModalBody: ({ children }: Props) => createElement('div', null, children),
  }
})

describe('support dialog markup', () => {
  it('is named by its translated title', () => {
    visible.value = true
    const html = renderToStaticMarkup(createElement(SupportDialog))
    const labelledBy = /role="dialog" aria-labelledby="([^"]+)"/.exec(html)?.[1]
    expect(labelledBy).toBeTruthy()
    const escaped = (labelledBy ?? '').replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    const named = new RegExp(`id="${escaped}"[^>]*>([^<]+)<`).exec(html)?.[1]
    // The server renders in the store's first language: the title of that language.
    expect(named).toBe(translate(useI18nStore.getInitialState().language, 'support.title'))
  })

  it('renders nothing once closed', () => {
    visible.value = false
    expect(renderToStaticMarkup(createElement(SupportDialog))).toBe('')
  })
})
