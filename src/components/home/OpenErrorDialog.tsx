import { Button, Modal, ModalBody, ModalContent, ModalFooter } from '@heroui/react'
import { FolderLock, FolderX, ImageOff, TriangleAlert } from 'lucide-react'
import { IconBox } from '@/components/ui/IconBox'
import { useT } from '@/lib/i18n'
import { isMac } from '@/lib/platform'
import { useAppStore } from '@/store/app-store'

/** Folder that could not be opened: no photos, permission denied, gone. */
export function OpenErrorDialog() {
  const t = useT()
  const error = useAppStore((s) => s.openError)
  const dismiss = useAppStore((s) => s.dismissOpenError)
  const pickAndOpen = useAppStore((s) => s.pickAndOpen)
  const openFolder = useAppStore((s) => s.openFolder)

  const content = (() => {
    switch (error?.code) {
      case 'noPhotos':
        return { icon: ImageOff, tone: 'accent' as const, title: t('preAnalysis.emptyTitle'), body: t('preAnalysis.emptyBody') }
      case 'permissionDenied':
        return {
          icon: FolderLock,
          tone: 'warning' as const,
          title: t('errors.permissionDeniedTitle'),
          body: isMac() ? t('errors.permissionDeniedMac') : t('errors.permissionDeniedOther'),
        }
      case 'notFound':
        return { icon: FolderX, tone: 'warning' as const, title: t('errors.generic'), body: t('errors.notFound') }
      default:
        return { icon: TriangleAlert, tone: 'danger' as const, title: t('errors.generic'), body: error?.message ?? '' }
    }
  })()

  return (
    <Modal isOpen={error !== null} onClose={dismiss} size="md" radius="lg" backdrop="blur" classNames={{ base: 'glass-strong rounded-[24px]', backdrop: 'bg-black/50' }}>
      <ModalContent>
        <ModalBody className="items-center px-8 pb-2 pt-8 text-center">
          <IconBox icon={content.icon} size="lg" tone={content.tone} />
          <h2 className="mt-3 text-base font-semibold">{content.title}</h2>
          <p className="text-sm text-default-500">{content.body}</p>
          {error?.path && <p className="mt-1 max-w-full truncate rounded-lg bg-default-100 px-2.5 py-1 font-mono text-[11px] text-default-500 dark:bg-white/[0.04]">{error.path}</p>}
        </ModalBody>
        <ModalFooter className="justify-center pb-6">
          <Button variant="light" onPress={dismiss}>
            {t('common.close')}
          </Button>
          {error?.code === 'permissionDenied' ? (
            <Button
              color="primary"
              onPress={() => {
                const path = error.path
                dismiss()
                void openFolder(path)
              }}
            >
              {t('common.retry')}
            </Button>
          ) : (
            <Button
              color="primary"
              onPress={() => {
                dismiss()
                void pickAndOpen()
              }}
            >
              {t('preAnalysis.chooseAnother')}
            </Button>
          )}
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
