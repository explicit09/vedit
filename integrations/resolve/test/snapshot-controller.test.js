const test = require('node:test');
const assert = require('node:assert/strict');

const { createSnapshotController } = require('../lib/snapshot-controller');

const activeContext = {
  database: { DbType: 'Disk', DbName: 'Local' },
  projectName: 'Show',
  timelineName: 'Fine Cut',
  timelineId: 'timeline-42',
};

const workspace = {
  directory: '/managed/show/timeline-42',
  timelinePath: '/managed/show/timeline-42/timeline.otio',
  pendingPath: '/managed/show/timeline-42/timeline.otio.pending',
};

function dependencies(overrides = {}) {
  const events = [];
  const resolveAdapter = {
    getActiveContext: async () => {
      events.push('context');
      return activeContext;
    },
    exportActiveTimeline: async (target) => {
      events.push(`export:${target}`);
      return activeContext;
    },
  };
  const fsPromises = {
    mkdir: async (directory, options) => events.push(`mkdir:${directory}:${options.recursive}`),
    rm: async (target, options) => events.push(`rm:${target}:${options.force}`),
  };
  const workspaceService = {
    timelineWorkspace: (_root, context) => {
      events.push(`workspace:${context.timelineId}`);
      return workspace;
    },
    promoteExport: async (_fs, pending, timeline) => {
      events.push(`promote:${pending}:${timeline}`);
    },
  };
  const review = {
    history: [{ hash: 'a1b2c3d', message: 'Trimmed intro', current: true }],
    detail: {
      hash: 'a1b2c3d',
      message: 'Trimmed intro',
      author: 'Editor <editor@example.com>',
      date: '2026-07-14T12:00:00Z',
      changes: ['Trimmed "intro" by 1.20s (out)'],
    },
  };
  const veditRunner = {
    load: async () => {
      events.push('load');
      return { history: [], detail: null };
    },
    snapshot: async () => {
      events.push('commit');
      return { commitLine: '[main a1b2c3d] Trimmed intro', ...review };
    },
  };
  return {
    events,
    values: {
      fsPromises,
      resolveAdapter,
      veditRunner,
      workspaceRoot: '/managed',
      workspaceService,
      ...overrides,
    },
  };
}

test('snapshot runs export, promotion, commit, and review in order', async () => {
  const fixture = dependencies();
  const controller = createSnapshotController(fixture.values);

  const state = await controller.snapshot();

  assert.deepEqual(fixture.events, [
    'context',
    'workspace:timeline-42',
    'mkdir:/managed/show/timeline-42:true',
    'rm:/managed/show/timeline-42/timeline.otio.pending:true',
    'export:/managed/show/timeline-42/timeline.otio.pending',
    'promote:/managed/show/timeline-42/timeline.otio.pending:/managed/show/timeline-42/timeline.otio',
    'commit',
  ]);
  assert.deepEqual(state, {
    status: 'ready',
    context: activeContext,
    history: [{ hash: 'a1b2c3d', message: 'Trimmed intro', current: true }],
    latest: {
      hash: 'a1b2c3d',
      message: 'Trimmed intro',
      author: 'Editor <editor@example.com>',
      date: '2026-07-14T12:00:00Z',
      changes: ['Trimmed "intro" by 1.20s (out)'],
    },
    error: null,
  });
  assert.equal(JSON.stringify(state).includes('/managed'), false);
});

test('concurrent snapshots share one export and commit', async () => {
  let releaseExport;
  const fixture = dependencies({
    resolveAdapter: {
      getActiveContext: async () => activeContext,
      exportActiveTimeline: () => new Promise((resolve) => {
        fixture.events.push('export');
        releaseExport = () => resolve(activeContext);
      }),
    },
  });
  const controller = createSnapshotController(fixture.values);

  const first = controller.snapshot();
  const second = controller.snapshot();
  await new Promise((resolve) => setImmediate(resolve));
  releaseExport();
  const [a, b] = await Promise.all([first, second]);

  assert.deepEqual(a, b);
  assert.equal(fixture.events.filter((event) => event === 'export').length, 1);
  assert.equal(fixture.events.filter((event) => event === 'commit').length, 1);
});

test('inspect returns a clean empty state for an uninitialized timeline', async () => {
  const fixture = dependencies();
  const controller = createSnapshotController(fixture.values);

  const state = await controller.inspect();

  assert.equal(state.status, 'empty');
  assert.deepEqual(state.history, []);
  assert.equal(state.latest, null);
  assert.equal(state.error, null);
});

test('snapshot turns operational errors into renderer-safe state', async () => {
  const failure = new Error('Resolve could not prepare this timeline snapshot.');
  failure.code = 'EXPORT_FAILED';
  failure.details = 'Timeline.Export returned false';
  const fixture = dependencies({
    resolveAdapter: {
      getActiveContext: async () => activeContext,
      exportActiveTimeline: async () => { throw failure; },
    },
  });
  const controller = createSnapshotController(fixture.values);

  const state = await controller.snapshot();

  assert.equal(state.status, 'error');
  assert.deepEqual(state.error, {
    code: 'EXPORT_FAILED',
    message: 'Resolve could not prepare this timeline snapshot.',
    details: 'Timeline.Export returned false',
  });
  assert.equal(Object.hasOwn(state.error, 'stack'), false);
});

