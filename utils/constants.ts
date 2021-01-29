export type Mode = 'view' | 'edit'

const INTERVALS = {
  HOURS: 'hours',
  DAYS: 'days',
  WEEKS: 'weeks',
  MONTHS: 'months',
  YEARS: 'years',
}

const EDIT: Mode = 'edit'
const VIEW: Mode = 'view'

// TODO how do Enums really work?
enum Modes {
  View,
  Edit,
}

const KEY_CODES = {
  ESC: 27,
  FWD_SLASH: 191,
}

const KEYS = {
  ESC: 'Escape',
  FWD_SLASH: '/',
}

export { INTERVALS, KEY_CODES, KEYS, EDIT, VIEW, Modes }
