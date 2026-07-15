const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const fsPromises = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const integrationRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(integrationRoot, '..', '..');
const installer = path.join(integrationRoot, 'scripts', 'install-macos.sh');

test('installer creates a minimal validated Resolve plugin', async (t) => {
  const temporary = await fsPromises.mkdtemp(path.join(os.tmpdir(), 'vedit-install-'));
  t.after(() => fsPromises.rm(temporary, { recursive: true, force: true }));
  const pluginRoot = path.join(temporary, 'plugins');
  const sdkModule = path.join(temporary, 'WorkflowIntegration.node');
  await fsPromises.writeFile(sdkModule, 'test-native-module');

  const result = spawnSync(installer, [], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: {
      ...process.env,
      VEDIT_REPO_ROOT: repoRoot,
      VEDIT_RESOLVE_PLUGIN_ROOT: pluginRoot,
      VEDIT_RESOLVE_SDK_PLUGIN: sdkModule,
      VEDIT_SIDECAR_BIN: '/usr/bin/true',
      VEDIT_SKIP_ARCH_CHECK: '1',
    },
  });

  assert.equal(
    result.status,
    0,
    result.stderr || result.stdout || result.error?.message || 'installer failed',
  );
  const installed = path.join(pluginRoot, 'com.explicit09.vedit.resolve');
  for (const file of [
    'manifest.xml',
    'package.json',
    'main.js',
    'preload.js',
    'index.html',
    'styles.css',
    'renderer.js',
    'WorkflowIntegration.node',
    'bin/vedit',
    'lib/workspace.js',
    'lib/resolve-adapter.js',
    'lib/vedit-runner.js',
    'lib/snapshot-controller.js',
    'lib/view-state.js',
  ]) {
    assert.equal(fs.existsSync(path.join(installed, file)), true, `${file} missing`);
  }
  assert.equal(fs.existsSync(path.join(installed, 'test')), false);
  assert.equal(fs.existsSync(path.join(installed, 'scripts')), false);
  assert.notEqual((await fsPromises.stat(path.join(installed, 'bin', 'vedit'))).mode & 0o111, 0);
  assert.match(result.stdout, /Resolve integration validated/);
});

test('installer fails clearly when the Resolve SDK bridge is missing', async (t) => {
  const temporary = await fsPromises.mkdtemp(path.join(os.tmpdir(), 'vedit-install-'));
  t.after(() => fsPromises.rm(temporary, { recursive: true, force: true }));

  const result = spawnSync(installer, [], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: {
      ...process.env,
      VEDIT_REPO_ROOT: repoRoot,
      VEDIT_RESOLVE_PLUGIN_ROOT: path.join(temporary, 'plugins'),
      VEDIT_RESOLVE_SDK_PLUGIN: path.join(temporary, 'missing.node'),
      VEDIT_SIDECAR_BIN: '/usr/bin/true',
      VEDIT_SKIP_ARCH_CHECK: '1',
    },
  });

  assert.notEqual(result.status, 0);
  assert.match(
    String(result.stderr || result.error?.message),
    /WorkflowIntegration\.node was not found/,
  );
});
