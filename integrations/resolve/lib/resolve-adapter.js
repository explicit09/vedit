function typedError(code, message) {
  const error = new Error(message);
  error.code = code;
  return error;
}

function createResolveAdapter({ getResolve, exportType }) {
  async function readActive() {
    const resolve = await getResolve();
    if (!resolve) {
      throw typedError('RESOLVE_UNAVAILABLE', 'Vedit lost its connection to Resolve.');
    }
    const projectManager = await resolve.GetProjectManager();
    if (!projectManager) {
      throw typedError('RESOLVE_UNAVAILABLE', 'Vedit lost its connection to Resolve.');
    }
    const [database, project] = await Promise.all([
      projectManager.GetCurrentDatabase(),
      projectManager.GetCurrentProject(),
    ]);
    if (!project) {
      throw typedError('NO_PROJECT', 'Open a Resolve project to use Vedit.');
    }
    const timeline = await project.GetCurrentTimeline();
    if (!timeline) {
      throw typedError('NO_TIMELINE', 'Open a timeline in Resolve to use Vedit.');
    }
    const [projectName, timelineName, timelineId] = await Promise.all([
      project.GetName(),
      timeline.GetName(),
      timeline.GetUniqueId(),
    ]);
    if (!timelineId) {
      throw typedError(
        'TIMELINE_ID_UNAVAILABLE',
        'Resolve did not provide an identity for this timeline.',
      );
    }
    return {
      resolve,
      timeline,
      context: { database, projectName, timelineName, timelineId },
    };
  }

  async function getActiveContext() {
    const { context } = await readActive();
    return context;
  }

  async function exportActiveTimeline(filePath) {
    const { resolve, timeline, context } = await readActive();
    const result = await timeline.Export(
      filePath,
      exportType ?? resolve.EXPORT_OTIO,
    );
    if (result !== true) {
      throw typedError(
        'EXPORT_FAILED',
        'Resolve could not prepare this timeline snapshot.',
      );
    }
    return context;
  }

  return { exportActiveTimeline, getActiveContext };
}

module.exports = { createResolveAdapter };
