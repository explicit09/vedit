const { app, BrowserWindow, ipcMain } = require('electron');
const fs = require('node:fs/promises');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

const WorkflowIntegration = require('./WorkflowIntegration.node');
const { createResolveAdapter } = require('./lib/resolve-adapter');
const { createSnapshotController } = require('./lib/snapshot-controller');
const { createVeditRunner } = require('./lib/vedit-runner');
const workspaceService = require('./lib/workspace');

const PLUGIN_ID = 'com.explicit09.vedit.resolve';
const TRUSTED_RENDERER_URL = pathToFileURL(path.join(__dirname, 'index.html')).toString();

let initializePromise = null;
let resolvePromise = null;
let cleanedUp = false;

async function getResolve() {
  if (!initializePromise) {
    initializePromise = WorkflowIntegration.InitializePromise(PLUGIN_ID);
  }
  const initialized = await initializePromise;
  if (!initialized) return null;
  if (!resolvePromise) {
    resolvePromise = WorkflowIntegration.GetResolvePromise();
  }
  return resolvePromise;
}

function cleanupResolveInterface() {
  if (cleanedUp) return true;
  cleanedUp = true;
  resolvePromise = null;
  initializePromise = null;
  return WorkflowIntegration.CleanUp();
}

function createController() {
  const binaryPath = path.join(__dirname, 'bin', 'vedit');
  return createSnapshotController({
    fsPromises: fs,
    resolveAdapter: createResolveAdapter({ getResolve }),
    veditRunner: createVeditRunner({ binaryPath }),
    workspaceRoot: path.join(app.getPath('videos'), 'Vedit'),
    workspaceService,
  });
}

function registerHandlers(controller) {
  function assertTrustedSender(event) {
    if (!event.senderFrame || event.senderFrame.url !== TRUSTED_RENDERER_URL) {
      throw new Error('Vedit rejected an IPC request from an untrusted renderer.');
    }
  }
  ipcMain.handle('vedit:inspect', (event) => {
    assertTrustedSender(event);
    return controller.inspect();
  });
  ipcMain.handle('vedit:snapshot', (event, options = {}) => {
    assertTrustedSender(event);
    return controller.snapshot({
    skipUnchanged: options.skipUnchanged === true,
    });
  });
  ipcMain.handle('vedit:cleanup', (event) => {
    assertTrustedSender(event);
    return cleanupResolveInterface();
  });
}

function createWindow() {
  const window = new BrowserWindow({
    width: 720,
    height: 760,
    minWidth: 620,
    minHeight: 640,
    useContentSize: true,
    title: 'Vedit',
    backgroundColor: '#101216',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
    },
  });
  window.setMenu(null);
  window.webContents.on('will-navigate', (event) => event.preventDefault());
  window.webContents.on('will-redirect', (event) => event.preventDefault());
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  window.loadFile(path.join(__dirname, 'index.html'));
  return window;
}

app.whenReady().then(() => {
  registerHandlers(createController());
  createWindow();
});

app.on('before-quit', cleanupResolveInterface);
app.on('window-all-closed', () => app.quit());
