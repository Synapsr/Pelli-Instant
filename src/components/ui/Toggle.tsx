import { Switch, type SwitchProps } from '@heroui/react'

/** HeroUI switch with the violet accent as "on" colour (the neutral primary is too faint in dark mode). */
export function Toggle({ classNames, ...props }: SwitchProps) {
  return (
    <Switch
      size="sm"
      {...props}
      classNames={{
        ...classNames,
        wrapper: `group-data-[selected=true]:!bg-[#5840f0] dark:group-data-[selected=true]:!bg-[#7c6af0] ${classNames?.wrapper ?? ''}`,
        thumb: `bg-white shadow-sm ${classNames?.thumb ?? ''}`,
      }}
    />
  )
}
