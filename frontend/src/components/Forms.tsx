import React, { forwardRef } from 'react'
import { useButton } from '@react-aria/button'
import styles from '../styles/Forms.module.css'

interface FormProps {
  children: React.ReactNode
}

export function Form({ children }: FormProps) {
  return <form className={styles.form}>{children}</form>
}

interface InputProps {
  type?: string
  value: string | number
  label: string
  placeholder?: string
  onChange: (value: string) => void
  pattern?: string
  min?: number
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ type = 'text', value, label, placeholder, onChange, pattern, min }, ref) => {
    return (
      <div className={styles.field}>
        <label className={styles.label}>{label}</label>
        <input
          ref={ref}
          type={type}
          className={styles.input}
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
          pattern={pattern}
          min={min}
        />
      </div>
    )
  }
)

Input.displayName = 'Input'

interface SelectOption {
  id: string
  display: string
}

interface SelectProps {
  label: string
  value: string
  onChange: (value: string) => void
  options: SelectOption[]
}

export function Select({ label, value, onChange, options }: SelectProps) {
  return (
    <div className={styles.field}>
      <label className={styles.label}>{label}</label>
      <select className={styles.select} value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.display}
          </option>
        ))}
      </select>
    </div>
  )
}

interface ButtonProps {
  label: string
  onClick: () => void
  children: React.ReactNode
}

export function Button({ label, onClick, children }: ButtonProps) {
  const ref = React.useRef<HTMLButtonElement>(null)
  const { buttonProps } = useButton({ onPress: onClick }, ref)

  return (
    <button {...buttonProps} ref={ref} className={styles.button}>
      {children}
    </button>
  )
}
