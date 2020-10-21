import React, { useState, useEffect } from 'react'
import Head from 'next/head'
// import sortBy from 'sort-by'
import { DateTime } from 'luxon'
import CreateEntryForm from '../components/CreateEntryForm'
import Header from '../components/Header'
import Footer from '../components/Footer'
import { saveEntries, retrieveEntries } from '../services/storage'
import { toTitleCase } from '../utils/formatters'
import { MODES } from '../utils/constants'
import { name } from '../package.json'
import styles from '../styles/Index.module.css'

// TODO refactor this, sorting, setting state, saving to storage, etc as a func
const setVisibility = (entry) => {
  let visible = true

  if (entry.dismissedAt) {
    const nextViewingDate = DateTime.fromISO(entry.dismissedAt).plus({
      [entry.interval]: entry.duration,
    })
    const diff = nextViewingDate.diffNow()

    visible = diff < 0
  }

  return {
    ...entry,
    visible,
  }
}

const Home = () => {
  const [entries, setEntries] = useState([])
  const [mode, setMode] = useState(MODES.VIEW)
  const [isFilterActive, setIsFilterActive] = useState(true)

  useEffect(() => {
    if (entries.length < 1) {
      const result = retrieveEntries()

      if (!!result) {
        const sorted = result.map(setVisibility)
        // TODO
        // .sort(sortBy('dismissedAt'))

        setEntries(sorted)
      }
    }
  }, [entries])

  const handleEntryClick = (entry) => {
    const nextEntries = entries.filter((x) => x.id !== entry.id)
    nextEntries.push({
      ...entry,
      visited: ++entry.visited,
      dismissedAt: new Date().toISOString(),
    })

    const sorted = nextEntries.map(setVisibility)

    setEntries(sorted)
    saveEntries(sorted)
  }

  const handleViewFilterClick = () => setIsFilterActive(!isFilterActive)

  return (
    <div className={styles.container}>
      <Head>
        <title>{toTitleCase(name)}</title>
        <link rel="icon" href="/favicon.ico" />
      </Head>

      <Header mode={mode} setMode={setMode} />

      <main className={styles.main}>
        {mode === MODES.CREATE ? (
          <CreateEntryForm
            onSubmit={(x) => {
              setEntries([...entries, x].map(setVisibility))
              saveEntries([...entries, x].map(setVisibility))
              setMode(MODES.VIEW)
            }}
          />
        ) : (
          <div className={styles.grid}>
            {entries
              .filter((x) => (isFilterActive ? x.visible : true))
              .map((x) => (
                <a
                  key={x.id}
                  className={styles.card}
                  href={x.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={() => handleEntryClick(x)}
                >
                  <h3>{x.title} &rarr;</h3>
                  <p title={x.description}>{x.description}</p>
                </a>
              ))}
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

export default Home
