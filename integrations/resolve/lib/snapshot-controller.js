function createSnapshotController({
  fsPromises,
  resolveAdapter,
  veditRunner,
  workspaceRoot,
  workspaceService,
}) {
  let inFlight = null;
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

  async function executeSnapshot() {
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

  function snapshot() {
    if (!inFlight) {
      inFlight = executeSnapshot().finally(() => {
        inFlight = null;
      });
    }
    return inFlight;
  }

  return { inspect, snapshot };
}

module.exports = { createSnapshotController };
