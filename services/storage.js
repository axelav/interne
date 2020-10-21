const INTERIOR_ENTRIES = 'INTERIOR_ENTRIES'

const saveEntries = (entries) =>
  entries
    ? global.localStorage.setItem(INTERIOR_ENTRIES, JSON.stringify(entries))
    : global.localStorage.removeItem(INTERIOR_ENTRIES)

const retrieveEntries = () =>
  JSON.parse(global.localStorage.getItem(INTERIOR_ENTRIES))

export { saveEntries, retrieveEntries }
