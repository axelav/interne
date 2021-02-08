import React from 'react'
import Link from 'next/link'
import { version } from '../package.json'
// import styles from '../styles/Footer.module.css'

const Footer = () => (
  <footer
    className="flex flex-col justify-center items-center border-t border-gray-300 w-full font-serif italic leading-normal text-black text-opacity-30"
    style={{ height: '100px' }}
  >
    <div>v{version}</div>
    <Link href="/data">
      <a>Import/Export Data</a>
    </Link>
    <a href="https://honkytonk.in" target="_blank" rel="noopener noreferrer">
      Powered by honkytonkin'
    </a>
  </footer>
)

export default Footer
