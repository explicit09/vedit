(function exposeAutoSnapshot(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) root.VeditAutoSnapshot = api;
}(typeof window === 'object' ? window : null, () => {
  function createAutoSnapshotScheduler({
    intervalMs = 30_000,
    setIntervalFn = setInterval,
    clearIntervalFn = clearInterval,
    onTick,
    onStatus,
  }) {
    let handle = null;
    let running = false;

    async function tick() {
      if (handle === null || running) return;
      running = true;
      try {
        await onTick();
      } finally {
        running = false;
      }
    }

    function start() {
      if (handle !== null) return;
      handle = setIntervalFn(tick, intervalMs);
      onStatus('enabled');
    }

    function stop() {
      if (handle === null) return;
      clearIntervalFn(handle);
      handle = null;
      onStatus('disabled');
    }

    function isEnabled() {
      return handle !== null;
    }

    return { isEnabled, start, stop };
  }

  return { createAutoSnapshotScheduler };
}));
