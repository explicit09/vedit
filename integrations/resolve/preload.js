const { contextBridge, ipcRenderer } = require('electron/renderer');

contextBridge.exposeInMainWorld('vedit', {
  inspect: () => ipcRenderer.invoke('vedit:inspect'),
  snapshot: (options = {}) => ipcRenderer.invoke('vedit:snapshot', {
    skipUnchanged: options.skipUnchanged === true,
  }),
  cleanup: () => ipcRenderer.invoke('vedit:cleanup'),
});
