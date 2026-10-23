// Browser regression fixture. The only backend is this synthetic in-memory note.
import React from 'react';
import { createRoot } from 'react-dom/client';
import { NoteEditor } from '../../src/features/notes/components/Editor/NoteEditor';
import { RenderedNoteContent } from '../../src/features/notes/components/Links/RenderedNoteContent';
import '../../src/styles/index.css';

const note = { id: 'audit-note', title: 'Audit', content: 'AB', format: 'md', created_at: new Date().toISOString(), modified_at: new Date().toISOString(), is_deleted: false, is_favorite: false, is_pinned: false, notebook_id: null };
const saves: string[] = [];
let delayWrites = false;
const calls: string[] = [];
Object.assign(window, { __TAURI_INTERNALS__: {
  metadata: { currentWindow: { label: 'audit' }, currentWebview: { label: 'audit' } },
  transformCallback: () => 1,
  unregisterCallback: () => {},
  invoke: async (command: string, args: any) => {
    calls.push(command);
    if (command === 'get_note') return { ...note };
    if (command === 'update_note') { if (delayWrites) await new Promise(resolve => setTimeout(resolve, 300)); Object.assign(note, args.dto); saves.push(note.content); return { ...note }; }
    if (command === 'get_attachment_count') return 1;
    if (command === 'list_notes') return [{ ...note, id: 'target', title: 'Target' }];
    if (command === 'list_notebooks') return [];
    if (command === 'resolve_link_target') return { status: 'missing', target_id: 'target' };
    if (command === 'list_attachments') return [{ id: 'image', note_id: note.id, file_name: 'audit.png', mime_type: 'image/png', file_size: 0 }];
    if (command === 'read_attachment_image') return [];
    if (command === 'get_backlinks') return [];
    if (command.startsWith('plugin:event|')) return 1;
    throw new Error('Unmocked IPC: ' + command);
  },
} });
const host = document.createElement('div'); document.body.append(host);
const root = createRoot(host);
const log: string[] = [];
const pause = (ms = 30) => new Promise(resolve => setTimeout(resolve, ms));
const assert = (condition: unknown, label: string) => { if (!condition) throw new Error(label); log.push('PASS ' + label); };
const body = () => document.querySelector('textarea')!;
const key = async (input: HTMLElement, key: string, shiftKey = false, ctrlKey = true) => {
  input.focus();
  input.dispatchEvent(new KeyboardEvent('keydown', { key, shiftKey, ctrlKey, bubbles: true, cancelable: true }));
  await pause();
};
const type = async (input: HTMLInputElement | HTMLTextAreaElement, value: string) => {
  input.focus();
  const proto = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, 'value')!.set!.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
  await pause();
};
async function run() {
  root.render(<NoteEditor noteId={note.id} startInEditMode showMinimap={false} />);
  for (let i = 0; i < 100 && body()?.value !== 'AB'; i++) await pause();
  assert(body()?.value === 'AB', 'synthetic note loaded');
  body().setSelectionRange(1, 1);
  await key(body(), 'Tab', false, false);
  assert(body().value === 'A  B', 'Tab inserts spaces');
  await pause(1000);
  assert(note.content === 'A  B', 'Tab-only autosave persists spaces');
  await key(body(), 'z');
  assert(body().value === 'AB', 'Ctrl+Z undoes Tab');
  await key(body(), 'y');
  assert(body().value === 'A  B', 'Ctrl+Y redoes Tab');
  await key(body(), 'z'); await key(body(), 'Z', true);
  assert(body().value === 'A  B', 'Ctrl+Shift+Z redoes');
  await key(body(), 'e');
  assert(!body(), 'preview opens');
  assert((document.querySelector('[aria-label="Undo"]') as HTMLButtonElement).disabled, 'preview disables undo');
  await key(document.body, 'e');
  await key(body(), 'z');
  assert(body().value === 'AB', 'undo survives preview remount');
  const redo = document.querySelector('[aria-label="Redo"]') as HTMLButtonElement;
  redo.focus(); redo.click(); await pause();
  assert(body().value === 'A  B', 'keyboard-focused toolbar uses body history');
  await type(body(), 'A\n\n\nB');
  assert(document.querySelector('.toolbar-btn-save')?.textContent?.trim() === 'Save', 'typing marks Save dirty');
  await pause(1000);
  assert(note.content === 'A\n\n\nB', 'line breaks autosaved verbatim');
  await key(body(), 'e');
  assert((document.querySelector('.md-blank-lines') as HTMLElement)?.style.height === '2lh', 'preview renders two blank lines');
  await key(document.body, 'e');
  const title = document.querySelector('input[aria-label="Note title"]') as HTMLInputElement;
  await type(title, 'Changed title'); await key(title, 'z');
  assert(title.value === 'Audit' && body().value === 'A\n\n\nB', 'title undo isolated from body');
  await key(body(), 'z');
  assert(body().value === 'A  B', 'body history remains after title undo');
  await pause(1000);
  assert(note.content === 'A  B', 'undo result autosaved');
  body().setSelectionRange(0, 0);
  await key(body(), 'k');
  for (let i = 0; i < 100 && !document.querySelector('.note-picker-item'); i++) await pause();
  (document.querySelector('.note-picker-item') as HTMLButtonElement).click(); await pause();
  assert(body().value.includes('notepad://note/target'), 'link picker inserts reference');
  await key(body(), 'z');
  assert(body().value === 'A  B', 'link insertion undo is one transaction');
  await key(body(), 'y');
  assert(body().value.includes('notepad://note/target'), 'link insertion redo');
  await key(body(), 'z');
  (document.querySelector('[data-insert-image-toggle]') as HTMLButtonElement).click(); await pause();
  (document.querySelector('[aria-label="Insert audit.png"]') as HTMLButtonElement).click(); await pause();
  assert(body().value.includes('notepad://attachment/image'), 'image panel inserts reference');
  await key(body(), 'z');
  assert(body().value === 'A  B', 'image insertion undo is one transaction');
  await key(body(), 'y');
  assert(body().value.includes('notepad://attachment/image'), 'image insertion redo');
  await key(body(), 'z');
  await key(body(), 'e');
  const previewTitle = document.querySelector('input[aria-label="Note title"]') as HTMLInputElement;
  await type(previewTitle, 'Preview title'); await key(previewTitle, 'z');
  assert(previewTitle.value === 'Audit', 'title undo also works while body is previewed');
  await key(document.body, 'e');
  body().dispatchEvent(new InputEvent('beforeinput', { inputType: 'historyRedo', bubbles: true, cancelable: true })); await pause();
  assert(body().value.includes('notepad://attachment/image'), 'native beforeinput redo uses shared history');
  await pause(1000);
  const beforeReadonly = body().value;
  root.render(<NoteEditor noteId={note.id} startInEditMode showMinimap={false} isTrash />); await pause();
  await key(body(), 'z');
  assert(body().value === beforeReadonly && (document.querySelector('[aria-label="Undo"]') as HTMLButtonElement).disabled, 'read-only note cannot undo');
  root.render(<NoteEditor noteId={note.id} startInEditMode showMinimap={false} />); await pause();
  const savedBaseline = note.content;
  delayWrites = true;
  await type(body(), savedBaseline + ' pending edit');
  await pause(850);
  await key(body(), 'z');
  root.render(<RenderedNoteContent content={'\n\n'} format="txt" />); await pause(1000);
  assert(note.content === savedBaseline, 'undo to baseline survives close during an in-flight save');

  assert(!document.querySelector('.editor-preview-empty'), 'whitespace-only note is not replaced by placeholder');
  root.render(<RenderedNoteContent content={'  A   B\n\n\nC'} format="md" />); await pause();
  assert(getComputedStyle(document.querySelector('.md-paragraph')!).whiteSpace === 'pre-wrap', 'Markdown text preserves spaces');
}
run().then(() => finish('PASS')).catch(error => finish('FAIL ' + error.stack));
function finish(status: string) {
  const out = document.createElement('pre'); out.id = 'audit-results';
  out.textContent = JSON.stringify({ status, log, saves, calls });
  document.body.append(out);
}
