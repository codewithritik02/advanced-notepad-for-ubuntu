import { test } from 'node:test';
import assert from 'node:assert/strict';
import { EditHistory, historyShortcut } from '../src/features/notes/utils/editHistory.ts';
import { parseMarkdown, splitAttachmentImages } from '../src/utils/markdown.ts';

test('production history replays typing, selection replacement, Tab, links and images', () => {
  const h = new EditHistory();
  const values = ['hello', 'hello world', 'hello  world', 'hello [link](notepad://note/test) world', 'hello ![image](notepad://attachment/test) world', 'नमस्ते 👋'];
  h.reset(values[0]);
  for (const value of values.slice(1)) h.record(value);
  for (const value of values.slice(0, -1).reverse()) assert.equal(h.apply('undo').value, value);
  assert.equal(h.apply('undo'), null);
  for (const value of values.slice(1)) assert.equal(h.apply('redo').value, value);
  assert.equal(h.apply('redo'), null);
  h.apply('undo'); h.record('different branch');
  assert.equal(h.canRedo, false);
  h.reset('another note');
  assert.equal(h.canUndo, false);
});

test('field histories are isolated and bounded, large paste remains reversible', () => {
  const body = new EditHistory(), title = new EditHistory();
  body.reset('body'); title.reset('title');
  title.record('new title'); body.record('new body');
  assert.equal(title.apply('undo').value, 'title');
  assert.equal(body.apply('undo').value, 'body');
  body.record('x'.repeat(4_000_001));
  assert.equal(body.apply('undo').value, 'body');
  body.reset('');
  for (let i = 1; i <= 300; i++) body.record(String(i));
  let count = 0;
  while (body.apply('undo')) count++;
  assert.equal(count, 200);
});

test('production shortcut resolver supports Ctrl/Cmd, alternate redo, and composition guards', () => {
  const key = { key: 'z', ctrlKey: true, metaKey: false, altKey: false, shiftKey: false };
  assert.equal(historyShortcut(key), 'undo');
  assert.equal(historyShortcut({ ...key, key: 'Y' }), 'redo');
  assert.equal(historyShortcut({ ...key, shiftKey: true }), 'redo');
  assert.equal(historyShortcut({ ...key, ctrlKey: false, metaKey: true }), 'undo');
  for (const extra of [{ altKey: true }, { isComposing: true }, { ctrlKey: false }, { key: 's' }]) {
    assert.equal(historyShortcut({ ...key, ...extra }), null);
  }
});

test('preview preserves blank line counts, indentation and code whitespace', () => {
  for (const lines of [1, 2, 5]) {
    assert.deepEqual(parseMarkdown('A' + '\n'.repeat(lines + 1) + 'B')[1], { t: 'blank', lines });
  }
  const boundary = parseMarkdown('\n\nA\n\n');
  assert.deepEqual(boundary[0], { t: 'blank', lines: 2 });
  assert.deepEqual(boundary.at(-1), { t: 'blank', lines: 2 });
  assert.deepEqual(parseMarkdown('  A   B')[0].c, [{ t: 'text', v: '  A   B' }]);
  assert.equal(parseMarkdown('```\nA\n\n\nB\n```')[0].v, 'A\n\n\nB');
  assert.deepEqual(parseMarkdown('> A\n>\n>\n> B')[0].c[1], { t: 'blank', lines: 2 });
  const original = 'A\n\n\n![image](notepad://attachment/test)\n\n\nB';
  assert.deepEqual(splitAttachmentImages(original).filter(p => p.type === 'text').map(p => p.text), ['A\n\n\n', '\n\n\nB']);
});
