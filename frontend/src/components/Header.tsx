import { MODES } from '../utils/constants'
import styles from '../styles/Header.module.css'

interface HeaderProps {
  mode: string
  setMode: (mode: string) => void
  setEntry: (entry: null) => void
  searchText: string
  setSearchText: (text: string) => void
}

export default function Header({
  mode,
  setMode,
  setEntry,
  searchText,
  setSearchText,
}: HeaderProps) {
  const handleAddClick = () => {
    setEntry(null)
    setMode(MODES.EDIT)
  }

  const handleCancelClick = () => {
    setMode(MODES.VIEW)
  }

  return (
    <header className={styles.header}>
      <div className={styles.container}>
        <h1 className={styles.title}>Interne</h1>

        {mode === MODES.VIEW ? (
          <div className={styles.controls}>
            <input
              type="text"
              className={styles.search}
              placeholder="Search..."
              value={searchText}
              onChange={(e) => setSearchText(e.target.value)}
            />
            <button className={styles.add} onClick={handleAddClick}>
              +
            </button>
          </div>
        ) : (
          <button className={styles.cancel} onClick={handleCancelClick}>
            Cancel
          </button>
        )}
      </div>
    </header>
  )
}
