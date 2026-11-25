# Modern HTML engine

The modern engine is a second, opt-in engine for `[htmltexture]` pages. It adds modern CSS
(flexbox, positioning, borders, gradients, `calc()`) and full JavaScript (classes, modules,
`Promise`, `fetch` of the page's own files). This page is for modders whose displays outgrow the
classic engine.

> **Note:** This API is still under development. Breaking changes may occur.

Everything else is shared with the classic engine and described on [HTML textures](HTMLTEXTURE.md):

- the model setup (`[htmltexture]`, `[useHtmlTexture]`, `lit`, LED signs, script texture numbers);
- the whole `window.omsi` API: vehicle state, route, departures, depot, IBIS functions;
- touch and mouse input, and what scenery pages may use.

## When to use it

| Use the classic engine when | Use the modern engine when |
|---|---|
| The page is simple text and boxes | You need flexbox, positioning, borders, gradients or `overflow` |
| It must be as light as possible on slow devices | You want classes, modules, `JSON`, `Date`, regular expressions or `try/catch` |
| One HTML file is enough | You want separate `.css` and `.js` files, or to reuse web code |

See [Which engine](HTMLTEXTURE.md#which-engine) for the full comparison.

## Minimal example

The model setup is the same as for any page (see [Minimal example](HTMLTEXTURE.md#minimal-example)):

```text
[htmltexture]
1024
512
html\index.html
```

The page asks for the modern engine with one `<meta>` line in its `<head>`:

```html
<!doctype html>
<html>
<head>
  <meta name="omsi-engine" content="modern">
  <link rel="stylesheet" href="style.css">
</head>
<body>
  <header class="bar">
    <span>Line <b id="line">-</b></span>
    <span id="clock">--:--:--</span>
  </header>
  <main class="speed"><span id="speed">0</span> km/h</main>
  <script type="module" src="app.js"></script>
</body>
</html>
```

`style.css`:

```css
body { margin: 0; background: #111; color: #fff; display: flex; flex-direction: column; height: 100vh; }
.bar { display: flex; justify-content: space-between; padding: 12px 24px; background: #0a4; }
.speed { flex: 1; display: flex; align-items: center; justify-content: center; font-size: 160px; }
```

`app.js`:

```js
const el = (id) => document.getElementById(id);
const last = {};
function set(id, value) {
  if (last[id] === value) return;
  last[id] = value;
  el(id).textContent = value;
}

omsi.update = ({ vehicle }) => {
  set('speed', String(Math.round(vehicle.motion.speedKmh ?? 0)));
  set('line', vehicle.route.line || '-');
  set('clock', omsi.time.asString);
};
```

A page without the `<meta>` line always runs on the classic engine. A larger example, a page
written as a small site with modules and classes, is in `docs/examples/htmltexture/site/` in the
openOMSI repository.

> **Tip:** To compare both engines on existing pages, `OMSI_HTML_ENGINE=modern` runs every page on
> the modern engine (see [Debug flags](DEBUG_FLAGS.md)).

## HTML

A page can be a small site: `.html`, `.css` and `.js` files (and ES modules) in its folder,
linked as on the web. The engine is not a browser, so a page that looks fine in Chrome may still
need changes. When something does not show, check these lists first.

- Nested elements, `<style>`, `<script>`, `<link rel="stylesheet" href>`, `<script src>`, inline
  `style=""`, all attributes (`data-*` too), entities such as `&amp;`.
- Block elements (`div`, `p`, `h1`-`h3`, `section`, `ul` …) with browser-like default sizes and
  margins.
- Inline elements: `span`, `b`, `strong`, `i`, `em`, `a`, `small`, `u`, `s`, `label`, `code` …,
  and `<br>`.
- `<button>` has a default look. `<tr>` is a row, and `<td>`/`<th>` are equal cells of it.
- `<img src width height>` with bmp, dds, tga, jpg and png, looked up next to the page.
- `<head>`, `<title>`, `<meta>` and `<template>` are read but not drawn.
- A `<script>` whose `type` is not JavaScript or `module` (a JSON block, a template) is not run.

Not available: canvas, list markers (bullets), form controls beyond a look of your own, iframes,
video, SVG. Text is drawn in Inter (with a fallback font for characters Inter lacks, as in the
[classic engine](HTMLTEXTURE.md#tips)).

## CSS

| Area | Supported |
|---|---|
| Selectors | Tag, `#id`, `.class`, `*`, compounds (`div.a#b`), attribute selectors (`[a]`, `[a=b]`, `^=`, `$=`, `*=`, `~=`, `\|=`), comma lists |
| Combinators | Space, `>`, `+`, `~` |
| Pseudo-classes | `:first-child`, `:last-child`, `:only-child`, `:nth-child()`, `:nth-last-child()`, `:nth-of-type()`, `:first-of-type`, `:last-of-type`, `:not()`, `:empty`, `:root` |
| Cascade | Specificity and `!important` |
| Functions | `calc()`, `min()`, `max()`, `clamp()`, custom properties `--name` with `var()` (inherited, with fallbacks) |
| Colours | Names, `#rgb`, `#rrggbb`, `rgb()`, `rgba()`, `hsl()`, `hsla()`, `currentcolor` |
| Units | `px`, `%`, `em`, `rem`, `ex`, `ch`, `pt`, `pc`, `cm`, `mm`, `in`, `vw`, `vh`, `vmin`, `vmax` |

`:hover`, `:focus`, `::before` and `::after` never match: a texture has no pointer state and no
generated content.

### Properties

| Group | Properties |
|---|---|
| Box | `width`, `height`, `min-`/`max-width`, `min-`/`max-height`, `margin` (with `auto`), `padding`, `box-sizing`, `visibility`, `display` |
| Border | `border` and per-side forms (all styles draw solid), `border-radius` (one radius; `50%` makes a circle) |
| Effects | `overflow` (`hidden` clips), `opacity`, `box-shadow` (the first shadow, not inset) |
| Position | `position: relative \| absolute \| fixed`, `top`, `right`, `bottom`, `left`, `inset`, `z-index` |
| Flexbox | `display: flex \| inline-flex`, `flex-direction`, `flex-wrap`, `flex-flow`, `justify-content`, `align-items`, `align-self`, `gap`, `row-gap`, `column-gap`, `flex`, `flex-grow`, `flex-shrink`, `flex-basis`, `order`, auto margins |
| Background | `background-color`, `background-image: url()`, `linear-gradient()` (angles, `to` directions, any number of stops), `background-size`, `background-repeat`, `background-position` |
| Text | `color`, `font-size`, `font-weight`, `text-align`, `line-height`, `letter-spacing`, `text-transform`, `text-decoration` (underline, line-through), `white-space: nowrap`, `text-overflow: ellipsis` (with `overflow: hidden`), `vertical-align: middle` for inline blocks |

- An absolute box is placed in its nearest positioned ancestor. `fixed` is relative to the texture.

Not available: grid (laid out as a column), floats, animations and transitions, custom fonts,
`@media`, `@font-face`, `@keyframes`, per-corner radii, outlines.

## JavaScript

Scripts run in Boa, a complete JavaScript engine:

- **Syntax:** classes with private fields, arrow functions, template strings, destructuring,
  spread, optional chaining, `??`, generators, `async`/`await`, ES modules.
- **Standard library:** `Promise`, `Map`, `Set`, `WeakMap`, `JSON`, `RegExp`, `Date`, `Math`,
  typed arrays, `Symbol`, `Proxy`, `Reflect` and more. Not included: `Intl`.
- Promises run after the script that created them.

### Browser APIs

| Area | Available |
|---|---|
| Modules | `<script type="module">` (inline or `src`) and `import … from './file.js'`, relative to the importing file, from the page's folders only |
| Document | `document.getElementById`, `querySelector`, `querySelectorAll`, `getElementsBy…`, `createElement`, `createTextNode`, `body`, `documentElement` |
| Tree | `appendChild`, `append`, `prepend`, `insertBefore`, `before`, `after`, `replaceWith`, `remove`, `cloneNode`, `children`, `childNodes`, `parentNode`, `firstChild` …, `nextElementSibling` … |
| Content | `textContent`, `innerText`, `innerHTML`, `outerHTML`, `insertAdjacentHTML` |
| Attributes | `id`, `className`, `classList`, `dataset`, `getAttribute` …, `hidden`, `value`, `src` |
| Style | `style` (any property, `setProperty`, `cssText`); `getComputedStyle(el)` gives size, display, colour, opacity and box properties of a laid-out element |
| Geometry | `matches`, `closest`, `getBoundingClientRect`, `offsetWidth` … |
| Events | `addEventListener` (with `once`), `removeEventListener`, `on…` properties and `on…=""` attributes, `dispatchEvent`, `Event`, `CustomEvent` |
| Timers | `setTimeout`, `setInterval`, `clearTimeout`, `clearInterval`, `requestAnimationFrame` |
| Files | `fetch('data.json')` reads a file below the page's folders (`text()`, `json()`), up to 4 MB |

- A press on the texture fires `pointerdown` and `mousedown`; the release fires `pointerup`,
  `mouseup` and `click`. Events bubble up to `document` and `window`. Moves fire `mousemove`.
- `DOMContentLoaded` and `load` fire once the scripts have run.
- Timers run while the page is drawn and use real time. At most 64 are active at once.
- `fetch` reaches nothing outside the page's folders. There is no network.

Browser objects that exist so that libraries do not stop: `window`, `navigator`, `location`,
`history`, `screen`, `localStorage`/`sessionStorage` (kept while the page lives),
`performance.now()`, `matchMedia`, `MutationObserver` and similar observers (they do nothing),
`atob`/`btoa`, `structuredClone`, `console`.

### Limits

These limits protect the game from runaway pages:

| Limit | Value |
|---|---|
| Iterations of one loop | About 5 million |
| Nested function calls | 400 |

A script that goes past a limit is stopped with an error in the log. The page and its other
scripts go on, and a `catch` cannot swallow the stop.

## Tips

- Compare against the last value before writing to the DOM, as in the example. Every DOM change
  causes a redraw.
- `console.log(...)` writes to the game's log with the prefix `htmltexture console:`.
- The `window.omsi` data is the same as in the classic engine: see
  [The omsi object](HTMLTEXTURE.md#the-omsi-object).
