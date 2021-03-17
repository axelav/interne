import React, { useState, useEffect } from 'react'
import Head from 'next/head'
import { useRouter } from 'next/router'
import { v4 as uuidv4 } from 'uuid'
import Header from '../components/Header'
import Footer from '../components/Footer'
import omit from 'lodash.omit'
import { Form, Textarea, Button } from '../components/Forms'
import { saveEntries, retrieveEntries } from '../services/storage'
import { toTitleCase } from '../utils/formatters'
import package from '../package.json'
import { INTERVALS, KEY_CODES } from '../utils/constants'
import pageStyles from '../styles/Pages.module.css'
import formStyles from '../styles/Forms.module.css'

const Data = () => {
  const [entries, setEntries] = useState('')
  const [error, setError] = useState('')
  const router = useRouter()

  useEffect(() => {
    if (entries.length < 1) {
      const result = retrieveEntries()

      if (!!result) {
        setEntries(
          JSON.stringify(
            result.map((x) => omit(x, ['diff', 'visible', 'availableAt'])),
            null,
            2
          )
        )
      }
    }
  }, [entries])

  useEffect(() => {
    const handleKeydown = ({ keyCode }) => {
      if (keyCode === KEY_CODES.ESC) {
        if (document.activeElement === document.body) {
          router.push('/')
        }
      }
    }

    document.addEventListener('keydown', handleKeydown)

    return () => document.removeEventListener('keydown', handleKeydown)
  }, [router])

  const handleSave = () => {
    let result

    try {
      result = JSON.parse(entries)
    } catch (err) {
      setError('Invalid JSON.')

      return
    }

    setError('')
    saveEntries(
      result.map((x) => {
        if (!x.id) {
          x.id = uuidv4()
          x.visited = 0
          x.createdAt = new Date().toISOString()
          x.updatedAt = null
          x.dismissedAt = null
          x.duration = !x.duration || '3'
          x.interval = !x.interval || INTERVALS.DAYS
        }

        return omit(x, ['diff', 'visible', 'availableAt'])
      })
    )

    router.push('/')
  }

  return (
    <div className={pageStyles.container}>
      <Head>
        <title>Data — {toTitleCase(package.name)}</title>
        <link rel="icon" href="/favicon.ico" />
      </Head>

      <Header />

      <main className={pageStyles.main}>
        <Form>
          {!!error && <div className={formStyles.error}>{error}</div>}
          <Textarea value={entries} label="Your Data" onChange={setEntries} />
          <Button onPress={handleSave}>Import</Button>
        </Form>
      </main>

      <Footer />
    </div>
  )
}

export default Data
