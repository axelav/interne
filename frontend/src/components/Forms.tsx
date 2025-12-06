import React, { forwardRef, useRef, useImperativeHandle } from "react";
import { useButton } from "@react-aria/button";
import styles from "../styles/Forms.module.css";

interface FormProps {
  children: React.ReactNode;
}

export function Form({ children }: FormProps) {
  return <form className={styles.form}>{children}</form>;
}

interface InputProps {
  type?: string;
  value: string | number;
  label?: string;
  placeholder?: string;
  onChange: (value: string) => void;
  pattern?: string;
  min?: number;
}

export interface InputRef {
  focus: () => void;
  addEventListener: (type: string, listener: EventListener) => void;
  className: string;
}

export const Input = forwardRef<InputRef, InputProps>(
  (
    {
      type = "text",
      value,
      label,
      placeholder,
      onChange,
      pattern,
      min,
      ...props
    },
    ref,
  ) => {
    const inputRef = useRef<HTMLInputElement>(null);

    useImperativeHandle(ref, () => ({
      focus: () => inputRef.current?.focus(),
      addEventListener: (type: string, listener: EventListener) =>
        inputRef.current?.addEventListener(type, listener),
      className: inputRef.current?.className || "",
    }));

    return (
      <div className={styles.container}>
        {!!label && <label className={styles.label}>{label}</label>}
        <input
          ref={inputRef}
          type={type}
          className={styles.input}
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
          pattern={pattern}
          min={min}
          {...props}
        />
      </div>
    );
  },
);

Input.displayName = "Input";

interface SelectOption {
  id: string;
  display: string;
}

interface SelectProps {
  label?: string;
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
}

export function Select({ label, value, onChange, options }: SelectProps) {
  return (
    <div className={styles.container}>
      {!!label && <label className={styles.label}>{label}</label>}
      <select
        className={styles.select}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.display}
          </option>
        ))}
      </select>
    </div>
  );
}

interface ButtonProps {
  onClick?: () => void;
  children: React.ReactNode;
}

export function Button({ onClick, children }: ButtonProps) {
  const ref = React.useRef<HTMLButtonElement>(null);
  const { buttonProps } = useButton({ onPress: onClick }, ref);

  return (
    <div className={styles["button-container"]}>
      <button {...buttonProps} ref={ref} className={styles.button}>
        {children}
      </button>
    </div>
  );
}

interface TextareaProps {
  label?: string;
  value: string | number;
  onChange: (value: string) => void;
}

export function Textarea({ label, value, onChange }: TextareaProps) {
  return (
    <div className={styles.container}>
      {!!label && <label className={styles.label}>{label}</label>}
      <textarea
        className={styles.textarea}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}
