const childProcess = require('node:child_process');
const fs = require('node:fs/promises');
const path = require('node:path');

const MAX_BUFFER = 2 * 1024 * 1024;

function parseLog(stdout) {
  return String(stdout)
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter(Boolean)
    .flatMap((line) => {
      const match = line.match(/^(\S+)\s{2}(.+)$/);
      if (!match) return [];
      const [, hash, rawSummary] = match;
      const current = /\s{2}\([^)]*HEAD[^)]*\)$/.test(rawSummary);
      const withoutRefs = rawSummary.replace(/\s{2}\([^)]*HEAD[^)]*\)$/, '');
      const authorIndex = withoutRefs.lastIndexOf('  by ');
      const message = (authorIndex === -1
        ? withoutRefs
        : withoutRefs.slice(0, authorIndex)).trim();
      return [{ hash, message, current }];
    });
}

function parseShow(stdout) {
  const lines = String(stdout).replace(/\r/g, '').split('\n');
  const hash = lines.find((line) => line.startsWith('commit '))?.slice(7).trim() ?? '';
  const author = lines.find((line) => line.startsWith('Author:'))?.slice(7).trim() ?? '';
  const date = lines.find((line) => line.startsWith('Date:'))?.slice(5).trim() ?? '';
  const firstBlank = lines.findIndex((line) => line === '');
  let cursor = firstBlank + 1;
  const messageLines = [];
  while (cursor > 0 && cursor < lines.length && lines[cursor].startsWith('    ')) {
    messageLines.push(lines[cursor].slice(4));
    cursor += 1;
  }
  while (cursor < lines.length && lines[cursor] === '') cursor += 1;
  return {
    hash,
    author,
    date,
    message: messageLines.join('\n'),
    changes: lines.slice(cursor).map((line) => line.trim()).filter(Boolean),
  };
}

function createVeditRunner({
  binaryPath,
  execFile = childProcess.execFile,
  fsPromises = fs,
}) {
  function run(args, cwd, failureMessage) {
    return new Promise((resolve, reject) => {
      execFile(binaryPath, args, {
        cwd,
        encoding: 'utf8',
        maxBuffer: MAX_BUFFER,
        shell: false,
      }, (error, stdout, stderr) => {
        if (!error) {
          resolve(String(stdout).trim());
          return;
        }
        const wrapped = new Error(failureMessage);
        wrapped.details = String(stderr || error.stderr || error.message).trim();
        wrapped.exitCode = error.code;
        wrapped.cause = error;
        reject(wrapped);
      });
    });
  }

  async function repositoryExists(directory) {
    try {
      await fsPromises.access(path.join(directory, '.vedit'));
      return true;
    } catch (error) {
      if (error?.code === 'ENOENT') return false;
      throw error;
    }
  }

  async function fileExists(filePath) {
    try {
      await fsPromises.access(filePath);
      return true;
    } catch (error) {
      if (error?.code === 'ENOENT') return false;
      throw error;
    }
  }

  async function hasSemanticChanges(workspace, candidatePath) {
    if (!await repositoryExists(workspace.directory)
      || !await fileExists(workspace.timelinePath)) {
      return true;
    }
    const before = path.relative(workspace.directory, workspace.timelinePath);
    const after = path.relative(workspace.directory, candidatePath);
    const output = await run(
      ['diff', before, after, '--json'],
      workspace.directory,
      'Vedit could not compare the automatic snapshot.',
    );
    const changes = JSON.parse(output);
    return Array.isArray(changes) && changes.length > 0;
  }

  async function load(workspace) {
    if (!await repositoryExists(workspace.directory)) {
      return { history: [], detail: null };
    }
    const historyOutput = await run(
      ['log'],
      workspace.directory,
      'Vedit could not read this timeline history.',
    );
    const history = parseLog(historyOutput);
    if (history.length === 0) return { history, detail: null };
    const showOutput = await run(
      ['show', 'HEAD'],
      workspace.directory,
      'Vedit could not read the latest timeline change.',
    );
    return { history, detail: parseShow(showOutput) };
  }

  async function snapshot(workspace) {
    if (!await repositoryExists(workspace.directory)) {
      await run(
        ['init'],
        workspace.directory,
        'Vedit could not initialize timeline history.',
      );
    }
    const timelineArgument = path.relative(workspace.directory, workspace.timelinePath);
    const commitLine = await run(
      ['commit', timelineArgument],
      workspace.directory,
      'Vedit could not snapshot this timeline.',
    );
    return { commitLine, ...await load(workspace) };
  }

  return { hasSemanticChanges, load, snapshot };
}

module.exports = {
  createVeditRunner,
  parseLog,
  parseShow,
};
