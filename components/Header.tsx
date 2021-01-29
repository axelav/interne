import React, { useState, useRef, useEffect } from 'react'
import Link from 'next/link'
import { DateTime } from 'luxon'
import { CSSTransition } from 'react-transition-group'
import { Input } from './Forms'
import { toTitleCase } from '../utils/formatters'
import { VIEW, EDIT, KEYS, Mode } from '../utils/constants'
import { name } from '../package.json'
import styles from '../styles/Header.module.css'

const Header = ({
  mode,
  setMode,
  setEntry,
  searchText,
  setSearchText,
}: {
  mode?: Mode
  setMode?: (mode: Mode) => void
  setEntry?: (entry: any) => void
  searchText?: string
  setSearchText?: (searchText: string) => void
}) => {
  const [showSearch, setShowSearch] = useState(false)
  const [showDate, setShowDate] = useState(true)
  const inputRef = useRef(null)

  useEffect(() => {
    const handleKeydown = (ev: KeyboardEvent) => {
      if (ev.key === KEYS.FWD_SLASH) {
        if (document.activeElement === document.body) {
          setShowSearch(true)
          inputRef.current.focus()

          // prevent `/` character from being used as input value
          ev.preventDefault()
        }
      }

      if (ev.key === KEYS.ESC) {
        if (
          !!inputRef.current &&
          inputRef.current.className === document.activeElement.className
        ) {
          setSearchText('')
          setShowSearch(false)
        }
      }
    }

    document.addEventListener('keydown', handleKeydown)

    return () => document.removeEventListener('keydown', handleKeydown)
  }, [searchText, setSearchText])

  useEffect(() => {
    if (inputRef.current) {
      // FIXME there's a bug when you
      // 1. enter text in input
      // 2. blur input
      // 3. hit `/` to focus input again
      // 4. enter new text
      // 5. blur - input value remains but input is hidden
      inputRef.current.addEventListener('blur', () => {
        if (!searchText) {
          setShowSearch(false)
        }
      })
    }
  }, [searchText])

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
              case EDIT:
                setMode(VIEW)
                break
              case VIEW:
                setMode(EDIT)
                break
              default:
                setMode(VIEW)
            }
          }}
        >
          {mode === EDIT ? 'View Entires' : 'Add Entry'}
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

export default Header
