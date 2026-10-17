import { useEffect, useState } from "react";
import { attachmentImageCache } from "../services/imageCache.ts";

export type AttachmentImageState =
  | { status: "loading" }
  | { status: "ready"; url: string }
  | { status: "error"; message: string };

/**
 * Loads an image attachment as a shared blob: URL. The URL is released again when the component
 * unmounts or the attachment id changes.
 */
export function useAttachmentImage(
  attachmentId: string | null,
  cache: Pick<typeof attachmentImageCache, "acquire"> = attachmentImageCache,
): AttachmentImageState {
  const [state, setState] = useState<AttachmentImageState>({ status: "loading" });

  useEffect(() => {
    if (!attachmentId) {
      setState({ status: "error", message: "No image selected." });
      return;
    }
    let cancelled = false;
    setState({ status: "loading" });
    const handle = cache.acquire(attachmentId);
    handle.promise.then(
      (url) => {
        if (!cancelled) setState({ status: "ready", url });
      },
      (error: unknown) => {
        if (!cancelled) {
          setState({
            status: "error",
            message: error instanceof Error ? error.message : String(error),
          });
        }
      },
    );
    return () => {
      cancelled = true;
      handle.release();
    };
  }, [attachmentId, cache]);

  return state;
}
