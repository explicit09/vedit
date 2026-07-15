const test = require('node:test');
const assert = require('node:assert/strict');

const { createResolveAdapter } = require('../lib/resolve-adapter');

function resolveFixture({ project = true, timeline = true, exportResult = true } = {}) {
  const calls = [];
  const timelineObject = timeline ? {
    GetName: async () => 'Episode 12 — Fine Cut',
    GetUniqueId: async () => 'timeline-unique-12',
    Export: async (filePath, exportType) => {
      calls.push({ filePath, exportType });
      return exportResult;
    },
  } : null;
  const projectObject = project ? {
    GetName: async () => 'TechNolgia Talks',
    GetCurrentTimeline: async () => timelineObject,
  } : null;
  const projectManager = {
    GetCurrentDatabase: async () => ({
      DbType: 'Disk',
      DbName: 'Local Database',
    }),
    GetCurrentProject: async () => projectObject,
  };
  const resolve = {
    EXPORT_OTIO: 13,
    GetProjectManager: async () => projectManager,
  };
  return { calls, resolve };
}

test('getActiveContext returns complete Resolve identity', async () => {
  const fixture = resolveFixture();
  const adapter = createResolveAdapter({ getResolve: async () => fixture.resolve });

  const context = await adapter.getActiveContext();

  assert.deepEqual(context, {
    database: { DbType: 'Disk', DbName: 'Local Database' },
    projectName: 'TechNolgia Talks',
    timelineName: 'Episode 12 — Fine Cut',
    timelineId: 'timeline-unique-12',
  });
});

test('exportActiveTimeline uses Resolve OTIO export and returns context', async () => {
  const fixture = resolveFixture();
  const adapter = createResolveAdapter({ getResolve: async () => fixture.resolve });

  const context = await adapter.exportActiveTimeline('/tmp/timeline.otio.pending');

  assert.equal(context.timelineId, 'timeline-unique-12');
  assert.deepEqual(fixture.calls, [
    { filePath: '/tmp/timeline.otio.pending', exportType: 13 },
  ]);
});

test('missing active project is an actionable typed error', async () => {
  const fixture = resolveFixture({ project: false });
  const adapter = createResolveAdapter({ getResolve: async () => fixture.resolve });

  await assert.rejects(adapter.getActiveContext(), (error) => {
    assert.equal(error.code, 'NO_PROJECT');
    assert.equal(error.message, 'Open a Resolve project to use Vedit.');
    return true;
  });
});

test('missing active timeline is an actionable typed error', async () => {
  const fixture = resolveFixture({ timeline: false });
  const adapter = createResolveAdapter({ getResolve: async () => fixture.resolve });

  await assert.rejects(adapter.getActiveContext(), (error) => {
    assert.equal(error.code, 'NO_TIMELINE');
    assert.equal(error.message, 'Open a timeline in Resolve to use Vedit.');
    return true;
  });
});

test('failed Resolve export never reports success', async () => {
  const fixture = resolveFixture({ exportResult: false });
  const adapter = createResolveAdapter({ getResolve: async () => fixture.resolve });

  await assert.rejects(
    adapter.exportActiveTimeline('/tmp/timeline.otio.pending'),
    (error) => {
      assert.equal(error.code, 'EXPORT_FAILED');
      assert.equal(error.message, 'Resolve could not prepare this timeline snapshot.');
      return true;
    },
  );
});

test('unavailable Resolve connection is an actionable typed error', async () => {
  const adapter = createResolveAdapter({ getResolve: async () => null });

  await assert.rejects(adapter.getActiveContext(), (error) => {
    assert.equal(error.code, 'RESOLVE_UNAVAILABLE');
    assert.equal(error.message, 'Vedit lost its connection to Resolve.');
    return true;
  });
});
