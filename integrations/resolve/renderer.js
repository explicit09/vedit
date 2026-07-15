const { createInitialView, reduceView } = window.VeditViewState;
const { createAutoSnapshotScheduler } = window.VeditAutoSnapshot;

const AUTO_SNAPSHOT_PREFERENCE = 'vedit.autoSnapshots.enabled';

const elements = {
  body: document.body,
  signal: document.getElementById('signal'),
  statusLabel: document.getElementById('status-label'),
  timelineName: document.getElementById('timeline-name'),
  projectName: document.getElementById('project-name'),
  snapshotButton: document.getElementById('snapshot-button'),
  snapshotLabel: document.getElementById('snapshot-label'),
  autoSnapshotToggle: document.getElementById('auto-snapshot-toggle'),
  autoSnapshotStatus: document.getElementById('auto-snapshot-status'),
  errorBanner: document.getElementById('error-banner'),
  errorMessage: document.getElementById('error-message'),
  errorDetailsWrap: document.getElementById('error-details-wrap'),
  errorDetails: document.getElementById('error-details'),
  retryButton: document.getElementById('retry-button'),
  latestChange: document.getElementById('latest-change'),
  latestMessage: document.getElementById('latest-message'),
  latestHash: document.getElementById('latest-hash'),
  latestAuthor: document.getElementById('latest-author'),
  latestDate: document.getElementById('latest-date'),
  changeList: document.getElementById('change-list'),
  emptyState: document.getElementById('empty-state'),
  historyCount: document.getElementById('history-count'),
  historyList: document.getElementById('history-list'),
};

let view = createInitialView();
let autoSnapshotNote = '';

function text(element, value) {
  element.textContent = value ?? '';
}

function clear(element) {
  while (element.firstChild) element.removeChild(element.firstChild);
}

function toggle(element, visible) {
  element.classList.toggle('is-hidden', !visible);
}

function statusCopy(phase) {
  return {
    boot: 'Connecting',
    loading: 'Connecting',
    saving: 'Saving snapshot',
    ready: 'Ready',
    empty: 'Ready',
    error: 'Attention',
  }[phase] || 'Connecting';
}

function readableDate(value) {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(date);
}

function renderChanges(changes) {
  clear(elements.changeList);
  for (const change of changes || []) {
    const item = document.createElement('li');
    text(item, change);
    elements.changeList.appendChild(item);
  }
}

function renderHistory(history) {
  clear(elements.historyList);
  for (const commit of history || []) {
    const item = document.createElement('li');
    item.dataset.current = String(commit.current === true);
    const hash = document.createElement('code');
    const message = document.createElement('p');
    const headDot = document.createElement('span');
    headDot.className = 'head-dot';
    headDot.setAttribute('aria-label', commit.current ? 'Current snapshot' : '');
    text(hash, commit.hash);
    text(message, commit.message);
    item.append(hash, message, headDot);
    elements.historyList.appendChild(item);
  }
  const count = history?.length || 0;
  text(elements.historyCount, `${count} ${count === 1 ? 'snapshot' : 'snapshots'}`);
}

function render() {
  elements.body.dataset.phase = view.phase;
  text(elements.statusLabel, statusCopy(view.phase));
  const hasContext = Boolean(view.context);
  text(elements.timelineName, hasContext ? view.context.timelineName : 'Waiting for Resolve…');
  text(elements.projectName, hasContext ? view.context.projectName : 'No project detected yet');
  elements.snapshotButton.disabled = view.busy || !hasContext;
  text(elements.snapshotLabel, view.phase === 'saving' ? 'Saving…' : 'Snapshot timeline');

  toggle(elements.errorBanner, Boolean(view.error));
  text(elements.errorMessage, view.error?.message);
  const hasDetails = Boolean(view.error?.details);
  toggle(elements.errorDetailsWrap, hasDetails);
  text(elements.errorDetails, view.error?.details);

  const hasLatest = Boolean(view.latest);
  toggle(elements.latestChange, hasLatest);
  toggle(elements.emptyState, !hasLatest);
  if (hasLatest) {
    text(elements.latestMessage, view.latest.message);
    text(elements.latestHash, view.latest.hash);
    text(elements.latestAuthor, view.latest.author);
    text(elements.latestDate, readableDate(view.latest.date));
    renderChanges(view.latest.changes);
  }
  renderHistory(view.history);
}

function renderAutoSnapshot(status) {
  const enabled = status !== 'disabled';
  elements.autoSnapshotToggle.setAttribute('aria-pressed', String(enabled));
  const copy = {
    disabled: 'Off · checks every 30s',
    enabled: 'On · next check within 30s',
    checking: 'Checking active timeline…',
    unchanged: 'On · timeline is up to date',
    saved: 'On · snapshot saved',
    error: 'On · last check needs attention',
  }[status];
  text(elements.autoSnapshotStatus, autoSnapshotNote || copy || 'Off · checks every 30s');
  autoSnapshotNote = '';
}

async function inspect() {
  view = reduceView(view, { type: 'inspect-started' });
  render();
  try {
    const payload = await window.vedit.inspect();
    view = reduceView(view, { type: 'operation-finished', payload });
  } catch (error) {
    view = reduceView(view, { type: 'operation-rejected', error });
  }
  render();
}

async function runSnapshot(options = {}) {
  view = reduceView(view, { type: 'snapshot-started' });
  render();
  try {
    const payload = await window.vedit.snapshot(options);
    view = reduceView(view, { type: 'operation-finished', payload });
    return payload;
  } catch (error) {
    view = reduceView(view, { type: 'operation-rejected', error });
    throw error;
  } finally {
    render();
  }
}

async function snapshot() {
  try {
    await runSnapshot({ skipUnchanged: false });
  } catch (_error) {
    // The shared view renders the retryable error from runSnapshot.
  }
}

const autoScheduler = createAutoSnapshotScheduler({
  onStatus: renderAutoSnapshot,
  async onTick() {
    renderAutoSnapshot('checking');
    try {
      const payload = await runSnapshot({ skipUnchanged: true });
      if (!autoScheduler.isEnabled()) return;
      if (payload.status === 'error') renderAutoSnapshot('error');
      else renderAutoSnapshot(payload.unchanged ? 'unchanged' : 'saved');
    } catch (_error) {
      if (autoScheduler.isEnabled()) renderAutoSnapshot('error');
    }
  },
});

function setAutoSnapshotEnabled(enabled, persist = true) {
  if (enabled) autoScheduler.start();
  else autoScheduler.stop();
  renderAutoSnapshot(enabled ? 'enabled' : 'disabled');
  if (persist) localStorage.setItem(AUTO_SNAPSHOT_PREFERENCE, String(enabled));
}

function toggleAutoSnapshot() {
  setAutoSnapshotEnabled(!autoScheduler.isEnabled());
}

async function boot() {
  await inspect();
  setAutoSnapshotEnabled(localStorage.getItem(AUTO_SNAPSHOT_PREFERENCE) === 'true', false);
}

elements.snapshotButton.addEventListener('click', snapshot);
elements.autoSnapshotToggle.addEventListener('click', toggleAutoSnapshot);
elements.retryButton.addEventListener('click', inspect);
window.addEventListener('beforeunload', () => {
  autoScheduler.stop();
  window.vedit.cleanup();
});
window.addEventListener('DOMContentLoaded', boot);
