# Disk budgets: launcher screenshots

These screenshots come from this branch's own launcher page (Troubleshooting → Overview), rendered in headless Chromium at 1920×1080 by `capture.mjs`. Only the backend is replaced:

- the launcher's test fixtures answer its commands;
- `storage_status` and `storage_cleanup` answer with the fields of `omsi-launcher-core`'s `storage::Status` and its real limits (1 GB local, 4 GB multiplayer);
- the sizes shown are chosen for the pictures, not measured on a disk.

To capture them again:

```
CHROME=<chrome> node docs/screenshots/storage-budgets/capture.mjs
```

## Pull request block

```markdown
### Disk budgets in the launcher

> Captured from this branch's launcher UI with stand-in storage answers (`docs/screenshots/storage-budgets/capture.mjs`): the limits are the real ones, the sizes are examples.

**1. Usage against both soft budgets**
![Storage usage](https://github.com/isaacsa2/openOMSI/blob/fix/cache-budgets-and-safe-cleanup/docs/screenshots/storage-budgets/01-storage-usage.png?raw=true)
Troubleshooting shows the rebuildable local cache against its 1 GB budget and multiplayer downloads against their separate 4 GB budget.

**2. A game is running: nothing is cleaned**
![Game running, over the soft limit](https://github.com/isaacsa2/openOMSI/blob/fix/cache-budgets-and-safe-cleanup/docs/screenshots/storage-budgets/02-game-running-over-soft-limit.png?raw=true)
The multiplayer downloads may go over 4 GB while a game uses them. The cleanup actions are disabled until every launcher-managed game is closed.

**3. Clearing asks first**
![Clear local cache confirmation](https://github.com/isaacsa2/openOMSI/blob/fix/cache-budgets-and-safe-cleanup/docs/screenshots/storage-budgets/03-clear-local-cache-confirmation.png?raw=true)
*Clear local cache…* and *Clear multiplayer downloads…* each ask for confirmation. Installed mods, saves and settings are never removed.

**4. Clean to limits**
![Clean to limits result](https://github.com/isaacsa2/openOMSI/blob/fix/cache-budgets-and-safe-cleanup/docs/screenshots/storage-budgets/04-clean-to-limits-result.png?raw=true)
Older eligible files are removed until each store is back under its budget, and the freed space is shown.

**5. Server vehicle files stay**
![Server store protected](https://github.com/isaacsa2/openOMSI/blob/fix/cache-budgets-and-safe-cleanup/docs/screenshots/storage-budgets/05-server-store-protected.png?raw=true)
On a computer that holds a server's vehicle manifests, those files follow the server's retention policy. *Clear multiplayer downloads* is disabled there.
```

## Not captured

- **Automatic cleanup:** it runs in the background on start and after a session; there is nothing on screen for it.
- **A real disk:** sizes over 4 GB and the grace period for young or in-progress files need a real LAN session.
