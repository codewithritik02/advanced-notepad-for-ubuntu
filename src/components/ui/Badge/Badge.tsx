import React from "react";
import "./Badge.css";

export type BadgeVariant =
  | "default"
  | "accent"
  | "subtle"
  | "danger"
  | "warning"
  | "success";

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
  size?: "sm" | "md";
}

export const Badge: React.FC<BadgeProps> = ({
  variant = "default",
  size = "sm",
  children,
  className = "",
  ...rest
}) => {
  return (
    <span
      className={`app-badge app-badge-${variant} app-badge-${size} ${className}`}
      {...rest}
    >
      {children}
    </span>
  );
};

export interface TagProps extends React.HTMLAttributes<HTMLSpanElement> {
  name: string;
  onRemove?: () => void;
}

export const Tag: React.FC<TagProps> = ({
  name,
  onRemove,
  className = "",
  ...rest
}) => {
  return (
    <span className={`app-tag ${className}`} {...rest}>
      <span className="app-tag-label">#{name}</span>
      {onRemove && (
        <button
          type="button"
          className="app-tag-remove"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          title={`Remove #${name}`}
          aria-label={`Remove #${name}`}
        >
          ×
        </button>
      )}
    </span>
  );
};
