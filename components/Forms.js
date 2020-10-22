import React from 'react'
import PropTypes from 'prop-types'
import styles from '../styles/Forms.module.css'

const Form = ({ children }) => {
  return <form className={styles.form}>{children}</form>
}

Form.propTypes = {
  children: PropTypes.node.isRequired,
}

const Input = ({ type, label, value, onChange, ...props }) => {
  return (
    <div className={styles.container}>
      {!!label && <label className={styles.label}>{label}</label>}
      <input
        className={styles.input}
        type={type}
        value={value}
        onChange={(x) => onChange(x.currentTarget.value)}
        {...props}
      />
    </div>
  )
}

Input.propTypes = {
  type: PropTypes.string.isRequired,
  label: PropTypes.string,
  value: PropTypes.oneOfType([PropTypes.string, PropTypes.number]).isRequired,
  onChange: PropTypes.func.isRequired,
}

Input.defaultProps = {
  type: 'text',
}

const Button = ({ label, onClick }) => {
  return (
    <div className={styles['button-container']}>
      <button className={styles.button} type="button" onClick={onClick}>
        {label}
      </button>
    </div>
  )
}

Button.propTypes = {
  label: PropTypes.string.isRequired,
  onClick: PropTypes.func.isRequired,
}

Button.defaultProps = {
  label: 'Submit',
}

const Select = ({ label, value, options, onChange }) => {
  return (
    <div className={styles.container}>
      {!!label && <label className={styles.label}>{label}</label>}
      <select
        className={styles.select}
        value={value}
        onChange={(x) => onChange(x.currentTarget.value)}
      >
        {options.map((x) => (
          <option key={x.id} value={x.id}>
            {x.display}
          </option>
        ))}
      </select>
    </div>
  )
}

Select.propTypes = {
  value: PropTypes.oneOfType([PropTypes.string, PropTypes.number]).isRequired,
  options: PropTypes.arrayOf(
    PropTypes.shape({
      id: PropTypes.oneOfType([PropTypes.string, PropTypes.number]),
      display: PropTypes.string.isRequired,
    })
  ).isRequired,
  label: PropTypes.string,
  onChange: PropTypes.func.isRequired,
}

const Textarea = ({ label, value, onChange }) => {
  return (
    <div className={styles.container}>
      {!!label && <label className={styles.label}>{label}</label>}
      <textarea
        className={styles.textarea}
        value={value}
        onChange={(x) => onChange(x.currentTarget.value)}
      />
    </div>
  )
}

Textarea.propTypes = {
  label: PropTypes.string,
  value: PropTypes.oneOfType([PropTypes.string, PropTypes.number]).isRequired,
  onChange: PropTypes.func.isRequired,
}

export { Form, Input, Button, Select, Textarea }
