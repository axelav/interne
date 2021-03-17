import React from 'react'
import Link from 'next/link'
import package from '../package.json'
import styles from '../styles/Footer.module.css'

const Footer = () => (
  <footer className={styles.footer}>
    <div>v{package.version}</div>
    <Link href="/data">
      <a>Import/Export Data</a>
    </Link>
    <a href="https://honkytonk.in" target="_blank" rel="noopener noreferrer">
      Powered by honkytonkin'
    </a>
  </footer>
)

export default Footer
