const INTERIOR_ENTRIES = 'INTERIOR_ENTRIES'
const SCROLL_POSITION = 'scrollY'

const saveEntries = (entries) =>
  entries
    ? global.localStorage.setItem(INTERIOR_ENTRIES, JSON.stringify(entries))
    : global.localStorage.removeItem(INTERIOR_ENTRIES)

const retrieveEntries = () =>
  JSON.parse(global.localStorage.getItem(INTERIOR_ENTRIES))

const saveScrollY = (scrollY) => {
  global.localStorage.setItem(SCROLL_POSITION, scrollY || window.scrollY)
}
const retrieveScrollY = () => global.localStorage.getItem(SCROLL_POSITION)

export { saveEntries, retrieveEntries, saveScrollY, retrieveScrollY }
