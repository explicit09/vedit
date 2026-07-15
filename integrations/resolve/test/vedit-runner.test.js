const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');

const {
  createVeditRunner,
  parseLog,
  parseShow,
} = require('../lib/vedit-runner');

function successfulExecutor(outputs, calls) {
  return (binary, args, options, callback) => {
    calls.push({ binary, args, options });
    const key = args.join(' ');
    const stdout = outputs[key];
    if (stdout === undefined) {
      callback(new Error(`Unexpected command: ${key}`), '', '');
      return;
    }
    if (key === 'init') {
      fs.mkdir(path.join(options.cwd, '.vedit'))
        .then(() => callback(null, stdout, ''), callback);
      return;
    }
    callback(null, stdout, '');
  };
}

test('parseLog returns editor-facing history entries', () => {
  const entries = parseLog([
    'a1b2c3d  Trimmed intro by 1.2s  by Editor <editor@example.com>  (HEAD -> main)',
    'f6e5d4c  Initial commit: 2 track(s), 4 clip(s)  by Editor <editor@example.com>',
  ].join('\n'));

  assert.deepEqual(entries, [
    { hash: 'a1b2c3d', message: 'Trimmed intro by 1.2s', current: true },
    { hash: 'f6e5d4c', message: 'Initial commit: 2 track(s), 4 clip(s)', current: false },
  ]);
});

test('parseShow returns commit metadata and semantic changes', () => {
  const detail = parseShow([
    'commit a1b2c3d',
    'Author: Editor <editor@example.com>',
    'Date:   2026-07-14T12:00:00Z',
    '',
    '    Trimmed intro by 1.2s',
    '',
    'Trimmed "intro" by 1.20s (out)',
    'Added crossfade (12 frames)',
  ].join('\n'));

  assert.deepEqual(detail, {
    hash: 'a1b2c3d',
    author: 'Editor <editor@example.com>',
    date: '2026-07-14T12:00:00Z',
    message: 'Trimmed intro by 1.2s',
    changes: ['Trimmed "intro" by 1.20s (out)', 'Added crossfade (12 frames)'],
  });
});

test('first snapshot initializes and commits before loading review data', async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'vedit-runner-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const timelinePath = path.join(directory, 'timeline.otio');
  await fs.writeFile(timelinePath, '{}');
  const calls = [];
  const execFile = successfulExecutor({
    init: 'Initialized empty vedit repository',
    'commit timeline.otio': '[main (root) a1b2c3d] Initial commit: 1 track(s), 1 clip(s)',
    log: 'a1b2c3d  Initial commit: 1 track(s), 1 clip(s)  by Editor <editor@example.com>  (HEAD -> main)',
    'show HEAD': [
      'commit a1b2c3d',
      'Author: Editor <editor@example.com>',
      'Date:   2026-07-14T12:00:00Z',
      '',
      '    Initial commit: 1 track(s), 1 clip(s)',
      '',
      'Initial commit. 1 track(s), 1 clip(s).',
    ].join('\n'),
  }, calls);
  const runner = createVeditRunner({ binaryPath: '/plugin/bin/vedit', execFile, fsPromises: fs });

  const result = await runner.snapshot({ directory, timelinePath });

  assert.deepEqual(calls.map((call) => call.args), [
    ['init'],
    ['commit', 'timeline.otio'],
    ['log'],
    ['show', 'HEAD'],
  ]);
  assert.equal(calls.every((call) => call.options.cwd === directory), true);
  assert.equal(calls.every((call) => call.options.shell === false), true);
  assert.equal(result.commitLine.startsWith('[main (root) a1b2c3d]'), true);
  assert.equal(result.history[0].hash, 'a1b2c3d');
  assert.deepEqual(result.detail.changes, ['Initial commit. 1 track(s), 1 clip(s).']);
});

