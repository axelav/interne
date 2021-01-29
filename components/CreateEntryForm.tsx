import React, { useState } from 'react'
import { v4 as uuidv4 } from 'uuid'
import { Form, Input, Select, Button } from './Forms'
import { toTitleCase } from '../utils/formatters'
import { INTERVALS } from '../utils/constants'
import { Entry } from '../pages/index'
import styles from '../styles/Forms.module.css'

const isValidUrl = (str: string) => {
  try {
    new URL(str)
  } catch (_) {
    return false
  }

  return true
}

interface CreateEntryFormInterface extends Entry {
  onSubmit: (entry: object) => void
}

const CreateEntryForm = ({ onSubmit, ...props }: CreateEntryFormInterface) => {
  const [url, setUrl] = useState(props.url || '')
  const [title, setTitle] = useState(props.title || '')
  const [description, setDescription] = useState(props.description || '')
  const [duration, setDuration] = useState(props.duration || '3')
  const [interval, setInterval] = useState(props.interval || INTERVALS.DAYS)
  const [error, setError] = useState('')

  const handleSubmit = () => {
    if (!url || !title) {
      setError('URL and Title are required.')
    } else if (!isValidUrl(url)) {
      setError('URL is invalid.')
    } else if (!duration) {
      setError('Duration is required.')
    } else if (duration < 1) {
      setError('Duration must be greater than 0.')
    } else {
      setError('')

      const entry = {
        id: props.id || uuidv4(),
        url,
        title,
        description,
        duration,
        interval,
        visited: 0,
        createdAt: props.createdAt || new Date().toISOString(),
        updatedAt: props.createdAt ? new Date().toISOString() : null,
        dismissedAt: props.dismissedAt || null,
      }

      onSubmit(entry)

      setUrl('')
      setTitle('')
      setDescription('')
      setDuration(3)
      setInterval(INTERVALS.DAYS)
    }
  }

  return (
    <Form>
      {!!error && <div className={styles.error}>{error}</div>}
      <Input
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
      />
    </Form>
  )
}

export default CreateEntryForm
