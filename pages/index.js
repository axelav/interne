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
  const [mode, setMode] = useState(MODES.VIEW)
  const [isFilterActive, setIsFilterActive] = useState(true)

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

  useEffect(() => {
    if (entries.length < 1) {
      const result = retrieveEntries()

      if (!!result) {
        handleEntiresChange(result)
      }
    }
  }, [entries])

  useEffect(() => {
    const interval = setInterval(() => handleEntiresChange(entries), 1000 * 60)
    return () => clearInterval(interval)
  }, [entries])

  const handleEntryClick = (entry) => {
    const nextEntries = entries.filter((x) => x.id !== entry.id)
    nextEntries.push({
      ...entry,
      visited: ++entry.visited,
      dismissedAt: new Date().toISOString(),
    })

    handleEntiresChange(nextEntries)
  }

  const handleViewFilterClick = () => setIsFilterActive(!isFilterActive)

  const visibleEntries = entries.filter((x) =>
    isFilterActive ? x.visible : true
  )

  return (
    <div className={pageStyles.container}>
      <Head>
        <title>{toTitleCase(name)}</title>
        <link rel="icon" href="/favicon.ico" />
      </Head>

      <Header mode={mode} setMode={setMode} />

      <main className={pageStyles.main}>
        {mode === MODES.CREATE ? (
          <CreateEntryForm
            onSubmit={(x) => {
              handleEntiresChange([...entries, x])
              setMode(MODES.VIEW)
            }}
          />
        ) : (
          <div className={styles.grid}>
            {visibleEntries.length > 0 ? (
              visibleEntries.map((x) => (
                <a
                  key={x.id}
                  className={
                    x.visible ? styles.card : `${styles.card} ${styles.foggy}`
                  }
                  href={x.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={() => handleEntryClick(x)}
                >
                  <h3>{x.title} &rarr;</h3>
                  <p title={x.description}>{x.description}</p>
                  {!x.visible && <div>Available {x.availableAt}</div>}
                </a>
              ))
            ) : (
              <p className={styles.empty} title="Go outside!">
                Iru eksteren!
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
            left: isFilterActive ? '-14px' : '-28px',
          }}
        >
          {isFilterActive ? 'View All' : <span>Viewing All</span>}
        </div>
      )}

      <Footer />
    </div>
  )
}

export default Index
