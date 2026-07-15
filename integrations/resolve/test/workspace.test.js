const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');

const {
  projectKey,
  promoteExport,
  slugify,
  timelineWorkspace,
} = require('../lib/workspace');

const context = (overrides = {}) => ({
  database: { DbType: 'Disk', DbName: 'Local' },
  projectName: 'Tech Show',
  timelineName: 'Cut 1',
  timelineId: 'timeline-42',
  ...overrides,
});

test('slugify produces a readable filesystem segment', () => {
  assert.equal(slugify('  Café / Final CUT  '), 'cafe-final-cut');
  assert.equal(slugify('***'), 'untitled');
});

test('projectKey changes when the Resolve database changes', () => {
  const disk = projectKey({ DbType: 'Disk', DbName: 'Local' }, 'Show');
  const cloud = projectKey({ DbType: 'PostgreSQL', DbName: 'Cloud' }, 'Show');

  assert.match(disk, /^[a-f0-9]{12}$/);
  assert.notEqual(disk, cloud);
});

test('timelineWorkspace keeps the same directory across timeline renames', () => {
  const original = timelineWorkspace('/tmp/Vedit', context());
  const renamed = timelineWorkspace(
    '/tmp/Vedit',
    context({ timelineName: 'Final Cut' }),
  );

  assert.equal(original.directory, renamed.directory);
  assert.equal(original.timelinePath, path.join(original.directory, 'timeline.otio'));
  assert.equal(original.pendingPath, path.join(original.directory, 'timeline.pending.otio'));
});

test('promoteExport atomically replaces the current timeline', async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'vedit-workspace-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const pendingPath = path.join(directory, 'timeline.otio.pending');
  const timelinePath = path.join(directory, 'timeline.otio');
  await fs.writeFile(pendingPath, '{"new":true}');
  await fs.writeFile(timelinePath, '{"old":true}');

  await promoteExport(fs, pendingPath, timelinePath);

  assert.equal(await fs.readFile(timelinePath, 'utf8'), '{"new":true}');
  await assert.rejects(fs.stat(pendingPath), { code: 'ENOENT' });
});

test('promoteExport refuses a missing or empty export', async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'vedit-workspace-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const pendingPath = path.join(directory, 'timeline.otio.pending');
  const timelinePath = path.join(directory, 'timeline.otio');

  await assert.rejects(
    promoteExport(fs, pendingPath, timelinePath),
    /did not create an OTIO file/i,
  );
  await fs.writeFile(pendingPath, '');
  await assert.rejects(
    promoteExport(fs, pendingPath, timelinePath),
    /created an empty OTIO file/i,
  );
});
