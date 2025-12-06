import type { Entry } from '../types/entry'
import type { Dayjs } from 'dayjs'
import { getCurrentDate, getDate } from './date'

const MAX = 7
const MILLIS_IN_DAY = 24 * 60 * 60 * 1000

// TODO: make user configurable
const opts = {
  entropy: 5,
}

export const getAvailableAtPlusEntropy = ({
  dismissed_at,
  interval,
  duration,
}: Pick<Entry, 'dismissed_at' | 'interval' | 'duration'>): {
  availableAt: Dayjs
  diff: number
} => {
  const now = getCurrentDate()
  const { entropy } = opts

  const availableAt = dismissed_at
    ? getDate(dismissed_at).add(duration, interval as any)
    : now.subtract(1, 'seconds')

  const diff = availableAt.diff(now)

  if (entropy && diff > MILLIS_IN_DAY) {
    const availableAtPlusEntropy = availableAt.add(
      Math.floor(Math.random() * ((entropy / 10) * MAX)),
      'days'
    )

    return {
      availableAt: availableAtPlusEntropy,
      diff: availableAtPlusEntropy.diff(now),
    }
  }

  return { availableAt, diff }
}
