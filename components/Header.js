import React, { useState, useRef } from 'react'
import PropTypes from 'prop-types'
import Link from 'next/link'
import { DateTime } from 'luxon'
import { CSSTransition } from 'react-transition-group'
import { Input } from './Forms'
import { toTitleCase } from '../utils/formatters'
import { MODES } from '../utils/constants'
import { name } from '../package.json'
import styles from '../styles/Header.module.css'

const Header = ({ mode, setMode, setEntry, searchText, setSearchText }) => {
  const [showSearch, setShowSearch] = useState(false)
  const [showDate, setShowDate] = useState(true)
  const inputRef = useRef(null)

  return (
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

      <div
        className={styles.date}
        onMouseEnter={() => setShowSearch(true)}
        onMouseLeave={() => {
          if (!searchText) {
            setShowSearch(false)
          }
        }}
      >
        <CSSTransition
          in={showSearch}
          timeout={200}
          classNames="fade"
          unmountOnExit
          onEnter={() => {
            setShowDate(false)
            inputRef.current.focus()
          }}
          onExited={() => setShowDate(true)}
        >
          <div>
            <Input
              ref={inputRef}
              value={searchText}
              onChange={setSearchText}
              placeholder="Search"
            />
          </div>
        </CSSTransition>

        <CSSTransition
          in={showDate}
          timeout={200}
          classNames="fade"
          unmountOnExit
          onEnter={() => setShowSearch(false)}
          onExited={() => setShowSearch(true)}
        >
          <div>{DateTime.local().toLocaleString(DateTime.DATE_MED)}</div>
        </CSSTransition>
      </div>
    </header>
  )
}

Header.propTypes = {
  mode: PropTypes.oneOf([MODES.VIEW, MODES.EDIT]).isRequired,
  setMode: PropTypes.func.isRequired,
  setEntry: PropTypes.func.isRequired,
  searchText: PropTypes.string,
  setSearchText: PropTypes.func.isRequired,
}

export default Header