test('snapshot aborts if the active timeline changes during export', async () => {
  const fixture = dependencies({
    resolveAdapter: {
      getActiveContext: async () => activeContext,
      exportActiveTimeline: async () => ({
        ...activeContext,
        timelineId: 'a-different-timeline',
      }),
    },
  });
  const controller = createSnapshotController(fixture.values);

  const state = await controller.snapshot();

  assert.equal(state.status, 'error');
  assert.equal(state.error.code, 'ACTIVE_TIMELINE_CHANGED');
  assert.equal(fixture.events.includes('commit'), false);
  assert.equal(fixture.events.some((event) => event.startsWith('promote:')), false);
});

test('automatic snapshot skips promotion and commit when semantics are unchanged', async () => {
  const fixture = dependencies({
    veditRunner: {
      load: async () => ({ history: [], detail: null }),
      snapshot: async () => assert.fail('unchanged timeline must not commit'),
      hasSemanticChanges: async (_workspace, candidatePath) => {
        fixture.events.push(`compare:${candidatePath}`);
        return false;
      },
    },
  });
  const controller = createSnapshotController(fixture.values);

  const state = await controller.snapshot({ skipUnchanged: true });

  assert.equal(state.status, 'empty');
  assert.equal(state.unchanged, true);
  assert.equal(state.context.timelineId, 'timeline-42');
  assert.equal(fixture.events.includes('commit'), false);
  assert.equal(fixture.events.some((event) => event.startsWith('promote:')), false);
  assert.equal(
    fixture.events.includes(`compare:${workspace.pendingPath}`),
    true,
  );
});

test('manual snapshot queued during an automatic check still creates its commit', async () => {
  let releaseComparison;
  let comparisons = 0;
  const fixture = dependencies({
    veditRunner: {
      load: async () => ({ history: [], detail: null }),
      hasSemanticChanges: async () => {
        comparisons += 1;
        return new Promise((resolve) => { releaseComparison = () => resolve(false); });
      },
      snapshot: async () => {
        fixture.events.push('commit');
        return { history: [{ hash: 'manual', message: 'Manual snapshot' }], detail: null };
      },
    },
  });
  const controller = createSnapshotController(fixture.values);

  const automatic = controller.snapshot({ skipUnchanged: true });
  await new Promise((resolve) => setImmediate(resolve));
  const manual = controller.snapshot({ skipUnchanged: false });
  releaseComparison();
  const [automaticState, manualState] = await Promise.all([automatic, manual]);

  assert.equal(automaticState.unchanged, true);
  assert.equal(manualState.unchanged, undefined);
  assert.equal(comparisons, 1);
  assert.equal(fixture.events.filter((event) => event === 'commit').length, 1);
  assert.equal(fixture.events.filter((event) => event.startsWith('export:')).length, 2);
});

test('unchanged automatic check loads history for the active workspace', async () => {
  const secondContext = { ...activeContext, timelineId: 'timeline-99', timelineName: 'Trailer' };
  let currentContext = activeContext;
  const fixture = dependencies({
    resolveAdapter: {
      getActiveContext: async () => currentContext,
      exportActiveTimeline: async () => currentContext,
    },
    workspaceService: {
      timelineWorkspace: (_root, context) => ({
        directory: `/managed/${context.timelineId}`,
        timelinePath: `/managed/${context.timelineId}/timeline.otio`,
        pendingPath: `/managed/${context.timelineId}/timeline.otio.pending`,
      }),
      promoteExport: async () => {},
    },
    veditRunner: {
      load: async (activeWorkspace) => ({
        history: [{ hash: activeWorkspace.directory.endsWith('99') ? 'trailer' : 'fine-cut' }],
        detail: null,
      }),
      hasSemanticChanges: async () => false,
      snapshot: async () => assert.fail('unchanged timeline must not commit'),
    },
  });
  const controller = createSnapshotController(fixture.values);
  await controller.inspect();
  currentContext = secondContext;

  const state = await controller.snapshot({ skipUnchanged: true });

  assert.equal(state.context.timelineId, 'timeline-99');
  assert.deepEqual(state.history, [{ hash: 'trailer' }]);
});

test('a failed later snapshot preserves the last successful review', async () => {
  const fixture = dependencies();
  const controller = createSnapshotController(fixture.values);
  const ready = await controller.snapshot();
  fixture.values.resolveAdapter.exportActiveTimeline = async () => {
    throw new Error('Temporary export failure');
  };

  const failed = await controller.snapshot();

  assert.equal(ready.history.length, 1);
  assert.deepEqual(failed.history, ready.history);
  assert.deepEqual(failed.latest, ready.latest);
  assert.equal(failed.status, 'error');
});
