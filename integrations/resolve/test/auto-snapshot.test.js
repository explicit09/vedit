const test = require('node:test');
const assert = require('node:assert/strict');

const { createAutoSnapshotScheduler } = require('../lib/auto-snapshot');

test('scheduler starts one 30-second interval and reports enabled', () => {
  const scheduled = [];
  const statuses = [];
  const scheduler = createAutoSnapshotScheduler({
    setIntervalFn: (callback, interval) => {
      scheduled.push({ callback, interval });
      return 42;
    },
    clearIntervalFn: () => assert.fail('clear must not run during start'),
    onTick: async () => {},
    onStatus: (status) => statuses.push(status),
  });

  scheduler.start();
  scheduler.start();

  assert.equal(scheduled.length, 1);
  assert.equal(scheduled[0].interval, 30_000);
  assert.equal(scheduler.isEnabled(), true);
  assert.deepEqual(statuses, ['enabled']);
});

test('scheduler stop clears the active interval and reports disabled', () => {
  const cleared = [];
  const statuses = [];
  const scheduler = createAutoSnapshotScheduler({
    setIntervalFn: () => 77,
    clearIntervalFn: (handle) => cleared.push(handle),
    onTick: async () => {},
    onStatus: (status) => statuses.push(status),
  });
  scheduler.start();

  scheduler.stop();
  scheduler.stop();

  assert.deepEqual(cleared, [77]);
  assert.equal(scheduler.isEnabled(), false);
  assert.deepEqual(statuses, ['enabled', 'disabled']);
});

test('scheduler never overlaps automatic snapshot ticks', async () => {
  let intervalCallback;
  let release;
  let ticks = 0;
  const scheduler = createAutoSnapshotScheduler({
    setIntervalFn: (callback) => {
      intervalCallback = callback;
      return 1;
    },
    clearIntervalFn: () => {},
    onTick: async () => {
      ticks += 1;
      if (ticks === 1) {
        await new Promise((resolve) => { release = resolve; });
      }
    },
    onStatus: () => {},
  });
  scheduler.start();

  const first = intervalCallback();
  const overlapping = intervalCallback();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(ticks, 1);
  release();
  await Promise.all([first, overlapping]);
  await intervalCallback();

  assert.equal(ticks, 2);
});
