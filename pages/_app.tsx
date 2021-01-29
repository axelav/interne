import React from 'react'
import { AppProps } from 'next/app'
import '../styles/globals.css'

const Interne = ({ Component, pageProps }: AppProps): React.ReactNode => {
  return <Component {...pageProps} />
}

export default Interne
