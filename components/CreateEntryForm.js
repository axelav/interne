import React, { useState, useEffect, useRef, useCallback } from 'react'
import PropTypes from 'prop-types'
import { v4 as uuidv4 } from 'uuid'
import { Form, Input, Select, Button } from './Forms'
import { toTitleCase } from '../utils/formatters'
import { INTERVALS } from '../utils/constants'
import { KEY_CODES } from '../utils/constants'
import styles from '../styles/Forms.module.css'

const isValidUrl = (str) => {
  try {
    new URL(str)
  } catch (_) {
    return false
  }

  return true
}

const CreateEntryForm = ({ onSubmit, entries, ...props }) => {
  const [url, setUrl] = useState(props.url || '')
  const [title, setTitle] = useState(props.title || '')
  const [description, setDescription] = useState(props.description || '')
  const [duration, setDuration] = useState(props.duration || '3')
  const [interval, setInterval] = useState(props.interval || INTERVALS.DAYS)
  const [error, setError] = useState('')

  const urlInputRef = useRef(null)

  useEffect(() => {
    urlInputRef.current.focus()
  }, [])

  const handleSubmit = useCallback(() => {
    if (!url || !title) {
      setError('URL and Title are required.')
    } else if (
      !props.id &&
      entries.map((x) => new URL(x.url).href).includes(new URL(url).href)
    ) {
      setError('URL already exists.')
    } else if (!isValidUrl(url)) {
      setError('URL is invalid.')
    } else if (!duration) {
      setError('Duration is required.')
    } else if (duration < 1) {
      setError('Duration must be greater than 0.')
    } else {
      setError('')

      const now = new Date()

      const entry = {
        url: new URL(url).href,
        title,
        description,
        duration,
        interval,
        visited: 0,
        id: props.id || uuidv4(),
        createdAt: props.createdAt || now.toISOString(),
        updatedAt: props.createdAt ? now.toISOString() : null,
        dismissedAt: props.dismissedAt || null,
      }

      onSubmit(entry)

      setUrl('')
      setTitle('')
      setDescription('')
      setDuration(3)
      setInterval(INTERVALS.DAYS)
    }
  }, [
    entries,
    description,
    duration,
    interval,
    title,
    url,
    onSubmit,
    props.id,
    props.createdAt,
    props.dismissedAt,
  ])

  useEffect(() => {
    const handleKeydown = ({ keyCode }) => {
      if (keyCode === KEY_CODES.ENTER) {
        handleSubmit()
      }
    }

    document.addEventListener('keydown', handleKeydown)

    return () => document.removeEventListener('keydown', handleKeydown)
  }, [handleSubmit])

  return (
    <Form>
      {!!error && <div className={styles.error}>{error}</div>}
      <Input
        type="url"
        ref={urlInputRef}
        value={url}
        label="URL"
        placeholder="http://example.com"
        onChange={setUrl}
      />
      <Input value={title} label="Title" onChange={setTitle} />
      <Input
        value={description}
        label="Description"
        onChange={setDescription}
      />
      <Input
        type="number"
        pattern="[0-9]*"
        value={duration}
        label="Duration"
        onChange={setDuration}
        min={1}
      />
      <Select
        label="Interval"
        value={interval}
        onChange={setInterval}
        options={Object.keys(INTERVALS).map((x) => ({
          id: INTERVALS[x],
          display: toTitleCase(x),
        }))}
      />
      <Button
        label={props.id ? 'Edit Entry' : 'Add Entry'}
        onClick={handleSubmit}
      >
        {props.id ? 'Edit Entry' : 'Add Entry'}
      </Button>
    </Form>
  )
}

CreateEntryForm.propTypes = {
  onSubmit: PropTypes.func.isRequired,
  entries: PropTypes.arrayOf(PropTypes.object).isRequired,
  id: PropTypes.string,
  url: PropTypes.string,
  title: PropTypes.string,
  description: PropTypes.string,
  duration: PropTypes.string,
  interval: PropTypes.oneOf([
    INTERVALS.HOURS,
    INTERVALS.DAYS,
    INTERVALS.WEEKS,
    INTERVALS.MONTHS,
    INTERVALS.YEARS,
  ]),
  createdAt: PropTypes.string,
  dismissedAt: PropTypes.string,
}

export default CreateEntryForm
