import React, { forwardRef, useRef, useImperativeHandle } from 'react'
import styles from '../styles/Forms.module.css'

interface Option {
  id: number | string
  display: string
}

const Form = ({ children }: { children: React.ReactNode }) => {
  return <form className={styles.form}>{children}</form>
}

const Input = forwardRef(
  (
    {
      type,
      label,
      value,
      onChange,
      ...props
    }: {
      type?: string
      label?: string
      placeholder?: string
      min?: number
      value: string | number
      onChange: (value: string) => void
    },
    ref
  ) => {
    const inputRef = useRef<HTMLInputElement>()

    useImperativeHandle(ref, () => ({
      focus: () => inputRef.current.focus(),
      addEventListener: (type: string, listener: (ev: KeyboardEvent) => void) =>
        inputRef.current.addEventListener(type, listener),
      className: inputRef.current.className,
    }))

    return (
      <div className={styles.container}>
        {!!label && <label className={styles.label}>{label}</label>}
        <input
          ref={inputRef}
          className={styles.input}
          type={type}
          value={value}
          onChange={(ev: React.FormEvent<HTMLInputElement>) =>
            onChange(ev.currentTarget.value)
          }
          {...props}
        />
      </div>
    )
  }
)

Input.defaultProps = {
  type: 'text',
}

const Button = ({ label, onClick }: { label: string; onClick: () => void }) => {
  return (
    <div className={styles['button-container']}>
      <button className={styles.button} type="button" onClick={onClick}>
        {label}
      </button>
    </div>
  )
}

Button.defaultProps = {
  label: 'Submit',
}

const Select = ({
  label,
  value,
  options,
  onChange,
}: {
  label?: string
  value: string | number
  options: Option[]
  onChange: (value: string) => void
}) => {
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

const Textarea = ({
  label,
  value,
  onChange,
}: {
  label?: string
  value: string | number
  onChange: (value: string) => void
}) => {
  return (
    <div className={styles.container}>
      {!!label && <label className={styles.label}>{label}</label>}
      <textarea
        className={styles.textarea}
        value={value}
        onChange={(ev: React.FormEvent<HTMLTextAreaElement>) =>
          onChange(ev.currentTarget.value)
        }
      />
    </div>
  )
}

export { Form, Input, Button, Select, Textarea }
