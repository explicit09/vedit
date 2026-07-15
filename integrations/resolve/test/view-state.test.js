const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const { createInitialView, reduceView } = require('../lib/view-state');

const readyPayload = {
  status: 'ready',
  context: {
    projectName: 'Show <script>alert(1)</script>',
    timelineName: 'Fine & Final',
  },
  history: [{ hash: 'abc1234', message: 'Trimmed <intro>', current: true }],
  latest: {
    hash: 'abc1234',
    message: 'Trimmed <intro>',
    author: 'Editor <editor@example.com>',
    date: '2026-07-14T12:00:00Z',
    changes: ['Removed <unsafe-looking-name>'],
  },
  error: null,
};

test('initial view enters loading without inventing project data', () => {
  const initial = createInitialView();
  const loading = reduceView(initial, { type: 'inspect-started' });

  assert.equal(loading.phase, 'loading');
  assert.equal(loading.busy, true);
  assert.equal(loading.context, null);
});

test('inspect result becomes a ready review view', () => {
  const state = reduceView(createInitialView(), {
    type: 'operation-finished',
    payload: readyPayload,
  });

  assert.equal(state.phase, 'ready');
  assert.equal(state.busy, false);
  assert.equal(state.context.projectName, 'Show <script>alert(1)</script>');
  assert.equal(state.latest.changes[0], 'Removed <unsafe-looking-name>');
});

test('snapshot start preserves the prior review while marking it busy', () => {
  const ready = reduceView(createInitialView(), {
    type: 'operation-finished',
    payload: readyPayload,
  });
  const saving = reduceView(ready, { type: 'snapshot-started' });

  assert.equal(saving.phase, 'saving');
  assert.equal(saving.busy, true);
  assert.deepEqual(saving.history, readyPayload.history);
  assert.deepEqual(saving.latest, readyPayload.latest);
});

test('empty and error results keep explicit phases and retry data', () => {
  const empty = reduceView(createInitialView(), {
    type: 'operation-finished',
    payload: {
      status: 'empty',
      context: { projectName: 'Show', timelineName: 'First Cut' },
      history: [],
      latest: null,
      error: null,
    },
  });
  const failed = reduceView(empty, {
    type: 'operation-finished',
    payload: {
      ...empty,
      status: 'error',
      error: { code: 'EXPORT_FAILED', message: 'Snapshot failed', details: 'false' },
    },
  });

  assert.equal(empty.phase, 'empty');
  assert.equal(failed.phase, 'error');
  assert.deepEqual(failed.error, {
    code: 'EXPORT_FAILED',
    message: 'Snapshot failed',
    details: 'false',
  });
});

test('unexpected IPC rejection becomes a retryable renderer error', () => {
  const state = reduceView(createInitialView(), {
    type: 'operation-rejected',
    error: new Error('IPC channel closed'),
  });

  assert.equal(state.phase, 'error');
  assert.equal(state.busy, false);
  assert.deepEqual(state.error, {
    code: 'PLUGIN_ERROR',
    message: 'Vedit could not communicate with Resolve.',
    details: 'IPC channel closed',
  });
});

test('renderer treats runtime strings as text instead of HTML', () => {
  const renderer = fs.readFileSync(
    path.resolve(__dirname, '..', 'renderer.js'),
    'utf8',
  );

  assert.match(renderer, /textContent/);
  assert.doesNotMatch(renderer, /\.innerHTML\s*=/);
  assert.doesNotMatch(renderer, /insertAdjacentHTML/);
});
