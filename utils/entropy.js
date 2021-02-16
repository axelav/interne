import { getCurrentDate, getDate } from '../utils/date'

const MAX = 7
const MILLIS_IN_DAY = 24 * 60 * 60 * 1000

// TODO user option
const opts = {
  entropy: 5,
}

const getAvailableAtPlusEntropy = ({ dismissedAt, interval, duration }) => {
  const now = getCurrentDate()
  const { entropy } = opts

  const dismissedAtDate = dismissedAt
    ? getDate(dismissedAt)
    : now.subtract(1, 'seconds')

  const availableAt = dismissedAtDate.add(duration, interval)
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

export { getAvailableAtPlusEntropy }
