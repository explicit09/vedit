function createSnapshotController({
  fsPromises,
  resolveAdapter,
  veditRunner,
  workspaceRoot,
  workspaceService,
}) {
  let inFlight = null;
  let inFlightSkipsUnchanged = false;
  let queuedManual = null;
  let lastState = {
    status: 'empty',
    context: null,
    history: [],
    latest: null,
    error: null,
  };

  function successState(context, review) {
    return {
      status: review.history.length === 0 ? 'empty' : 'ready',
      context,
      history: review.history,
      latest: review.detail,
      error: null,
    };
  }

  function errorState(error, context = lastState.context) {
    const safeError = {
      code: error.code || 'UNEXPECTED',
      message: error.message || 'Vedit could not complete this operation.',
    };
    if (error.details) safeError.details = String(error.details);
    return {
      ...lastState,
      status: 'error',
      context,
      error: safeError,
    };
  }

  async function inspect() {
    let context = null;
    try {
      context = await resolveAdapter.getActiveContext();
      const workspace = workspaceService.timelineWorkspace(workspaceRoot, context);
      const review = await veditRunner.load(workspace);
      lastState = successState(context, review);
      return lastState;
    } catch (error) {
      return errorState(error, context);
    }
  }

  async function executeSnapshot({ skipUnchanged = false } = {}) {
    let context = null;
    let workspace = null;
    try {
      context = await resolveAdapter.getActiveContext();
      workspace = workspaceService.timelineWorkspace(workspaceRoot, context);
      await fsPromises.mkdir(workspace.directory, { recursive: true });
      await fsPromises.rm(workspace.pendingPath, { force: true });
      const exportedContext = await resolveAdapter.exportActiveTimeline(
        workspace.pendingPath,
      );
      if (exportedContext.timelineId !== context.timelineId) {
        const error = new Error(
          'The active timeline changed while Vedit was preparing the snapshot.',
        );
        error.code = 'ACTIVE_TIMELINE_CHANGED';
        throw error;
      }
      if (skipUnchanged
        && !await veditRunner.hasSemanticChanges(workspace, workspace.pendingPath)) {
        await fsPromises.rm(workspace.pendingPath, { force: true });
        const review = await veditRunner.load(workspace);
        lastState = successState(context, review);
        return { ...lastState, unchanged: true };
      }
      await workspaceService.promoteExport(
        fsPromises,
        workspace.pendingPath,
        workspace.timelinePath,
      );
      const review = await veditRunner.snapshot(workspace);
      lastState = successState(context, review);
      return lastState;
    } catch (error) {
      if (workspace) {
        await fsPromises.rm(workspace.pendingPath, { force: true }).catch(() => {});
      }
      return errorState(error, context);
    }
  }

  function beginSnapshot(skipUnchanged) {
    const operation = executeSnapshot({ skipUnchanged });
    inFlight = operation;
    inFlightSkipsUnchanged = skipUnchanged;
    const clear = () => {
      if (inFlight === operation) {
        inFlight = null;
        inFlightSkipsUnchanged = false;
      }
    };
    operation.then(clear, clear);
    return operation;
  }

  function snapshot(options = {}) {
    const skipUnchanged = options.skipUnchanged === true;
    if (!inFlight) return beginSnapshot(skipUnchanged);
    if (!skipUnchanged && inFlightSkipsUnchanged) {
      if (!queuedManual) {
        const automatic = inFlight;
        queuedManual = automatic
          .then(
            () => beginSnapshot(false),
            () => beginSnapshot(false),
          )
          .finally(() => { queuedManual = null; });
      }
      return queuedManual;
    }
    return inFlight;
  }

  return { inspect, snapshot };
}

module.exports = { createSnapshotController };
