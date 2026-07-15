(function exposeViewState(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) root.VeditViewState = api;
}(typeof window === 'object' ? window : null, () => {
  function createInitialView() {
    return {
      phase: 'boot',
      busy: false,
      context: null,
      history: [],
      latest: null,
      error: null,
    };
  }

  function reduceView(state, event) {
    switch (event.type) {
      case 'inspect-started':
        return { ...state, phase: 'loading', busy: true, error: null };
      case 'snapshot-started':
        return { ...state, phase: 'saving', busy: true, error: null };
      case 'operation-finished':
        return {
          ...state,
          phase: event.payload.status,
          busy: false,
          context: event.payload.context,
          history: event.payload.history,
          latest: event.payload.latest,
          error: event.payload.error,
        };
      case 'operation-rejected':
        return {
          ...state,
          phase: 'error',
          busy: false,
          error: {
            code: 'PLUGIN_ERROR',
            message: 'Vedit could not communicate with Resolve.',
            details: event.error?.message || String(event.error),
          },
        };
      default:
        return state;
    }
  }

  return { createInitialView, reduceView };
}));
