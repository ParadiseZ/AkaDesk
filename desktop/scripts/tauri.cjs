const { spawnSync } = require('node:child_process');
const path = require('node:path');
const desktopRoot = path.resolve(__dirname, '..');
const cli = path.join(desktopRoot, 'ui/node_modules/@tauri-apps/cli/tauri.js');
const result = spawnSync(process.execPath, [cli, ...process.argv.slice(2)], {
  cwd: desktopRoot,
  stdio: 'inherit',
});
if (result.error) { console.error(result.error.message); }
process.exit(result.status ?? 1);
