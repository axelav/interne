import React from 'react'
import PropTypes from 'prop-types'
import '../styles/globals.css'

const Interior = ({ Component, pageProps }) => {
  return <Component {...pageProps} />
}

Interior.propTypes = {
  Component: PropTypes.func.isRequired,
  pageProps: PropTypes.object.isRequired,
}

export default Interior
