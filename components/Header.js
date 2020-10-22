import React from 'react'
import PropTypes from 'prop-types'
import Link from 'next/link'
import { DateTime } from 'luxon'
import { toTitleCase } from '../utils/formatters'
import { MODES } from '../utils/constants'
import { name } from '../package.json'
import styles from '../styles/Header.module.css'

const Header = ({ mode, setMode, setEntry }) => (
  <header className={styles.header}>
    <h1 className={styles.title}>
      <Link href="/">
        <a>{toTitleCase(name)}</a>
      </Link>
    </h1>

    {!!mode && (
      <div
        className={styles.mode}
        onClick={() => {
          setEntry(null)

          switch (mode) {
            case MODES.EDIT:
              setMode(MODES.VIEW)
              break
            case MODES.VIEW:
              setMode(MODES.EDIT)
              break
            default:
              setMode(MODES.VIEW)
          }
        }}
      >
        {mode === MODES.EDIT ? 'View Entires' : 'Add Entry'}
      </div>
    )}

    <div className={styles.date}>
      {DateTime.local().toLocaleString(DateTime.DATE_MED)}
    </div>
  </header>
)

Header.propTypes = {
  mode: PropTypes.oneOf([MODES.VIEW, MODES.EDIT]),
  setMode: PropTypes.func,
}

export default Header
