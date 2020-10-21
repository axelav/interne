import React from 'react'
import PropTypes from 'prop-types'
import Link from 'next/link'
import { DateTime } from 'luxon'
import { toTitleCase } from '../utils/formatters'
import { MODES } from '../utils/constants'
import { name } from '../package.json'
import styles from '../styles/Header.module.css'

const Header = ({ mode, setMode }) => (
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
          switch (mode) {
            case MODES.CREATE:
              setMode(MODES.VIEW)
              break
            case MODES.VIEW:
              setMode(MODES.CREATE)
              break
            default:
              setMode(MODES.VIEW)
          }
        }}
      >
        {mode === MODES.CREATE ? 'View Entires' : 'Add Entry'}
      </div>
    )}

    <div className={styles['current-date']}>
      {DateTime.local().toLocaleString(DateTime.DATE_MED)}
    </div>
  </header>
)

Header.propTypes = {
  mode: PropTypes.oneOf([MODES.VIEW, MODES.CREATE]),
  setMode: PropTypes.func,
}

export default Header
