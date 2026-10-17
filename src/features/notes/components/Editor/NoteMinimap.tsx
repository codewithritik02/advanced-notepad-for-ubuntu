import React, { useCallback, useEffect, useRef, useState } from "react";
import { minimapScale, pointerToScrollTop, sliderRect } from "../../utils/minimapMath";
import { layoutTextWords, type MinimapKind } from "../../utils/minimapText";
import "./EditorScrollTools.css";

export const MINIMAP_WIDTH_PX = 88;
const MAX_PREVIEW_WORDS = 12000;
const MIN_HOST_WIDTH_PX = 620;

export interface NoteMinimapProps {
  /** The scrolling element that holds the note (the minimap mirrors and controls it). */
  scrollRef: React.RefObject<HTMLElement | null>;
  /** The note's text and view mode: the minimap redraws when either changes. */
  content: string;
  viewMode: string;
  onVisibilityChange?: (visible: boolean) => void;
}

interface CanvasColors {
  text: string;
  strong: string;
  accent: string;
  muted: string;
}

function readColors(): CanvasColors {
  const style = getComputedStyle(document.documentElement);
  const get = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    text: get("--color-text-secondary", "#64748b"),
    strong: get("--color-text-primary", "#1e293b"),
    accent: get("--color-accent-default", "#4f46e5"),
    muted: get("--color-text-muted", "#94a3b8"),
  };
}

/**
 * Overview strip of the whole note beside the editor (like VS Code's minimap).
 * Click anywhere to jump there; drag the highlighted window to scroll up and down.
 */
