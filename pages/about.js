import React from 'react'
import Head from 'next/head'
import Header from '../components/Header'
import Footer from '../components/Footer'
import { toTitleCase } from '../utils/formatters'
import { name } from '../package.json'
import pageStyles from '../styles/Pages.module.css'

const About = () => {
  return (
    <div className={pageStyles.container}>
      <Head>
        <title>About — {toTitleCase(name)}</title>
        <link rel="icon" href="/favicon.ico" />
      </Head>

      <Header />

      <main className={pageStyles.main}>
        <div>About Interne</div>
      </main>

      <Footer />
    </div>
  )
}

export default About
