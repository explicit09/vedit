const crypto = require('node:crypto');
const path = require('node:path');

function slugify(value) {
  const slug = String(value ?? '')
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return slug || 'untitled';
}

function shortHash(value) {
  return crypto.createHash('sha256').update(value).digest('hex').slice(0, 12);
}

function projectKey(database, projectName) {
  const identity = [
    database?.DbType ?? '',
    database?.DbName ?? '',
    database?.IpAddress ?? '',
    projectName ?? '',
  ].join('\0');
  return shortHash(identity);
}

function timelineWorkspace(root, context) {
  const projectDirectory = `${slugify(context.projectName)}--${projectKey(
    context.database,
    context.projectName,
  )}`;
  const timelineDirectory = `timeline--${shortHash(String(context.timelineId))}`;
  const directory = path.join(path.resolve(root), projectDirectory, timelineDirectory);
  const timelinePath = path.join(directory, 'timeline.otio');
  return {
    directory,
    timelinePath,
    pendingPath: `${timelinePath}.pending`,
  };
}

async function promoteExport(fsPromises, pendingPath, timelinePath) {
  let stat;
  try {
    stat = await fsPromises.stat(pendingPath);
  } catch (error) {
    if (error?.code === 'ENOENT') {
      throw new Error('Resolve did not create an OTIO file.');
    }
    throw error;
  }
  if (stat.size === 0) {
    throw new Error('Resolve created an empty OTIO file.');
  }
  await fsPromises.rename(pendingPath, timelinePath);
}

module.exports = {
  projectKey,
  promoteExport,
  slugify,
  timelineWorkspace,
};
