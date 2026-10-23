/** Isolated browser test: no Tauri backend, installed app, or real notes. */
import { build } from 'vite';
import { mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { spawn } from 'node:child_process';

const workspace = await mkdtemp(path.join(tmpdir(), 'notepad-editor-test-'));
const output = path.join(workspace, 'web');
await build({
  configFile: false,
  define: { 'process.env.NODE_ENV': '"production"' },
  build: { outDir: output, emptyOutDir: true, lib: {
    entry: 'tests/editor-browser/entry.tsx', name: 'EditorAudit', formats: ['iife'], fileName: () => 'audit.js',
  } },
});
const files = await readdir(output);
const css = files.filter(file => file.endsWith('.css')).map(file => `<link rel="stylesheet" href="/${file}">`).join('');
await writeFile(path.join(output, 'index.html'), `<html><head><meta charset="utf-8">${css}</head><body><script src="/audit.js"></script></body></html>`);
const server = http.createServer(async (request, response) => {
  const file = request.url === '/' ? 'index.html' : request.url?.slice(1);
  if (!file || !['index.html', ...files].includes(file)) { response.writeHead(404).end(); return; }
  try {
    const type = file.endsWith('.js') ? 'text/javascript' : file.endsWith('.css') ? 'text/css' : 'text/html';
    response.writeHead(200, { 'Content-Type': type });
    response.end(await readFile(path.join(output, file)));
  } catch { response.writeHead(500).end(); }
});
await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
try {
  const browser = spawn(process.env.EDITOR_TEST_BROWSER || 'chromium', [
    '--headless', '--no-sandbox', '--disable-gpu', `--user-data-dir=${path.join(workspace, 'profile')}`,
    '--virtual-time-budget=15000', '--dump-dom', `http://127.0.0.1:${server.address().port}/`,
  ]);
  let dom = '', stderr = '';
  browser.stdout.on('data', chunk => { dom += chunk; });
  browser.stderr.on('data', chunk => { stderr += chunk; });
  const timeout = setTimeout(() => browser.kill('SIGKILL'), 60000);
  try {
    await new Promise((resolve, reject) => { browser.once('error', reject); browser.once('close', resolve); });
  } finally { clearTimeout(timeout); }
  await writeFile(path.join(workspace, 'dom.html'), dom);
  await writeFile(path.join(workspace, 'browser.log'), stderr);
  const match = /<pre id="audit-results">([\s\S]*?)<\/pre>/.exec(dom);
  if (!match) throw new Error(`Browser did not complete the fixture. Logs: ${workspace}`);
  const result = JSON.parse(match[1].replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&amp;', '&'));
  console.log(JSON.stringify(result, null, 2));
  console.log(`Isolated test artifacts: ${workspace}`);
  if (result.status !== 'PASS') process.exitCode = 1;
} finally {
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
}
