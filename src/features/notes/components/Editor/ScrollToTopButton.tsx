import React, { useEffect, useRef, useState } from "react";
import { animateScrollTo, type ScrollAnimation } from "../../utils/scrollAnimation";
import "./EditorScrollTools.css";

export interface ScrollToTopButtonProps {
  scrollRef: React.RefObject<HTMLElement | null>;
  /** Show the button once the note is scrolled further than this many pixels. */
  threshold?: number;
}

/** Floating "back to top" button, like on websites. Fades in after scrolling and glides to the top. */
export const ScrollToTopButton: React.FC<ScrollToTopButtonProps> = ({
  scrollRef,
  threshold = 320,
}) => {
  const [visible, setVisible] = useState(false);
  const animationRef = useRef<ScrollAnimation | null>(null);

  useEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      setVisible(element.scrollTop > threshold);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    element.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      element.removeEventListener("scroll", onScroll);
      if (frame) cancelAnimationFrame(frame);
      animationRef.current?.cancel();
    };
  }, [scrollRef, threshold]);

  const handleClick = () => {
    const element = scrollRef.current;
    if (!element) return;
    animationRef.current?.cancel();
    animationRef.current = animateScrollTo(element, 0);
  };

  return (
    <button
      type="button"
      className={`scroll-to-top-btn ${visible ? "is-visible" : ""}`}
      aria-label="Scroll to top of note"
      title="Back to top"
      tabIndex={visible ? 0 : -1}
      aria-hidden={visible ? undefined : true}
      onClick={handleClick}
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="m18 15-6-6-6 6" />
        <path d="M6 5h12" />
      </svg>
    </button>
  );
};

export default ScrollToTopButton;
