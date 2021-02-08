const LAST_VISIT = 'LAST_VISIT'
const INTERIOR_ENTRIES = 'INTERIOR_ENTRIES'

const saveLastVisit = (ts) => global.localStorage.setItem(LAST_VISIT, ts)
const getLastVisit = () => global.localStorage.getItem(LAST_VISIT)

const saveEntries = (entries) =>
  entries
    ? global.localStorage.setItem(INTERIOR_ENTRIES, JSON.stringify(entries))
    : global.localStorage.removeItem(INTERIOR_ENTRIES)

const retrieveEntries = () =>
  JSON.parse(global.localStorage.getItem(INTERIOR_ENTRIES))

export { saveEntries, retrieveEntries, saveLastVisit, getLastVisit }
