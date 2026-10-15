import React, { useState } from "react";
import "./PasswordInput.css";

export interface PasswordInputProps {
  id: string;
  label?: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  error?: string;
  helpText?: string;
  disabled?: boolean;
  autoFocus?: boolean;
  autoComplete?: "current-password" | "new-password" | "off";
  onKeyDown?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  showToggle?: boolean;
  required?: boolean;
}

export const PasswordInput: React.FC<PasswordInputProps> = ({
  id,
  label,
  value,
  onChange,
  placeholder = "Enter password",
  error,
  helpText,
  disabled = false,
  autoFocus = false,
  autoComplete = "off",
  onKeyDown,
  showToggle = true,
  required = false,
}) => {
  const [showPassword, setShowPassword] = useState(false);

  const toggleShowPassword = () => {
    setShowPassword((prev) => !prev);
  };

  return (
    <div className={`password-input-group ${error ? "has-error" : ""}`}>
      {label && (
        <label htmlFor={id} className="password-input-label">
          {label}
          {required && <span className="password-required-mark">*</span>}
        </label>
      )}

      <div className="password-input-wrapper">
        <input
          id={id}
          type={showPassword ? "text" : "password"}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          disabled={disabled}
          autoFocus={autoFocus}
          autoComplete={autoComplete}
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          onKeyDown={onKeyDown}
          className="password-input-field"
          aria-invalid={Boolean(error)}
          aria-describedby={
            error ? `${id}-error` : helpText ? `${id}-help` : undefined
          }
        />

        {showToggle && (
          <button
            type="button"
            className="password-toggle-btn"
            onClick={toggleShowPassword}
            disabled={disabled}
            aria-label={showPassword ? "Hide password" : "Show password"}
            title={showPassword ? "Hide password" : "Show password"}
            tabIndex={-1}
          >
            {showPassword ? (
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" />
                <line x1="1" y1="1" x2="23" y2="23" />
              </svg>
            ) : (
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                <circle cx="12" cy="12" r="3" />
              </svg>
            )}
          </button>
        )}
      </div>

      {helpText && !error && (
        <span id={`${id}-help`} className="password-help-text">
          {helpText}
        </span>
      )}

      {error && (
        <span id={`${id}-error`} className="password-error-text" role="alert">
          {error}
        </span>
      )}
    </div>
  );
};
