const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const read = (name) => fs.readFileSync(path.join(root, name), 'utf8');

test('manifest declares the stable Vedit plugin identity and entrypoint', () => {
  const manifest = read('manifest.xml');

  assert.match(manifest, /<Id>com\.explicit09\.vedit\.resolve<\/Id>/);
  assert.match(manifest, /<Name>Vedit<\/Name>/);
  assert.match(manifest, /<FilePath>main\.js<\/FilePath>/);
});

test('browser window keeps the renderer sandboxed and isolated', () => {
  const main = read('main.js');

  assert.match(main, /contextIsolation:\s*true/);
  assert.match(main, /sandbox:\s*true/);
  assert.match(main, /nodeIntegration:\s*false/);
  assert.doesNotMatch(main, /webSecurity:\s*false/);
  assert.doesNotMatch(main, /enableRemoteModule:\s*true/);
});

test('preload exposes only the three Vedit workflow methods', () => {
  const preload = read('preload.js');
  const exposedMethods = [...preload.matchAll(/^\s{2}([a-z]+):\s*\(/gm)]
    .map((match) => match[1]);

  assert.deepEqual(exposedMethods, ['inspect', 'snapshot', 'cleanup']);
  assert.doesNotMatch(preload, /send\s*:/);
  assert.doesNotMatch(preload, /invoke\s*:/);
});

test('main process registers only the matching IPC workflow channels', () => {
  const main = read('main.js');
  const channels = [...main.matchAll(/ipcMain\.handle\('([^']+)'/g)]
    .map((match) => match[1]);

  assert.deepEqual(channels, ['vedit:inspect', 'vedit:snapshot', 'vedit:cleanup']);
});

test('workspace root uses Electron’s supported videos path', () => {
  const main = read('main.js');

  assert.match(main, /app\.getPath\('videos'\)/);
  assert.doesNotMatch(main, /app\.getPath\('movies'\)/);
});

test('automatic snapshot option is reduced to one boolean across IPC', () => {
  const main = read('main.js');
  const preload = read('preload.js');

  assert.match(preload, /skipUnchanged:\s*options\.skipUnchanged\s*===\s*true/);
  assert.match(main, /skipUnchanged:\s*options\.skipUnchanged\s*===\s*true/);
});

test('main process rejects untrusted IPC senders and renderer navigation', () => {
  const source = fs.readFileSync(path.resolve(__dirname, '..', 'main.js'), 'utf8');

  assert.match(source, /senderFrame/);
  assert.match(source, /TRUSTED_RENDERER_URL/);
  assert.match(source, /will-navigate/);
  assert.match(source, /setWindowOpenHandler/);
  assert.match(source, /action:\s*'deny'/);
});