test('snapshot skips init when the repository already exists', async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'vedit-runner-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  await fs.mkdir(path.join(directory, '.vedit'));
  const timelinePath = path.join(directory, 'timeline.otio');
  await fs.writeFile(timelinePath, '{}');
  const calls = [];
  const execFile = successfulExecutor({
    'commit timeline.otio': '[main a1b2c3d] No semantic changes',
    log: 'a1b2c3d  No semantic changes  by Editor <editor@example.com>  (HEAD -> main)',
    'show HEAD': 'commit a1b2c3d\nAuthor: Editor <editor@example.com>\nDate:   now\n\n    No semantic changes\n\n(no semantic changes from parent)',
  }, calls);
  const runner = createVeditRunner({ binaryPath: '/plugin/bin/vedit', execFile, fsPromises: fs });

  await runner.snapshot({ directory, timelinePath });

  assert.equal(calls.some((call) => call.args[0] === 'init'), false);
});

test('command failures retain diagnostic stderr without changing the message', async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'vedit-runner-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  await fs.mkdir(path.join(directory, '.vedit'));
  const timelinePath = path.join(directory, 'timeline.otio');
  await fs.writeFile(timelinePath, '{}');
  const execFile = (_binary, args, _options, callback) => {
    const error = new Error('Command failed');
    error.code = 2;
    callback(error, '', args[0] === 'commit' ? 'invalid OTIO near byte 42' : '');
  };
  const runner = createVeditRunner({ binaryPath: '/plugin/bin/vedit', execFile, fsPromises: fs });

  await assert.rejects(
    runner.snapshot({ directory, timelinePath }),
    (error) => {
      assert.equal(error.message, 'Vedit could not snapshot this timeline.');
      assert.equal(error.details, 'invalid OTIO near byte 42');
      assert.equal(error.exitCode, 2);
      return true;
    },
  );
});

test('semantic comparison skips an unchanged pending export', async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'vedit-runner-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  await fs.mkdir(path.join(directory, '.vedit'));
  const timelinePath = path.join(directory, 'timeline.otio');
  const candidatePath = path.join(directory, 'timeline.pending.otio');
  await fs.writeFile(timelinePath, '{}');
  await fs.writeFile(candidatePath, '{}');
  const calls = [];
  const execFile = successfulExecutor({
    'diff timeline.otio timeline.pending.otio --json': '[]',
  }, calls);
  const runner = createVeditRunner({ binaryPath: '/plugin/bin/vedit', execFile, fsPromises: fs });

  const changed = await runner.hasSemanticChanges(
    { directory, timelinePath },
    candidatePath,
  );

  assert.equal(changed, false);
  assert.deepEqual(calls.map((call) => call.args), [
    ['diff', 'timeline.otio', 'timeline.pending.otio', '--json'],
  ]);
});

test('semantic comparison detects changed pending export', async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'vedit-runner-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  await fs.mkdir(path.join(directory, '.vedit'));
  const timelinePath = path.join(directory, 'timeline.otio');
  const candidatePath = path.join(directory, 'timeline.pending.otio');
  await fs.writeFile(timelinePath, '{}');
  await fs.writeFile(candidatePath, '{}');
  const execFile = successfulExecutor({
    'diff timeline.otio timeline.pending.otio --json': '[{"op":"trimmed"}]',
  }, []);
  const runner = createVeditRunner({ binaryPath: '/plugin/bin/vedit', execFile, fsPromises: fs });

  assert.equal(await runner.hasSemanticChanges(
    { directory, timelinePath },
    candidatePath,
  ), true);
});

test('first pending export is always treated as changed', async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'vedit-runner-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const timelinePath = path.join(directory, 'timeline.otio');
  const candidatePath = path.join(directory, 'timeline.pending.otio');
  await fs.writeFile(candidatePath, '{}');
  const execFile = () => assert.fail('diff must not run before the first snapshot');
  const runner = createVeditRunner({ binaryPath: '/plugin/bin/vedit', execFile, fsPromises: fs });

  assert.equal(await runner.hasSemanticChanges(
    { directory, timelinePath },
    candidatePath,
  ), true);
});
