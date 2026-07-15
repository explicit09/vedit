#!/usr/bin/env node

const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

function fail(message) {
  process.stderr.write(`Vedit validation failed: ${message}\n`);
  process.exit(1);
}

const pluginDirectory = path.resolve(process.argv[2] || path.join(
  '/Library/Application Support/Blackmagic Design/DaVinci Resolve',
  'Workflow Integration Plugins',
  'com.explicit09.vedit.resolve',
));

const requiredFiles = [
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
];

for (const relativePath of requiredFiles) {
  const absolutePath = path.join(pluginDirectory, relativePath);
  if (!fs.existsSync(absolutePath) || !fs.statSync(absolutePath).isFile()) {
    fail(`${relativePath} is missing from ${pluginDirectory}`);
  }
}

const manifest = fs.readFileSync(path.join(pluginDirectory, 'manifest.xml'), 'utf8');
if (!manifest.includes('<Id>com.explicit09.vedit.resolve</Id>')) {
  fail('manifest.xml has the wrong plugin identity');
}

const sidecarPath = path.join(pluginDirectory, 'bin', 'vedit');
if ((fs.statSync(sidecarPath).mode & 0o111) === 0) {
  fail('bin/vedit is not executable');
}

try {
  execFileSync(sidecarPath, ['--version'], { stdio: 'pipe' });
} catch (error) {
  fail(`bin/vedit could not run: ${error.message}`);
}

if (process.env.VEDIT_SKIP_ARCH_CHECK !== '1') {
  if (process.arch !== 'arm64') {
    fail(`V1 requires Apple silicon; Node reported ${process.arch}`);
  }
  for (const relativePath of ['bin/vedit', 'WorkflowIntegration.node']) {
    const output = execFileSync('/usr/bin/file', [
      path.join(pluginDirectory, relativePath),
    ], { encoding: 'utf8' });
    if (!output.includes('arm64')) {
      fail(`${relativePath} does not contain an arm64 binary`);
    }
  }
}

process.stdout.write(`Resolve integration validated at ${pluginDirectory}\n`);
