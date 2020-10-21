import React from 'react'
import PropTypes from 'prop-types'
import '../styles/globals.css'

const Interne = ({ Component, pageProps }) => {
  return <Component {...pageProps} />
}

Interne.propTypes = {
  Component: PropTypes.func.isRequired,
  pageProps: PropTypes.object.isRequired,
}

export default Interne
