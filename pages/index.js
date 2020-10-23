import React, { useState, useEffect } from 'react'
import Head from 'next/head'
import orderBy from 'lodash.orderby'
import omit from 'lodash.omit'
import { DateTime } from 'luxon'
import CreateEntryForm from '../components/CreateEntryForm'
import Header from '../components/Header'
import Footer from '../components/Footer'
import { saveEntries, retrieveEntries } from '../services/storage'
import { toTitleCase } from '../utils/formatters'
import { MODES } from '../utils/constants'
import { name } from '../package.json'
import pageStyles from '../styles/Pages.module.css'
import styles from '../styles/Index.module.css'

const Index = () => {
  const [entries, setEntries] = useState([])
  const [entry, setEntry] = useState(null)
  const [mode, setMode] = useState(MODES.VIEW)
  const [isFilterActive, setIsFilterActive] = useState(true)

  useEffect(() => {
    if (entries.length < 1) {
      const result = retrieveEntries()

      if (!!result && result.length > 0) {
        handleEntiresChange(result)
      }
    }
  }, [entries])

  useEffect(() => {
    const interval = setInterval(() => handleEntiresChange(entries), 1000 * 15)
    return () => clearInterval(interval)
  }, [entries])

  const handleEntiresChange = (entries) => {
    const setVisibility = (entry) => {
      let visible = true
      let diff
      let nextAvailableDate

      if (entry.dismissedAt) {
        nextAvailableDate = DateTime.fromISO(entry.dismissedAt).plus({
          [entry.interval]: entry.duration,
        })
        diff = nextAvailableDate.diffNow().toObject().milliseconds

        visible = diff < 0
      }

      return {
        ...entry,
        diff,
        visible,
        availableAt:
          nextAvailableDate && !visible
            ? nextAvailableDate.toRelative()
            : undefined,
      }
    }

    const orderedEntries = orderBy(
      entries.map(setVisibility),
      ['visible', 'diff', 'createdAt'],
      ['desc', 'asc', 'asc']
    )

    setEntries(orderedEntries)
    saveEntries(
      orderedEntries.map((x) => omit(x, ['diff', 'visible', 'availableAt']))
    )
  }

  const handleEntryClick = (entry) => {
    setTimeout(() => {
      const nextEntries = entries.filter((x) => x.id !== entry.id)
      nextEntries.push({
        ...entry,
        visited: ++entry.visited,
        dismissedAt: new Date().toISOString(),
      })

      handleEntiresChange(nextEntries)
    }, 500)
  }

  const handleViewFilterClick = () => setIsFilterActive(!isFilterActive)

  const handleEditEntry = (entry) => {
    setEntry(entry)
    setMode(MODES.EDIT)
  }

  const handleDeleteEntry = (entry) => {
    const shouldDelete = global.confirm('Are you sure?')

    if (shouldDelete) {
      const nextEntries = entries.filter((x) => x.id !== entry.id)

      handleEntiresChange(nextEntries)
    }
  }

  const visibleEntries = entries.filter((x) =>
    isFilterActive ? x.visible : true
  )

  return (
    <div className={pageStyles.container}>
      <Head>
        <title>{toTitleCase(name)}</title>
        <link rel="icon" href="/favicon.ico" />
      </Head>

      <Header mode={mode} setMode={setMode} setEntry={setEntry} />

      <main className={pageStyles.main}>
        {mode === MODES.EDIT ? (
          <CreateEntryForm
            onSubmit={(x) => {
              handleEntiresChange([...entries.filter((y) => y.id !== x.id), x])
              setMode(MODES.VIEW)
            }}
            {...entry}
          />
        ) : (
          <div className={styles.grid}>
            {visibleEntries.length > 0 ? (
              visibleEntries.map((x) => (
                <div
                  key={x.id}
                  className={
                    x.visible ? styles.card : `${styles.card} ${styles.foggy}`
                  }
                >
                  <div className={styles.availability}>
                    {!x.visible ? (
                      <span>Available {x.availableAt}</span>
                    ) : (
                      <span>
                        {x.dismissedAt
                          ? `Last viewed ${DateTime.fromISO(
                              x.dismissedAt
                            ).toRelative()}`
                          : 'Never viewed'}
                      </span>
                    )}
                  </div>
                  <a
                    href={x.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={() => handleEntryClick(x)}
                  >
                    <h2 title={x.title}>{x.title} &rarr;</h2>
                    <p title={x.description}>{x.description}</p>
                  </a>

                  <div className={styles.controls}>
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
              ))
            ) : (
              <p className={styles.empty} title="Go outside!">
                Iru eksteren!
              </p>
            )}
          </div>
        )}
      </main>

      {/* TODO */}
      {/* - show Search input when viewing all entries */}
      {/* - focus on Search with `/` character press? */}
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
