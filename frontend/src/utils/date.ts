import dayjs, { Dayjs } from 'dayjs'
import relativeTime from 'dayjs/plugin/relativeTime'

dayjs.extend(relativeTime)

export const getCurrentDate = (): Dayjs => dayjs()

export const getDate = (date: string): Dayjs => dayjs(date)

export const getRelativeTimeFromNow = (date: string): string => {
  return dayjs(date).fromNow()
}
