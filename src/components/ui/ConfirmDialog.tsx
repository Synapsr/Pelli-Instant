import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { useT } from '@/lib/i18n'

export function ConfirmDialog({
  open,
  title,
  body,
  confirmLabel,
  danger = false,
  onConfirm,
  onClose,
}: {
  open: boolean
  title: string
  body: string
  confirmLabel: string
  danger?: boolean
  onConfirm: () => void
  onClose: () => void
}) {
  const t = useT()
  return (
    <Modal isOpen={open} onClose={onClose} size="sm" radius="lg" backdrop="blur" classNames={{ base: 'glass-strong rounded-[24px]', backdrop: 'bg-black/50' }}>
      <ModalContent>
        <ModalHeader className="pb-1 text-base">{title}</ModalHeader>
        <ModalBody>
          <p className="text-sm text-default-500">{body}</p>
        </ModalBody>
        <ModalFooter>
          <Button variant="light" onPress={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            color={danger ? 'danger' : 'primary'}
            variant={danger ? 'flat' : 'solid'}
            onPress={() => {
              onConfirm()
              onClose()
            }}
          >
            {confirmLabel}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
