# Interface themes

An interface theme changes the look of the game's own screens. This page is for players who want to switch themes and for modders who want to make one.

A theme can change:

- the colours of the pause menu, its settings windows and lists, photo mode, the developer tools, plugin panels and notifications;
- the corners of the panels, and whether they are frosted glass over the game's picture;
- the logo at the top of the pause menu;
- the loading screen, whose logo can itself be the progress bar.

The HUD, the navigator and the cockpit keep their own look. The navigator follows its light or dark map.

## Choose a theme

1. In the game, open **Esc → Options → Interface**.
2. Pick a theme under **Theme pack**. openOMSI's own look is the first entry.
3. The theme applies from the loading screen of the next start.

The choice is saved as `ui_theme_pack` in `settings.cfg`. If a theme cannot be read, openOMSI keeps its own look and `game.log` says why.

A theme is laid over openOMSI's light or dark look, whichever you chose under **Interface theme** (`ui_theme`). A theme that sets all its surfaces and text colours looks the same in both. A theme with only a few colours (an accent alone) is the light or dark look with its accent.

## Make a theme

1. Create a folder `Themes/<id>/` in a content folder. The folder name is the theme's id: letters, digits, `-`, `_` and `.`, at most 40 characters. `openomsi` is reserved.
2. Put a `theme.json` in it (example below).
3. Add any logo files to the same folder.
4. Start the game and choose the theme in **Esc → Options → Interface → Theme pack**.

```json
{
  "name": "Night shift",
  "title": "Night shift",
  "colors": {
    "accent": "#2a75f7",
    "background": "#0a142e",
    "surface": "#141a26",
    "ink": "#e8ebf2"
  },
  "radius": 12,
  "glass": { "opacity": 0.55, "blur": 26, "edge": 0.14, "highlight": 0.09 },
  "loading": {
    "background": "#0a142e",
    "text": "#c4cad6",
    "logo": "logo.svg",
    "progress": "stroke"
  },
  "logo": "icon.svg"
}
```

Everything is optional. What a theme leaves out stays openOMSI's (light or dark, as chosen).

### Where themes are found

| Folder | Theme |
|---|---|
| `Themes/<id>/` in a content folder | A theme of its own (a mod) |
| `plugins/<name>/` | A plugin's theme; its id is the plugin's name |

- The content folders' `Themes` are searched first.
- If two themes have the same id (in any letter case), the first one found is used.
- A content folder another player's game sent in a LAN session brings no theme.

## theme.json reference

