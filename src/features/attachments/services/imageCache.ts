import { attachmentsStorage } from "../../../services/storage/attachments.ts";
import { sniffImageMime } from "../utils/imageMime.ts";

/**
 * Shared, reference-counted cache of blob: URLs for image attachments.
 *
 * Many components can show the same image (preview, thumbnail, lightbox) while the bytes are
 * fetched and decoded once. A URL is revoked a short while after its last user releases it, so
 * memory is returned but quick re-renders do not reload the image.
 */
export interface ImageCacheDeps {
  loadBytes: (attachmentId: string) => Promise<Uint8Array>;
  createUrl: (blob: Blob) => string;
  revokeUrl: (url: string) => void;
  /** Delay before an unused URL is revoked. */
  releaseDelayMs: number;
}

interface Entry {
  refs: number;
  promise: Promise<string>;
  url: string | null;
  timer: ReturnType<typeof setTimeout> | null;
}

export interface ImageHandle {
  promise: Promise<string>;
  release: () => void;
}

export function createImageCache(deps: ImageCacheDeps) {
  const entries = new Map<string, Entry>();

  const drop = (id: string, entry: Entry) => {
    if (entries.get(id) === entry) entries.delete(id);
    if (entry.timer) clearTimeout(entry.timer);
    entry.timer = null;
    if (entry.url) deps.revokeUrl(entry.url);
    entry.url = null;
  };

  const acquire = (attachmentId: string): ImageHandle => {
    const id = attachmentId.trim();
    let entry = entries.get(id);

    if (!entry) {
      const created: Entry = {
        refs: 0,
        url: null,
        timer: null,
        promise: Promise.resolve(""),
      };
      created.promise = (async () => {
        const bytes = await deps.loadBytes(id);
        const mime = sniffImageMime(bytes);
        if (!mime) throw new Error("This attachment is not a supported image.");
        // Copy into a fresh buffer so the Blob never aliases a shared ArrayBuffer.
        const url = deps.createUrl(new Blob([new Uint8Array(bytes)], { type: mime }));
        if (entries.get(id) === created) {
          created.url = url;
        } else {
          // invalidated while loading
          deps.revokeUrl(url);
          throw new Error("Image is no longer available.");
        }
        return url;
      })();
      // A failed load must not stay cached, so a later retry can succeed.
      created.promise.catch(() => {
        if (entries.get(id) === created) entries.delete(id);
      });
      entries.set(id, created);
      entry = created;
    }

    if (entry.timer) {
      clearTimeout(entry.timer);
      entry.timer = null;
    }
    entry.refs += 1;

    const owned = entry;
    let released = false;
    return {
      promise: owned.promise,
      release: () => {
        if (released) return;
        released = true;
        owned.refs -= 1;
        if (owned.refs > 0) return;
        owned.timer = setTimeout(() => {
          if (owned.refs <= 0) drop(id, owned);
        }, deps.releaseDelayMs);
      },
    };
  };

  /** Forget an attachment's image immediately (for example after the attachment was removed). */
  const invalidate = (attachmentId: string) => {
    const id = attachmentId.trim();
    const entry = entries.get(id);
    if (entry) drop(id, entry);
  };

  return { acquire, invalidate, size: () => entries.size };
}

export const attachmentImageCache = createImageCache({
  loadBytes: (id) => attachmentsStorage.readImage(id),
  createUrl: (blob) => URL.createObjectURL(blob),
  revokeUrl: (url) => URL.revokeObjectURL(url),
  releaseDelayMs: 30_000,
});
