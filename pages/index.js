import React, { useState, useEffect } from 'react'
import Head from 'next/head'
import orderBy from 'lodash.orderby'
import omit from 'lodash.omit'
import CreateEntryForm from '../components/CreateEntryForm'
import Header from '../components/Header'
import Footer from '../components/Footer'
import {
  saveEntries,
  retrieveEntries,
  saveScrollY,
  retrieveScrollY,
} from '../services/storage'
import { getAvailableAtPlusEntropy } from '../utils/entropy'
import { getRelativeTimeFromNow } from '../utils/date'
import { toTitleCase } from '../utils/formatters'
import { MODES, KEY_CODES } from '../utils/constants'
import packageData from '../package.json'
import pageStyles from '../styles/Pages.module.css'
import styles from '../styles/Index.module.css'

const msgs = [
  {
    en: 'Read a book!',
    eo: 'Legi libron!',
  },
  {
    en: 'Go outside!',
    eo: 'Iru eksteren!',
  },
]

const emptyListMsg = msgs[Math.floor(Math.random() * msgs.length)]

const Index = () => {
  const [entries, setEntries] = useState([])
  const [entry, setEntry] = useState(null)
  const [mode, setMode] = useState(MODES.VIEW)
  const [isFilterActive, setIsFilterActive] = useState(true)
  const [searchText, setSearchText] = useState('')
  const [visibleEntries, setVisibleEntries] = useState([])

  useEffect(() => {
    const handleKeydown = ({ keyCode }) => {
      if (keyCode === KEY_CODES.ESC) {
        if (mode === MODES.EDIT) {
          setMode(MODES.VIEW)
        } else if (document.activeElement === document.body) {
          setIsFilterActive(!isFilterActive)
        }
      }
    }

    document.addEventListener('keydown', handleKeydown)

    return () => document.removeEventListener('keydown', handleKeydown)
  }, [isFilterActive, mode, searchText])

  useEffect(() => {
    const handleScrollTo = () => {
      const scrollY = retrieveScrollY()

      if (scrollY) {
        window.scrollTo(0, scrollY)
      }
    }

    if (mode === MODES.VIEW) {
      handleScrollTo()
    }
  }, [mode])

  useEffect(() => {
    if (entries.length < 1) {
      const result = retrieveEntries()

      if (!!result && result.length > 0) {
        handleEntriesChange(result)
      }
    }
  }, [entries])

  useEffect(() => {
    // TODO use ReactCSSTransitionGroup
    const nextEntries = orderBy(
      entries.filter((x) => {
        const escapeRegExp = (str) => str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
        const regex = new RegExp(escapeRegExp(searchText), 'gi')
        const match =
          x.title.match(regex) ||
          x.description.match(regex) ||
          x.url.match(regex)

        if (searchText) {
          return !!match
        } else {
          return isFilterActive ? x.visible : true
        }
      }),
      isFilterActive ? ['dismissedAt'] : ['dismissedAt', 'availableAt'],
      isFilterActive ? ['desc'] : ['desc', 'asc']
    )

    setVisibleEntries(nextEntries)
  }, [entries, isFilterActive, searchText])

  const handleEntriesChange = (entries) => {
    const setAdditionalProps = (entry) => {
      const { availableAt, diff } = getAvailableAtPlusEntropy(entry)
      const visible = diff < 0

      return {
        ...entry,
        visible,
        availableAt,
      }
    }

    const result = entries.map(setAdditionalProps)

    setEntries(result)
    saveEntries(result.map((x) => omit(x, ['visible', 'availableAt'])))
  }

  const handleEntryClick = (entry) => {
    setTimeout(() => {
      const nextEntries = entries.filter((x) => x.id !== entry.id)
      nextEntries.push({
        ...entry,
        visited: ++entry.visited,
        dismissedAt: new Date().toISOString(),
      })

      handleEntriesChange(nextEntries)
    }, 200)
  }

  const handleViewFilterClick = () => setIsFilterActive(!isFilterActive)

  const handeSaveEntry = (x) => {
    if (!x.updatedAt) {
      saveScrollY(0)
    }

    handleEntriesChange([...entries.filter((y) => y.id !== x.id), x])
    setMode(MODES.VIEW)
  }

  const handleEditEntry = (entry) => {
    setEntry(entry)
    setMode(MODES.EDIT)
    saveScrollY()

    window.scrollTo(0, 0)
  }

  const handleDeleteEntry = (entry) => {
    const shouldDelete = global.confirm('Are you sure?')

    if (shouldDelete) {
      const nextEntries = entries.filter((x) => x.id !== entry.id)

      handleEntriesChange(nextEntries)
    }
  }

  return (
    <div className={pageStyles.container}>
      <Head>
        <title>{toTitleCase(packageData.name)}</title>
        <link rel="icon" href="/favicon.ico" />
      </Head>

      <Header
        mode={mode}
        setMode={setMode}
        setEntry={setEntry}
        searchText={searchText}
        setSearchText={setSearchText}
      />

      <main className={pageStyles.main}>
        {mode === MODES.EDIT ? (
          <CreateEntryForm
            onSubmit={handeSaveEntry}
            entries={entries}
            {...entry}
          />
        ) : (
          <div className={styles.grid}>
            {visibleEntries.length > 0 ? (
              visibleEntries.map((x) => (
                <div
                  key={x.id}
                  className={
                    x.visible
                      ? styles.card
                      : `${styles.card} ${styles.unavailable}`
                  }
                >
                  <div className={styles.viewed}>
                    <span>
                      {x.dismissedAt
                        ? `Last viewed ${getRelativeTimeFromNow(x.dismissedAt)}`
                        : 'Never viewed'}
                    </span>
                  </div>
                  <a
                    href={x.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={() => handleEntryClick(x)}
                  >
                    <div className={styles.title}>
                      <h2 title={x.title}>{x.title}</h2>
                      <div className={styles.rarr}>&rarr;</div>
                    </div>
                    <p title={x.description}>{x.description}</p>
                  </a>

                  <div className={styles['flex-between']}>
                    <div className={styles.availability}>
                      {!x.visible && (
                        <span>
                          Available {getRelativeTimeFromNow(x.availableAt)}
                        </span>
                      )}
                    </div>

                    <div className={styles.controls}>
                      {x.visible && (
                        <div
                          className={styles.ignore}
                          onClick={() => handleEntryClick(x)}
                        >
                          Mark Read
                        </div>
                      )}
                      <div
                        className={styles.edit}
                        onClick={() => handleEditEntry(x)}
                      >
                        Edit
                      </div>
                      <div
                        className={styles.delete}
                        onClick={() => handleDeleteEntry(x)}
                      >
                        Delete
                      </div>
                    </div>
                  </div>
                </div>
              ))
            ) : (
              <p
                className={styles.empty}
                title={!!searchText ? 'Neniuj rezultoj' : emptyListMsg.eo}
              >
                {!!searchText ? 'No results' : emptyListMsg.en}
              </p>
            )}
          </div>
        )}
      </main>

      {mode === MODES.VIEW && (
        <div
          className={styles.filter}
          onClick={handleViewFilterClick}
          style={{
            left: isFilterActive ? '-14px' : '-41px',
          }}
        >
          {isFilterActive ? 'View All' : <span>View Available</span>}
        </div>
      )}

      <Footer />
    </div>
  )
}

export default Index
