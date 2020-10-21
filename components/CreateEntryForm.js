import React, { useState } from 'react'
import PropTypes from 'prop-types'
import { v4 as uuidv4 } from 'uuid'
import { Form, Input, Select, Button } from './Forms'
import { toTitleCase } from '../utils/formatters'
import { INTERVALS } from '../utils/constants'
import styles from '../styles/Forms.module.css'

const CreateEntryForm = ({ onSubmit }) => {
  const [url, setUrl] = useState('')
  const [title, setTitle] = useState('')
  const [description, setDescription] = useState('')
  const [duration, setDuration] = useState(3)
  const [interval, setInterval] = useState(INTERVALS.DAYS)
  const [error, setError] = useState('')

  const handleSubmit = () => {
    if (!url || !title) {
      setError('URL and Title are required.')
    } else {
      setError('')

      const entry = {
        url,
        title,
        description,
        duration,
        interval,
        visited: 0,
        id: uuidv4(),
        createdAt: new Date().toISOString(),
        updatedAt: null,
        dismissedAt: null,
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
      <Input value={url} label="URL" onChange={setUrl} />
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
      <Button label="Add" onClick={handleSubmit} />
    </Form>
  )
}

CreateEntryForm.propTypes = {
  onSubmit: PropTypes.func.isRequired,
}

export default CreateEntryForm
