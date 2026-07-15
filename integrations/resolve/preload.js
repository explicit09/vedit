const { contextBridge, ipcRenderer } = require('electron/renderer');

contextBridge.exposeInMainWorld('vedit', {
  inspect: () => ipcRenderer.invoke('vedit:inspect'),
  snapshot: () => ipcRenderer.invoke('vedit:snapshot'),
  cleanup: () => ipcRenderer.invoke('vedit:cleanup'),
});
