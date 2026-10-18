/** Ids of external documents (files opened from outside the notebook) start with this prefix. */
export const EXTERNAL_ID_PREFIX = "ext-";

export function isExternalId(id: string | null | undefined): boolean {
  return typeof id === "string" && id.startsWith(EXTERNAL_ID_PREFIX);
}
