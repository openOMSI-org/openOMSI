# Server fleet: launcher screenshots

These screenshots come from this branch's own launcher page, rendered in headless Chromium at 1920×1080 by `capture.mjs`. Only the backend is replaced:

- the launcher's test fixtures (`launcher/tests/fixtures`) answer its commands;
- `server_query` answers like a fleet v1 server called `fleet.example` (not a real address);
- `Vehicles/Example_Fleet/S21.bus` and its `Server …` paint names are test data.

No live VPS, download or game session is shown.

To capture them again:

```
CHROME=<chrome> node docs/screenshots/server-fleet/capture.mjs
```

## Pull request block

```markdown
### Server fleet in the launcher

> Captured from this branch's launcher UI with offline fixture data (`docs/screenshots/server-fleet/capture.mjs`): no live server, download or game session.

**1. The server's bus catalogue**
![Server bus catalogue](https://github.com/isaacsa2/openOMSI/blob/test/server-fleet-auto-sync/docs/screenshots/server-fleet/01-server-bus-catalogue.png?raw=true)
After joining a fleet v1 server, the picker lists only the buses the server publishes. That includes **S21 (Example Fleet)**, which is not installed locally. A locally installed bus keeps its 3D preview and is tagged **Server**.

**2. Paints published by the server**
![Server paint selection](https://github.com/isaacsa2/openOMSI/blob/test/server-fleet-auto-sync/docs/screenshots/server-fleet/02-server-paint-selection.png?raw=true)
The paint list is the server's: the bus's standard look plus each repaint the server announces, each listed once. The server also announces *Beige*, the bus's standard look, which is folded into *Standard (Beige)* instead of being listed twice.

**3. Server-only bus without a preview**
![Server-only bus without preview](https://github.com/isaacsa2/openOMSI/blob/test/server-fleet-auto-sync/docs/screenshots/server-fleet/03-server-only-bus-no-preview.png?raw=true)
A bus whose files are not installed gets no 3D preview. A placeholder says the preview becomes available after the verified download, and the bus can still be selected.

**4. Consent before any download**
![Download consent](https://github.com/isaacsa2/openOMSI/blob/test/server-fleet-auto-sync/docs/screenshots/server-fleet/04-download-consent.png?raw=true)
*Join and drive* with a server fleet bus first asks for explicit approval. Matching local files are reused; other files are verified and cached for this session without replacing installed mods. The transfer progress is shown by the game during the join and is not captured here.
```

## Not captured

- **Download progress:** it is drawn in the game while joining ("Downloading another player's bus: …%"), not in the launcher. It needs a running server and game.
- **The bus after its download:** the verified files are cached for the session and do not appear as an installed mod in the launcher, so the launcher has no separate state to show.
- **Multiplayer in game:** this needs OMSI content, a live server and a second player.
