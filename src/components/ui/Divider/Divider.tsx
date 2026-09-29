import React from "react";
import "./Divider.css";

export interface DividerProps extends React.HTMLAttributes<HTMLDivElement> {
  orientation?: "horizontal" | "vertical";
  spacing?: "sm" | "md" | "lg";
}

export const Divider: React.FC<DividerProps> = ({
  orientation = "horizontal",
  spacing = "md",
  className = "",
  ...rest
}) => {
  return (
    <div
      role="separator"
      aria-orientation={orientation}
      className={`app-divider app-divider-${orientation} app-divider-spacing-${spacing} ${className}`}
      {...rest}
    />
  );
};
