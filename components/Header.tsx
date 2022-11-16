import React, { useState, useRef, useEffect, SyntheticEvent } from 'react'
import PropTypes from 'prop-types'
import Link from 'next/link'
import { CSSTransition } from 'react-transition-group'
import { Input } from './Forms'
import { getCurrentDateLocalized } from '../utils/date'
import { toTitleCase } from '../utils/formatters'
import { MODES, KEY_CODES, Modes } from '../utils/constants'
import packageData from '../package.json'
import styles from '../styles/Header.module.css'

interface Props {
  mode: Modes
  searchText?: string
  setMode: React.Dispatch<React.SetStateAction<Modes>>
  setEntry: React.Dispatch<React.SetStateAction<string>>
  setSearchText: React.Dispatch<React.SetStateAction<string>>
}

const Header = ({
  mode,
  setMode,
  setEntry,
  searchText,
  setSearchText,
}: Props) => {
  const [showSearch, setShowSearch] = useState(false)
  const [showDate, setShowDate] = useState(true)
  const inputRef = useRef(null)

  useEffect(() => {
    const handleKeydown = (evt: KeyboardEvent) => {
      if (evt.keyCode === KEY_CODES.FWD_SLASH) {
        if (document.activeElement === document.body) {
          setShowSearch(true)
          inputRef.current.focus()

          // prevent `/` character from being used as input value
          evt.preventDefault()
        }
      }

      if (evt.keyCode === KEY_CODES.ESC) {
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
          <a>{toTitleCase(packageData.name)}</a>
        </Link>
      </h1>

      {mode && (
        <div
          className={styles.mode}
          onClick={() => {
            setEntry(null)

            if (mode === Modes.Edit) {
              setMode(Modes.View)
            } else {
              setMode(Modes.Edit)
            }

            // switch (mode) {
            //   case Modes.Edit:
            //     setMode(Modes.View)
            //     break
            //   case Modes.View:
            //     setMode(Modes.Edit)
            //     break
            //   default:
            //     setMode(Modes.View)
            // }
          }}
        >
          {mode === Modes.Edit ? 'View Entires' : 'Add Entry'}
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
              onChange={(evt) => setSearchText(evt.currentTarget.value)}
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
          <div>{getCurrentDateLocalized()}</div>
        </CSSTransition>
      </div>
    </header>
  )
}

export default Header
