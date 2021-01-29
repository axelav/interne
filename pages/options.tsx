import React, { useEffect } from 'react'
import Head from 'next/head'
import { useRouter } from 'next/router'
import Header from '../components/Header'
import Footer from '../components/Footer'
import { Form, Button } from '../components/Forms'
import { toTitleCase } from '../utils/formatters'
import { name } from '../package.json'
import { KEY_CODES } from '../utils/constants'
import pageStyles from '../styles/Pages.module.css'

const Options = () => {
  const router = useRouter()

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
    router.push('/')
  }

  return (
    <div className={pageStyles.container}>
      <Head>
        <title>Options — {toTitleCase(name)}</title>
        <link rel="icon" href="/favicon.ico" />
      </Head>

      <Header />

      <main className={pageStyles.main}>
        <Form>
          <Button label="Save" onClick={handleSave} />
        </Form>
      </main>

      <Footer />
    </div>
  )
}

export default Options
