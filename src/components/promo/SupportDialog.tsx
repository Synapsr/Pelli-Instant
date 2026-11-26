import { Button, Modal, ModalBody, ModalContent } from '@heroui/react'
import { ArrowRight, Heart, Instagram, Sparkles, Star, X } from 'lucide-react'
import { useRef } from 'react'
import { useT } from '@/lib/i18n'
import { GITHUB_URL, INSTAGRAM_URL } from '@/lib/links'
import { closeSupportDialog, followFromSupportDialog, getPlusFromSupportDialog, useSupportDialogVisible } from '@/store/support'

/**
 * The free version's dialog on the way to the review of a sort: follow Pelli
 * on Instagram first (photographers rarely have a GitHub account), a GitHub
 * star second, and a clear way to Plus, which hides it. The look of the Plus
 * dialog, much lighter. Every way out goes on to the review.
 */
export function SupportDialog() {
  const t = useT()
  const open = useSupportDialogVisible()
  const instagram = useRef<HTMLButtonElement>(null)
  return (
    <Modal
      isOpen={open}
      onClose={closeSupportDialog}
      size="md"
      radius="lg"
      backdrop="blur"
      hideCloseButton
      classNames={{ base: 'glass-strong max-w-[420px] overflow-hidden rounded-[28px] text-foreground', backdrop: 'bg-black/60' }}
    >
      {/* The dialog takes the focus as it opens: it goes to Instagram, the first step. */}
      <ModalContent onFocus={(e) => e.target === e.currentTarget && instagram.current?.focus()}>
        <ModalBody className="relative overflow-hidden p-0">
          {/* Soft violet light behind the top, as in the Plus dialog. */}
          <span aria-hidden className="pointer-events-none absolute -top-36 left-1/2 h-64 w-[420px] -translate-x-1/2 rounded-full bg-[radial-gradient(closest-side,rgba(88,64,240,0.34),transparent)]" />
          <div className="relative flex flex-col items-center px-8 pb-6 pt-9 text-center">
            <span className="brand-fill flex size-12 items-center justify-center rounded-2xl shadow-[0_12px_32px_-10px_rgba(88,64,240,0.9)]" aria-hidden>
              <Heart className="size-[22px]" strokeWidth={1.9} />
            </span>
            <h2 className="mt-5 text-balance text-[22px] font-semibold leading-tight tracking-[-0.02em] text-foreground">{t('support.title')}</h2>
            <p className="mt-2.5 max-w-[340px] text-pretty text-[13.5px] leading-relaxed text-default-500">{t('support.body')}</p>

            <Button
              ref={instagram}
              autoFocus
              radius="full"
              className="btn-brand mt-6 h-12 w-full text-[15px] font-semibold"
              startContent={<Instagram className="size-[18px]" aria-hidden />}
              onPress={() => followFromSupportDialog(INSTAGRAM_URL)}
            >
              {t('support.instagram')}
            </Button>
            <Button
              radius="full"
              variant="light"
              className="mt-2 h-10 w-full text-[13px] font-medium text-default-500 data-[hover=true]:bg-white/[0.05] data-[hover=true]:text-foreground"
              startContent={<Star className="size-4 text-[#b8adf8]" aria-hidden />}
              onPress={() => followFromSupportDialog(GITHUB_URL)}
            >
              {t('support.github')}
            </Button>
            <button
              type="button"
              onClick={closeSupportDialog}
              className="focus-ring group mt-3 inline-flex items-center gap-1 rounded-full px-3 py-1.5 text-[13px] text-default-400 transition-colors duration-150 hover:text-foreground"
            >
              {t('support.later')}
              <ArrowRight className="size-3.5 transition-transform duration-150 group-hover:translate-x-0.5" aria-hidden />
            </button>
          </div>

          <div className="relative flex flex-wrap items-center justify-center gap-x-2.5 gap-y-1.5 border-t border-white/[0.07] bg-white/[0.025] px-6 py-3.5 text-[12.5px]">
            <span className="text-default-500">{t('support.plusQuestion')}</span>
            <button
              type="button"
              onClick={getPlusFromSupportDialog}
              className="focus-ring inline-flex items-center gap-1.5 rounded-full border border-[#a599f5]/25 bg-[#5840f0]/15 px-3 py-1 font-semibold text-[#d0c9ff] transition-[background-color,border-color] duration-150 hover:border-[#a599f5]/45 hover:bg-[#5840f0]/25"
            >
              <Sparkles className="size-3.5" strokeWidth={2} aria-hidden />
              {t('support.plusCta')}
            </button>
          </div>

          {/* Last in the tab order: the focus starts on Instagram. */}
          <button
            type="button"
            onClick={closeSupportDialog}
            aria-label={t('common.close')}
            className="focus-ring absolute right-3 top-3 z-20 flex size-10 items-center justify-center rounded-full text-default-500 transition-[background-color,color,scale] duration-150 hover:bg-white/[0.06] hover:text-foreground active:scale-[0.96]"
          >
            <X className="size-[18px]" />
          </button>
        </ModalBody>
      </ModalContent>
    </Modal>
  )
}