| Key | What it sets |
|---|---|
| `name` | The theme's name in the options, at most 40 characters. Without it: the id. |
| `title` | The name at the top of the pause menu, beside `logo`, above openOMSI's version. At most 40 characters. |
| `colors` | Colours by name (see [Colours](#colours)). |
| `radius` | Panel corners, 0 to 24 points. openOMSI's own is 8. |
| `glass` | Frosted glass panels: `true` for the defaults, or an object (see [Glass](#glass)). |
| `loading` | The loading screen: `background`, `text`, `logo` and `progress`. |
| `logo` | The logo at the top of the pause menu (see [Logos](#logos)). |

`author`, `version` and `description` are accepted and ignored. Any other key is logged and left out. A wrong value (an unknown colour format, a radius out of range, a missing file) stops the theme from loading, with the reason in `game.log`. `theme.json` may be at most 4 MB.

### Colours

Write a colour as `"#rrggbb"`, `"#rrggbbaa"`, `[r, g, b]` or `[r, g, b, a]` (0 to 255; opacity `a` 0 to 1).

| Group | Names |
|---|---|
| Accent | `accent`, `accent_hover`, `accent_ink` (text on the accent), `accent_2` |
| Surfaces | `background`, `sidebar`, `surface`, `surface_2`, `surface_3` (each a step lighter) |
| Text | `ink`, `ink_2`, `ink_muted` |
| Status | `danger`, `danger_2`, `ok`, `warn` |
| Older screens | `rail`, `panel`, `field`, `hover`, `selected`, `edge`, `text`, `text_soft`, `text_dim`, `text_faint` |

A colour you leave out follows the one it belongs with, if that is given: `sidebar` follows the background, `panel` the surface, `text` the ink, `accent_hover` becomes a lighter accent, and so on. So a theme of a few colours is complete.

### Glass

| Key | What it does | Range | Default |
|---|---|---|---|
| `opacity` | How much of the panel's colour lies over the blurred picture | 0–1 | 0.6 |
| `blur` | Blur radius in points | 0–64 | 24 |
| `edge` | The light edge round a panel | 0–1 | 0.16 |
| `highlight` | The sheen at the top of a panel | 0–1 | 0.10 |

With glass on, the pause menu's rail, the panels of its pages and lists, and the settings windows become frosted glass:

- The game's picture behind them is blurred, with the panel's colour over it.
- The picture is the frame the game just drew, blurred on the graphics card every frame. Nothing waits for the graphics card. So in a LAN session, which does not pause, the glass shows the game moving behind the menu.
- Where the window's picture cannot be copied, panels are see-through without blur.
- Buttons, action cards and fields stay solid in the theme's colours, so they read as controls.
- In a VR headset the menu takes the theme's colours but no glass.

### Logos

A logo is a file in the theme's folder: an SVG (sharp at any size) or a picture (PNG, TGA, BMP, JPEG or DDS, at most 4096 pixels wide and high). Nothing outside the theme's folder is read.

The pause menu's `logo` is drawn one of two ways:

- **Icon:** drawn 32 points square, with `title` beside it.
- **Wordmark:** a logo at least 1.6 times as wide as high is drawn across the top of the rail, instead of the icon and the name.

### Loading screen

| Key | What it sets |
|---|---|
| `background` | The loading screen's background colour |
| `text` | The colour of the map's name and the tips |
| `logo` | The loading logo file |
| `progress` | How progress is shown: `"bar"`, `"stroke"` or `"fill"` |

`progress` values:

- `"bar"` (default): openOMSI's striped bar along the bottom, in the theme's accent.
- `"stroke"`: the logo is the bar. Its path with `id="progress"` is drawn along as loading goes on, over a faint track of the whole path. Use one `<path ... />` without its own dash pattern: a ring, a line.
- `"fill"`: the logo fills from the left as loading goes on, faint and grey beyond.

`"stroke"` needs a `logo`. A logo without a `progress` path is filled instead of stroked.

**Elements that appear during loading.** In an SVG loading logo, a top-level element with an `id` and `data-at="0.4"` stays hidden until loading reaches that point (0 to 1; along the `progress` path when stroked). Then it pops up from its middle with a little overshoot.

- `data-blink`: it flashes twice instead (a light).
- `data-delay="0.4"`: it waits that many seconds after loading reached it.

```html
<path id="progress" d="M485 362 A167 167 0 1 0 334 459 L1673 459" stroke="#2a75f7" .../>
<path id="bus" data-at="0.33" d="..."/>
<g id="lights" data-at="0.33" data-delay="0.4" data-blink=""><circle .../></g>
<g id="stop-1" data-at="0.41"><circle cx="428" cy="459" r="61" .../></g>
```

## Themes from plugins

A plugin can bring a theme in its own folder (`plugins/<name>/theme.json`) and can switch themes:

| Function | What it does |
|---|---|
| `omsi.ui.themes()` | Lists the themes, each `{ id, name }`; openOMSI's own look first (id `""`) |
| `omsi.ui.theme()` | The theme in use (`""`: openOMSI's own) |
| `omsi.ui.set_theme(id)` | Switches to a theme, as if the player chose it |

A plugin should switch at most once, and only away from openOMSI's own look. The player can always switch back in the options. See [Plugin API reference](PLUGIN_API.md) and [Writing plugins](PLUGINS.md).
