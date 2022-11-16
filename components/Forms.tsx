import React, { forwardRef, useRef, useImperativeHandle } from 'react'
import PropTypes from 'prop-types'
import { useButton } from '@react-aria/button'
import styles from '../styles/Forms.module.css'

const Form = ({ children }: { children: React.ReactNode }) => {
  return <form className={styles.form}>{children}</form>
}

type HTMLInputExtended = React.InputHTMLAttributes<HTMLInputElement> & {
  ref?: React.ForwardedRef<HTMLInputElement>
}

interface InputProps {
  label: string
}

const Input = forwardRef<HTMLInputExtended, InputProps>((props, ref) => {
  const inputRef: React.RefObject<HTMLInputElement> = useRef()

  useImperativeHandle(ref, () => ({
    focus: () => inputRef.current.focus(),
    addEventListener: (
      type: string,
      listener: EventListenerOrEventListenerObject
    ) => inputRef.current.addEventListener(type, listener),
    className: inputRef.current.className,
  }))

  return (
    <div className={styles.container}>
      {label && <label className={styles.label}>{label}</label>}
      <input ref={inputRef} className={styles.input} {...props} />
    </div>
  )
})

// Input.propTypes = {
//   type: PropTypes.string.isRequired,
//   label: PropTypes.string,
//   value: PropTypes.oneOfType([PropTypes.string, PropTypes.number]).isRequired,
//   onChange: PropTypes.func.isRequired,
// }

// Input.defaultProps = {
//   type: 'text',
// }

interface ButtonProps {
  children: React.ReactNode
  label: string
  onClick: Function
}

const Button = (props: ButtonProps) => {
  const ref = useRef()
  const { buttonProps } = useButton(props, ref)

  return (
    <div className={styles['button-container']}>
      <button {...buttonProps} ref={ref} className={styles.button}>
        {props.children || 'Submit'}
      </button>
    </div>
  )
}

interface Option {
  id: string | number
  display: string
}

interface SelectProps {
  label: string
  value: string | number
  options: Option[]
  onChange: Function
}

const Select = ({ label, value, options, onChange }: SelectProps) => {
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

interface TextareaProps {
  label: string
  value: string
  onChange: Function
  // onChange: React.ChangeEventHandler<HTMLTextAreaElement>
}

const Textarea = ({ label, value, onChange }: TextareaProps) => {
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

export { Form, Input, Button, Select, Textarea }
