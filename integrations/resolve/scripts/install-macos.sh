#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INTEGRATION_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="${VEDIT_REPO_ROOT:-$(cd "$INTEGRATION_ROOT/../.." && pwd)}"
PLUGIN_ROOT="${VEDIT_RESOLVE_PLUGIN_ROOT:-/Library/Application Support/Blackmagic Design/DaVinci Resolve/Workflow Integration Plugins}"
SDK_PLUGIN="${VEDIT_RESOLVE_SDK_PLUGIN:-/Library/Application Support/Blackmagic Design/DaVinci Resolve/Developer/Workflow Integrations/Examples/SamplePromisePlugin/WorkflowIntegration.node}"
TARGET="$PLUGIN_ROOT/com.explicit09.vedit.resolve"

if [[ ! -f "$SDK_PLUGIN" ]]; then
  echo "Vedit install failed: WorkflowIntegration.node was not found at $SDK_PLUGIN" >&2
  echo "Install DaVinci Resolve Studio 20.1 or newer, then run this installer again." >&2
  exit 1
fi

if [[ -n "${VEDIT_SIDECAR_BIN:-}" ]]; then
  SIDECAR="$VEDIT_SIDECAR_BIN"
else
  cargo build --release -p vedit-cli --manifest-path "$REPO_ROOT/Cargo.toml"
  SIDECAR="$REPO_ROOT/target/release/vedit"
fi

if [[ ! -x "$SIDECAR" ]]; then
  echo "Vedit install failed: sidecar is missing or not executable at $SIDECAR" >&2
  exit 1
fi

mkdir -p "$PLUGIN_ROOT"
STAGING="$(mktemp -d "$PLUGIN_ROOT/.vedit-install.XXXXXX")"
BACKUP="$TARGET.previous"

cleanup() {
  if [[ -d "$STAGING" ]]; then rm -rf "$STAGING"; fi
}
trap cleanup EXIT

mkdir -p "$STAGING/bin" "$STAGING/lib"
for file in manifest.xml package.json main.js preload.js index.html styles.css renderer.js; do
  cp "$INTEGRATION_ROOT/$file" "$STAGING/$file"
done
for file in workspace.js resolve-adapter.js vedit-runner.js snapshot-controller.js auto-snapshot.js view-state.js; do
  cp "$INTEGRATION_ROOT/lib/$file" "$STAGING/lib/$file"
done
cp "$SDK_PLUGIN" "$STAGING/WorkflowIntegration.node"
cp "$SIDECAR" "$STAGING/bin/vedit"
chmod 755 "$STAGING/bin/vedit"

if [[ -e "$BACKUP" ]]; then rm -rf "$BACKUP"; fi
if [[ -e "$TARGET" ]]; then mv "$TARGET" "$BACKUP"; fi
if ! mv "$STAGING" "$TARGET"; then
  if [[ -e "$BACKUP" ]]; then mv "$BACKUP" "$TARGET"; fi
  exit 1
fi
if ! node "$SCRIPT_DIR/validate-install.js" "$TARGET"; then
  rm -rf "$TARGET"
  if [[ -e "$BACKUP" ]]; then mv "$BACKUP" "$TARGET"; fi
  exit 1
fi
if [[ -e "$BACKUP" ]]; then rm -rf "$BACKUP"; fi
echo "Restart DaVinci Resolve, then open Workspace -> Workflow Integrations -> Vedit."
