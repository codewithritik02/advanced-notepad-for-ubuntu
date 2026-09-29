import React from "react";
import "./Button.css";

export type ButtonVariant = "primary" | "secondary" | "subtle" | "danger" | "ghost";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: React.ReactNode;
  iconPosition?: "left" | "right";
  isLoading?: boolean;
}

export const Button: React.FC<ButtonProps> = ({
  variant = "secondary",
  size = "md",
  icon,
  iconPosition = "left",
  isLoading = false,
  disabled,
  children,
  className = "",
  ...rest
}) => {
  const isDisabled = disabled || isLoading;

  return (
    <button
      type="button"
      className={`app-btn app-btn-${variant} app-btn-${size} ${isLoading ? "is-loading" : ""} ${className}`}
      disabled={isDisabled}
      aria-disabled={isDisabled}
      {...rest}
    >
      {isLoading ? (
        <span className="btn-spinner" aria-hidden="true" />
      ) : (
        icon && iconPosition === "left" && <span className="btn-icon">{icon}</span>
      )}
      {children && <span className="btn-text">{children}</span>}
      {!isLoading && icon && iconPosition === "right" && (
        <span className="btn-icon">{icon}</span>
      )}
    </button>
  );
};

export interface IconButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  icon: React.ReactNode;
  title: string;
  size?: ButtonSize;
  variant?: ButtonVariant;
}

export const IconButton: React.FC<IconButtonProps> = ({
  icon,
  title,
  size = "md",
  variant = "ghost",
  className = "",
  ...rest
}) => {
  return (
    <button
      type="button"
      className={`app-icon-btn app-btn-${variant} app-icon-btn-${size} ${className}`}
      title={title}
      aria-label={title}
      {...rest}
    >
      {icon}
    </button>
  );
};
