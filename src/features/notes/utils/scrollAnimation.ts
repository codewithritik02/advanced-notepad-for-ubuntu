/**
 * Smooth, cancellable scrolling for the "back to top" button.
 * Implemented with requestAnimationFrame so the duration and easing are identical in every webview
 * (native `behavior: "smooth"` varies between engines and cannot be cancelled).
 */

export function easeInOutCubic(t: number): number {
  const x = Math.min(1, Math.max(0, t));
  return x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2;
}

/** Longer trips take a little longer, but never feel sluggish. */
export function scrollDurationMs(distancePx: number): number {
  const distance = Math.abs(distancePx);
  if (distance < 1) return 0;
  return Math.round(Math.min(900, Math.max(320, 260 + Math.sqrt(distance) * 9)));
}

export function prefersReducedMotion(): boolean {
  try {
    return (
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches
    );
  } catch {
    return false;
  }
}

export interface ScrollAnimation {
  cancel: () => void;
}

const USER_INPUT_EVENTS = ["wheel", "touchstart", "pointerdown", "keydown"] as const;

/** Animates `element.scrollTop` to `targetTop`. Any user scroll input cancels it immediately. */
export function animateScrollTo(
  element: HTMLElement,
  targetTop: number,
  options: { durationMs?: number; reducedMotion?: boolean; onDone?: () => void } = {},
): ScrollAnimation {
  const startTop = element.scrollTop;
  const maxTop = Math.max(0, element.scrollHeight - element.clientHeight);
  const target = Math.min(maxTop, Math.max(0, targetTop));
  const reduced = options.reducedMotion ?? prefersReducedMotion();
  const duration = reduced ? 0 : (options.durationMs ?? scrollDurationMs(target - startTop));

  let frame = 0;
  let finished = false;

  const finish = () => {
    if (finished) return;
    finished = true;
    if (frame) cancelAnimationFrame(frame);
    for (const type of USER_INPUT_EVENTS) element.removeEventListener(type, cancel);
    options.onDone?.();
  };
  function cancel() {
    finish();
  }

  if (duration <= 0 || Math.abs(target - startTop) < 1) {
    element.scrollTop = target;
    finished = true;
    options.onDone?.();
    return { cancel: () => {} };
  }

  for (const type of USER_INPUT_EVENTS) {
    element.addEventListener(type, cancel, { passive: true });
  }

  const startedAt = performance.now();
  const step = (now: number) => {
    if (finished) return;
    const progress = (now - startedAt) / duration;
    element.scrollTop = startTop + (target - startTop) * easeInOutCubic(progress);
    if (progress >= 1) {
      element.scrollTop = target;
      finish();
      return;
    }
    frame = requestAnimationFrame(step);
  };
  frame = requestAnimationFrame(step);

  return { cancel };
}
