import { DateTime } from 'luxon'

const MAX = 7
const MILLIS_IN_DAY = 24 * 60 * 60 * 1000

// TODO user option
const opts = {
  entropy: 5,
}

const getAvailableAtPlusEntropy = ({ dismissedAt, interval, duration }) => {
  const { entropy } = opts

  const availableAt = (
    DateTime.fromISO(dismissedAt) || DateTime.local().minus(1, 'sec')
  ).plus({ [interval]: duration })
  const diff = availableAt.diffNow().toObject().milliseconds

  if (entropy && diff > MILLIS_IN_DAY) {
    const availableAtPlusEntropy = availableAt.plus({
      days: Math.floor(Math.random() * ((entropy / 10) * MAX)),
    })

    return {
      availableAt: availableAtPlusEntropy,
      diff: availableAtPlusEntropy.diffNow().toObject().milliseconds,
    }
  }

  return { availableAt, diff }
}

export { getAvailableAtPlusEntropy }
