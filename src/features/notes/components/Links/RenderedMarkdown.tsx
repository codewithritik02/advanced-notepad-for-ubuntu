import React, { useMemo } from "react";
import {
  parseMarkdown,
  type MdBlock,
  type MdInline,
  type MdAlign,
} from "../../../../utils/markdown";
import { InternalLinkPill } from "./InternalLinkPill";
import { logger } from "../../../../utils/logger";
import "./RenderedMarkdown.css";

export interface RenderedMarkdownProps {
  content: string;
  onActivateLink?: (targetNoteId: string) => void;
}

/** Opens an external link in the system browser, never inside the app webview. */
async function openExternal(href: string): Promise<void> {
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(href);
  } catch (error) {
    logger.warn("Could not open external link", { error: String(error) });
  }
}

function renderInline(
  nodes: MdInline[],
  onActivateLink: ((id: string) => void) | undefined,
  keyPrefix: string,
): React.ReactNode[] {
  return nodes.map((node, index) => {
    const key = `${keyPrefix}-${index}`;
    switch (node.t) {
      case "text":
        return <React.Fragment key={key}>{node.v}</React.Fragment>;
      case "br":
        return <br key={key} />;
      case "code":
        return (
          <code key={key} className="md-inline-code">
            {node.v}
          </code>
        );
      case "strong":
        return <strong key={key}>{renderInline(node.c, onActivateLink, key)}</strong>;
      case "em":
        return <em key={key}>{renderInline(node.c, onActivateLink, key)}</em>;
      case "del":
        return <del key={key}>{renderInline(node.c, onActivateLink, key)}</del>;
      case "image":
        return (
          <span key={key} className="md-image-placeholder" title="Images are not loaded in previews">
            {node.alt ? `🖼 ${node.alt}` : "🖼 image"}
          </span>
        );
      case "internal":
        return (
          <InternalLinkPill
            key={key}
            targetId={node.id}
            label={node.label}
            onActivate={onActivateLink}
          />
        );
      case "link":
        return (
          <a
            key={key}
            className="md-link"
            href={node.href}
            title={node.href}
            rel="noopener noreferrer"
            onClick={(event) => {
              event.preventDefault();
              void openExternal(node.href);
            }}
          >
            {renderInline(node.c, onActivateLink, key)}
          </a>
        );
      default:
        return null;
    }
  });
}

function cellAlign(align: MdAlign): React.CSSProperties | undefined {
  return align ? { textAlign: align } : undefined;
}

function renderBlocks(
  blocks: MdBlock[],
  onActivateLink: ((id: string) => void) | undefined,
  keyPrefix: string,
): React.ReactNode[] {
  return blocks.map((block, index) => {
    const key = `${keyPrefix}-${index}`;
    switch (block.t) {
      case "heading": {
        const Tag = `h${block.level}` as "h1" | "h2" | "h3" | "h4" | "h5" | "h6";
        return (
          <Tag key={key} className={`md-heading md-h${block.level}`}>
            {renderInline(block.c, onActivateLink, key)}
          </Tag>
        );
      }
      case "paragraph":
        return (
          <p key={key} className="md-paragraph">
            {renderInline(block.c, onActivateLink, key)}
          </p>
        );
      case "code":
        return (
          <pre key={key} className="md-code-block" data-lang={block.lang || undefined}>
            <code>{block.v}</code>
          </pre>
        );
      case "quote":
        return (
          <blockquote key={key} className="md-quote">
            {renderBlocks(block.c, onActivateLink, key)}
          </blockquote>
        );
      case "hr":
        return <hr key={key} className="md-hr" />;
      case "list": {
        const Tag = block.ordered ? "ol" : "ul";
        const isTaskList = block.items.some((item) => item.checked !== null);
        return (
          <Tag
            key={key}
            className={`md-list ${isTaskList ? "md-task-list" : ""}`}
            {...(block.ordered && block.start !== 1 ? { start: block.start } : {})}
          >
            {block.items.map((item, itemIndex) => (
              <li
                key={`${key}-${itemIndex}`}
                className={item.checked !== null ? "md-task-item" : undefined}
              >
                {item.checked !== null && (
                  <input
                    type="checkbox"
                    className="md-task-checkbox"
                    checked={item.checked}
                    readOnly
                    disabled
                    aria-label={item.checked ? "Completed task" : "Open task"}
                  />
                )}
                <div className="md-list-item-body">
                  {renderBlocks(item.c, onActivateLink, `${key}-${itemIndex}`)}
                </div>
              </li>
            ))}
          </Tag>
        );
      }
      case "table":
        return (
          <div key={key} className="md-table-wrap">
            <table className="md-table">
              <thead>
                <tr>
                  {block.head.map((cell, c) => (
                    <th key={`${key}-h${c}`} style={cellAlign(block.align[c] ?? null)}>
                      {renderInline(cell, onActivateLink, `${key}-h${c}`)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {block.rows.map((row, r) => (
                  <tr key={`${key}-r${r}`}>
                    {row.map((cell, c) => (
                      <td key={`${key}-r${r}c${c}`} style={cellAlign(block.align[c] ?? null)}>
                        {renderInline(cell, onActivateLink, `${key}-r${r}c${c}`)}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        );
      default:
        return null;
    }
  });
}

/**
 * Renders Markdown note content as formatted, read-only elements. Falls back to plain text when
 * the note is too large to parse safely.
 */
export const RenderedMarkdown: React.FC<RenderedMarkdownProps> = ({
  content,
  onActivateLink,
}) => {
  const blocks = useMemo(() => parseMarkdown(content), [content]);

  if (blocks === null) {
    return (
      <div className="md-too-large">
        <p className="md-too-large-notice">
          This note is too large to format in the preview. Showing plain text.
        </p>
        <div className="md-plain">{content}</div>
      </div>
    );
  }

  return <div className="md-preview">{renderBlocks(blocks, onActivateLink, "md")}</div>;
};
