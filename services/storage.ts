import { Entry } from '../pages/index'
const INTERIOR_ENTRIES = 'INTERIOR_ENTRIES'

const saveEntries = (entries: Entry[]): void =>
  entries
    ? global.localStorage.setItem(INTERIOR_ENTRIES, JSON.stringify(entries))
    : global.localStorage.removeItem(INTERIOR_ENTRIES)

const retrieveEntries = (): Entry[] =>
  JSON.parse(global.localStorage.getItem(INTERIOR_ENTRIES))

export { saveEntries, retrieveEntries }
