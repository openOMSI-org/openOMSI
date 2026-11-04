# Interface themes

An interface theme gives the game's own screens a look of their own: the colours of the pause
menu, its settings windows and lists, the photo mode, the developer tools, the plugins' panels
and notifications; the panels as frosted glass over the game's picture; the logo at the top of
the pause menu; and the loading screen, whose logo can itself be the loading bar. The HUD, the
navigator and the cockpit keep their own look (the navigator follows its light or dark map).

The player chooses a theme in **Options › Interface › Theme pack**; openOMSI's own look is
the first entry. The choice is kept in the settings file (`ui_theme_pack`) and is in force from the
loading screen of the next start on - a theme that cannot be read leaves openOMSI's own look, with
a line in `game.log` saying why.

A theme goes over openOMSI's light or dark look, whichever the player chose in **Options ›
Interface › Interface theme** (`ui_theme`): its colours, corners and glass on top, and what it
leaves out from that look. A theme that gives all its surfaces and inks looks the same in both; one
of a few colours (an accent alone) is a light or a dark look with its accent.

## Where themes live

A theme is a folder with a `theme.json`; its id is the folder's name (letters, digits, `-`, `_`
and `.`):

| Folder | |
| --- | --- |
| `Themes/<id>/` in a content folder | a theme of its own (a mod) |
| `plugins/<name>/` | a plugin's theme, its id the plugin's name |

The content folders' `Themes` come first; of two themes of one id (in any case) the first is
used. A content folder another player's game sent in a LAN session brings no theme.

## theme.json

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

Everything is optional; what a theme leaves out stays openOMSI's (light or dark, as chosen).

| Key | |
| --- | --- |
| `name` | The theme's name in the options (40 characters at most; without it, the id). |
| `title` | The name at the top of the pause menu, beside `logo`, over openOMSI's version. |
| `colors` | Colours by name, as `"#rrggbb"`, `"#rrggbbaa"` or `[r, g, b]` / `[r, g, b, a]` (0 to 255; the opacity 0 to 1). |
| `radius` | The corners of the panels (0 to 24 points; openOMSI's are 8). |
| `glass` | `true`, or the glass's `opacity` (how much of a panel's colour lies over the blurred picture, 0 to 1), `blur` (its radius in points, 0 to 64), `edge` and `highlight` (the light edge round a panel and the sheen at its top, 0 to 1). |
| `loading` | The loading screen: `background`, `text` (the map's name and the tips), `logo` and `progress`. |
| `logo` | The logo at the top of the pause menu: an icon (drawn 32 points square, with `title` beside it), or a wordmark - a logo at least 1.6 times as wide as high, drawn across the top of the rail in place of the icon and the name. |

**Colours.** The names are those of the game's palette: `accent`, `accent_hover`, `accent_ink`
(text on the accent), `accent_2`, `background`, `sidebar`, `surface`, `surface_2`, `surface_3` (the
surfaces, a step lighter each), `ink`, `ink_2`, `ink_muted` (the texts), `danger`, `danger_2`,
`ok`, `warn`, and the older screens' `rail`, `panel`, `field`, `hover`, `selected`, `edge`, `text`,
`text_soft`, `text_dim`, `text_faint`. A colour left out follows the one it goes with when that
is given (`sidebar` the background, `panel` the surface, `text` the ink, `accent_hover` a lighter
accent ...), so a theme of a few colours is whole.

**Logos** are a file in the theme's folder: an SVG (drawn sharp at any size) or a picture (PNG,
TGA, BMP, JPEG, DDS; 4096 pixels at most). Nothing outside the folder is read.

**The loading bar.** `loading.progress` says how the loading screen shows how far it is:

- `"bar"` (the default): openOMSI's striped bar along the bottom, in the theme's accent;
- `"stroke"`: the logo is the bar - its path with `id="progress"` (one `<path ... />` without a
  dash of its own: a ring, a line) is drawn along as far as the loading is, over a faint track of
  the whole path;
- `"fill"`: the logo is filled from the left as far as the loading is, faint and grey beyond.

A logo without a path `progress` is filled instead of stroked.

**Things that come in.** An element at the top level of an SVG loading logo with an `id` and
`data-at="0.4"` stays hidden until the loading is that far (0 to 1, along the path `progress`
when the logo is stroked), then pops up from its middle with a little overshoot. With
`data-blink` it flashes twice instead (a light), and `data-delay="0.4"` waits that many seconds
after the loading reached it:

```svg
<path id="progress" d="M485 362 A167 167 0 1 0 334 459 L1673 459" stroke="#2a75f7" .../>
<path id="bus" data-at="0.33" d="..."/>
<g id="lights" data-at="0.33" data-delay="0.4" data-blink=""><circle .../></g>
<g id="stop-1" data-at="0.41"><circle cx="428" cy="459" r="61" .../></g>
```

## Glass

With `glass`, the pause menu's rail, the panels of its pages and lists, and the settings windows are panes
of frosted glass: the game's picture behind them blurred, the panel's colour over it at the
glass's opacity, a light edge and a sheen at the top. The picture is the frame the game has just
drawn, copied and blurred on the graphics card every frame the menu is open (nothing is read
back, nothing waits for the graphics card), so it moves where the game goes on behind the menu
(a LAN session, which does not pause). Where the window's picture cannot be copied, the panels
are see-through without the blur. In a headset the menu takes the theme's colours but no glass.
What is clicked - the buttons, the action cards, the fields - stays solid in the theme's colours
on the glass, so that it reads as a control and not as a pane.

## For plugins

`omsi.ui.themes()` lists the themes, `omsi.ui.theme()` says the one in use (`""`: openOMSI's
own) and `omsi.ui.set_theme(id)` switches to one, as if the player chose it (see
[PLUGINS.md](PLUGINS.md#an-interface-theme)). A plugin should do that once at most, and only
over openOMSI's own look: the player can always switch back in the options.
