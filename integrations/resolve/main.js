const { app, BrowserWindow, ipcMain } = require('electron');
const fs = require('node:fs/promises');
const path = require('node:path');

const WorkflowIntegration = require('./WorkflowIntegration.node');
const { createResolveAdapter } = require('./lib/resolve-adapter');
const { createSnapshotController } = require('./lib/snapshot-controller');
const { createVeditRunner } = require('./lib/vedit-runner');
const workspaceService = require('./lib/workspace');

const PLUGIN_ID = 'com.explicit09.vedit.resolve';

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
  ipcMain.handle('vedit:inspect', () => controller.inspect());
  ipcMain.handle('vedit:snapshot', () => controller.snapshot());
  ipcMain.handle('vedit:cleanup', () => cleanupResolveInterface());
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
  window.loadFile('index.html');
  return window;
}

app.whenReady().then(() => {
  registerHandlers(createController());
  createWindow();
});

app.on('before-quit', cleanupResolveInterface);
app.on('window-all-closed', () => app.quit());
