import dayjs, { Dayjs } from 'dayjs'
import relativeTime from 'dayjs/plugin/relativeTime'
import localizedFormat from 'dayjs/plugin/localizedFormat'

dayjs.extend(relativeTime)
dayjs.extend(localizedFormat)

export const getCurrentDate = (): Dayjs => dayjs()

export const getDate = (date: string): Dayjs => dayjs(date)

export const getRelativeTimeFromNow = (date: string): string => {
  return dayjs(date).fromNow()
}

export const getCurrentDateLocalized = (): string => {
  return dayjs().format('LL')
}