export const NoteMinimap: React.FC<NoteMinimapProps> = ({
  scrollRef,
  content,
  viewMode,
  onVisibilityChange,
}) => {
  const trackRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const sliderRef = useRef<HTMLDivElement>(null);
  const scaleRef = useRef(0.2);
  const frameRef = useRef(0);
  const dragRef = useRef<{ pointerId: number; grabOffset: number } | null>(null);
  const [visible, setVisible] = useState(false);
  const visibleRef = useRef(false);
  const [dragging, setDragging] = useState(false);

  const updateSlider = useCallback(() => {
    const scroller = scrollRef.current;
    const slider = sliderRef.current;
    if (!scroller || !slider) return;
    const rect = sliderRect(scroller.scrollTop, scroller.clientHeight, scaleRef.current);
    slider.style.transform = `translateY(${rect.top}px)`;
    slider.style.height = `${rect.height}px`;
  }, [scrollRef]);

  const draw = useCallback(() => {
    const scroller = scrollRef.current;
    const track = trackRef.current;
    const canvas = canvasRef.current;
    if (!scroller || !track || !canvas) return;

    const shouldShow =
      scroller.scrollHeight > scroller.clientHeight + 24 &&
      scroller.clientWidth + (visibleRef.current ? MINIMAP_WIDTH_PX : 0) >= MIN_HOST_WIDTH_PX;
    if (shouldShow !== visibleRef.current) {
      visibleRef.current = shouldShow;
      setVisible(shouldShow);
      onVisibilityChange?.(shouldShow);
    }
    if (!shouldShow) return;

    const width = MINIMAP_WIDTH_PX;
    const height = Math.max(1, scroller.clientHeight);
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * dpr) || canvas.height !== Math.round(height * dpr)) {
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);

    const scale = minimapScale(scroller.scrollHeight, height);
    scaleRef.current = scale;
    const colors = readColors();
    const padX = 8;
    const usable = width - padX * 2;
    const scRect = scroller.getBoundingClientRect();
    const toY = (el: Element) => el.getBoundingClientRect().top - scRect.top + scroller.scrollTop;

    const bar = (y: number, h: number, x: number, w: number, color: string, alpha: number) => {
      ctx.globalAlpha = alpha;
      ctx.fillStyle = color;
      ctx.fillRect(padX + x, y * scale, Math.max(1, w), Math.max(1, h * scale));
    };

    const doc = scroller.querySelector(".editor-document");
    if (doc) {
      for (const child of Array.from(doc.children)) {
        const rect = child.getBoundingClientRect();
        if (rect.height < 2) continue;
        const y = toY(child);
        if (child.classList.contains("editor-body-wrap")) continue;
        bar(y, rect.height, 0, usable, colors.muted, 0.12);
      }
    }

    // One small box per real word, so the strip looks like a tiny copy of the actual text.
    const docEl = (doc as HTMLElement | null) ?? scroller;
    const docRect = docEl.getBoundingClientRect();
    const sx = usable / Math.max(1, docRect.width);
    const kindStyle: Record<MinimapKind, { color: string; alpha: number }> = {
      text: { color: colors.text, alpha: 0.55 },
      strong: { color: colors.strong, alpha: 0.9 },
      code: { color: colors.muted, alpha: 0.6 },
      link: { color: colors.accent, alpha: 0.8 },
      quote: { color: colors.muted, alpha: 0.45 },
      marker: { color: colors.muted, alpha: 0.5 },
      image: { color: colors.accent, alpha: 0.45 },
    };
    const word = (xPx: number, yPx: number, wPx: number, hPx: number, kind: MinimapKind) => {
      const style = kindStyle[kind];
      ctx.globalAlpha = style.alpha;
      ctx.fillStyle = style.color;
      ctx.fillRect(
        padX + xPx * sx,
        yPx * scale,
        Math.max(1, wPx * sx),
        Math.max(1, hPx * scale * (kind === "strong" ? 1.2 : 1)),
      );
    };

    const textarea = scroller.querySelector<HTMLTextAreaElement>("textarea.editor-content-area");
    const preview = scroller.querySelector<HTMLElement>(".md-preview, .editor-preview-area");

    if (textarea) {
      // Edit mode: wrap the text exactly like the editor does and draw every word.
      const style = getComputedStyle(textarea);
      const fontSize = parseFloat(style.fontSize) || 14;
      const lineHeight = parseFloat(style.lineHeight) || fontSize * 1.6;
      const measureCtx = document.createElement("canvas").getContext("2d");
      const taRect = textarea.getBoundingClientRect();
      const left = taRect.left - docRect.left;
      const top = toY(textarea);
      if (measureCtx) {
        measureCtx.font = `${style.fontWeight} ${fontSize}px ${style.fontFamily}`;
        const layout = layoutTextWords(textarea.value, {
          measure: (text) => measureCtx.measureText(text).width,
          availableWidth: textarea.clientWidth,
          lineHeight,
        });
        if (layout.truncated) {
          // Too big to draw word by word: show the text as one soft block.
          bar(top, taRect.height, left * sx, usable - left * sx, colors.text, 0.25);
        } else {
          for (const box of layout.boxes) {
            word(left + box.x, top + box.y + lineHeight * 0.15, box.w, box.h, box.kind);
          }
        }
      }
    } else if (preview) {
      // Preview mode: use the real position of every rendered word.
      const root = preview.querySelector<HTMLElement>(".md-preview") ?? preview;
      const kindOf = (el: Element): MinimapKind => {
        if (el.closest("h1,h2,h3,h4,h5,h6,strong,b")) return "strong";
        if (el.closest("pre,code")) return "code";
        if (el.closest("a,[class*='link-pill']")) return "link";
        if (el.closest("blockquote,del")) return "quote";
        return "text";
      };

      root.querySelectorAll("pre").forEach((block) => {
        bar(toY(block), block.getBoundingClientRect().height, 0, usable, colors.muted, 0.12);
      });
      root
        .querySelectorAll<HTMLElement>(
          "img.attachment-image:not(.is-full), .attachment-image-skeleton, .attachment-image-missing",
        )
        .forEach((image) => {
          const rect = image.getBoundingClientRect();
          word((rect.left - docRect.left), toY(image), rect.width, rect.height, "image");
        });

      const range = document.createRange();
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      let drawn = 0;
      let node = walker.nextNode() as Text | null;
      while (node && drawn < MAX_PREVIEW_WORDS) {
        const parent = node.parentElement;
        if (parent && !parent.closest("script,style,.attachment-image-missing")) {
          const kind = kindOf(parent);
          for (const match of node.data.matchAll(/\S+/g)) {
            range.setStart(node, match.index ?? 0);
            range.setEnd(node, (match.index ?? 0) + match[0].length);
            const rect = range.getBoundingClientRect();
            if (rect.width <= 0 || rect.height <= 0) continue;
            word(
              rect.left - docRect.left,
              rect.top - scRect.top + scroller.scrollTop + rect.height * 0.15,
              rect.width,
              rect.height * 0.7,
              kind,
            );
            drawn += 1;
            if (drawn >= MAX_PREVIEW_WORDS) break;
          }
        }
        node = walker.nextNode() as Text | null;
      }
      range.detach();
    }
    ctx.globalAlpha = 1;
    updateSlider();
  }, [scrollRef, updateSlider, onVisibilityChange]);

  const scheduleDraw = useCallback(() => {
    if (frameRef.current) return;
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = 0;
      draw();
    });
  }, [draw]);

  // Follow scrolling, resizing, theme changes and the note's own layout changes
  useEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    let sliderFrame = 0;
    const onScroll = () => {
      if (sliderFrame) return;
      sliderFrame = requestAnimationFrame(() => {
        sliderFrame = 0;
        updateSlider();
      });
    };
    scroller.addEventListener("scroll", onScroll, { passive: true });
    const resizeObserver = new ResizeObserver(scheduleDraw);
    resizeObserver.observe(scroller);
    const doc = scroller.querySelector(".editor-document");
    if (doc) resizeObserver.observe(doc);
    const themeObserver = new MutationObserver(scheduleDraw);
    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    scheduleDraw();
    return () => {
      scroller.removeEventListener("scroll", onScroll);
      resizeObserver.disconnect();
      themeObserver.disconnect();
      if (frameRef.current) cancelAnimationFrame(frameRef.current);
      if (sliderFrame) cancelAnimationFrame(sliderFrame);
      frameRef.current = 0;
    };
  }, [scrollRef, scheduleDraw, updateSlider]);

  // Redraw (debounced) when the text or view mode changes; once more later for late-loading images
  useEffect(() => {
    const quick = window.setTimeout(scheduleDraw, 120);
    const late = window.setTimeout(scheduleDraw, 600);
    return () => {
      window.clearTimeout(quick);
      window.clearTimeout(late);
    };
  }, [content, viewMode, scheduleDraw]);

  const scrollToPointer = (clientY: number, grabOffset?: number) => {
    const scroller = scrollRef.current;
    const track = trackRef.current;
    if (!scroller || !track) return;
    scroller.scrollTop = pointerToScrollTop({
      pointerY: clientY - track.getBoundingClientRect().top,
      clientHeight: scroller.clientHeight,
      scrollHeight: scroller.scrollHeight,
      scale: scaleRef.current,
      grabOffset,
    });
  };

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const scroller = scrollRef.current;
    const track = trackRef.current;
    if (!scroller || !track) return;
    event.preventDefault();
    try {
      track.setPointerCapture(event.pointerId);
    } catch {
      // Capture is only a convenience: dragging still works while the pointer stays over the strip.
    }
    const y = event.clientY - track.getBoundingClientRect().top;
    const rect = sliderRect(scroller.scrollTop, scroller.clientHeight, scaleRef.current);
    // Grabbing the window keeps your grip point; clicking elsewhere jumps there, centred.
    const insideSlider = y >= rect.top && y <= rect.top + rect.height;
    const grabOffset = insideSlider ? y - rect.top : rect.height / 2;
    dragRef.current = { pointerId: event.pointerId, grabOffset };
    setDragging(true);
    scrollToPointer(event.clientY, insideSlider ? grabOffset : undefined);
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    scrollToPointer(event.clientY, drag.grabOffset);
  };

  const endDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    dragRef.current = null;
    setDragging(false);
    try {
      trackRef.current?.releasePointerCapture(event.pointerId);
    } catch {
      // Already released
    }
  };

  return (
    <div
      ref={trackRef}
      className={`note-minimap ${visible ? "is-visible" : ""} ${dragging ? "is-dragging" : ""}`}
      aria-hidden="true"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onWheel={(event) => {
        const scroller = scrollRef.current;
        if (scroller) scroller.scrollTop += event.deltaY;
      }}
    >
      <canvas ref={canvasRef} className="note-minimap-canvas" />
      <div ref={sliderRef} className="note-minimap-slider" />
    </div>
  );
};

export default NoteMinimap;
