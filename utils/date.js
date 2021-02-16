import * as dayjs from 'dayjs'
import relativeTime from 'dayjs/plugin/relativeTime'
import localizedFormat from 'dayjs/plugin/localizedFormat'

dayjs.extend(relativeTime)
dayjs.extend(localizedFormat)

const getCurrentDate = () => dayjs()
const getDate = (str) => dayjs(str)
const getCurrentDateLocalized = () => dayjs().format('LL')
const getRelativeTimeFromNow = (str) => dayjs(str).fromNow()

export {
  dayjs as default,
  getCurrentDate,
  getDate,
  getCurrentDateLocalized,
  getRelativeTimeFromNow,
}
