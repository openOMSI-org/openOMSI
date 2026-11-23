# The website

openomsi.org is a static site built from this folder and from [`docs/`](../docs). There is no
framework and no bundler: `build.mjs` is one Node script (Node 20 or newer) that writes plain HTML,
CSS and a few small scripts into `site/dist/`. Every page works without JavaScript; scripts only add
search, filters for long tables, copy buttons and the live parts (latest release, downloads,
release notes, issues, the players counter).

```bash
cd site
npm ci
node build.mjs            # writes site/dist/; --check fails on a broken internal link
python3 -m http.server -d dist 4173
```

`.github/workflows/pages.yml` runs the same build and publishes `site/dist/` to GitHub Pages.

## Where things are

| Path | What |
| --- | --- |
| `src/config.mjs` | the documentation menu (groups, titles, one-line summaries), the downloads, project links |
| `src/md.mjs` | Markdown to HTML: callouts, typed table columns, code highlighting, heading ids |
| `src/docs.mjs` | reads `docs/*.md`, cuts long documents into chapters, resolves links between documents |
| `src/pages.mjs`, `src/layout.mjs` | the page templates (plain template strings) |
| `static/css/site.css` | the whole style sheet; the palette is the launcher's (`launcher/ui/styles.css`) |
| `static/js/` | `theme.js` (before first paint), `site.js` (search, copy, filters), `dynamic.js` (GitHub data) |
| `privacy/index.html` | the text of the privacy policy, put into the site's frame |

## Writing documentation

A document is `docs/NAME.md`. A new one must be listed in `src/config.mjs` to get a place in the
menu (the build warns about, and still publishes, a file that is not listed).

* A document longer than about 26 KB is published as one page per `##` chapter; a chapter longer
  than 28 KB with at least three `###` sections is cut once more. Headings keep their GitHub ids, so
  `FILE.md#heading` links work wherever the heading ends up. Write `[text](OTHER.md#heading)`.
* `> **Note:** ...`, `> **Warning:** ...`, `> [!NOTE]` and the like become callouts (note, important,
  warning, caution, tip, experimental, legacy, deprecated ...).
* In a table, a column headed *Key* or *Keys* shows its code as keys; *Type*, *Kind*, *Read*, *Since*,
  *Status* and the like as small tags. A table of twelve rows or more gets a filter field.
* A code block without a language is guessed: `[keyword]` lines are an OMSI file, shell prompts a
  shell, box-drawing characters a diagram. Name the language when the guess is wrong.

Old addresses (`https://openomsi.org/#/docs/USER_GUIDE`) are sent to their new pages by
`assets/js/legacy.js`, which the build writes from the document list.

openMapper's own documents are read from its repository at build time (and cached in `.cache/`).

## Releases and issues

The Download, Releases and Issues pages and the release notes on the home page are not fetched from
GitHub by the visitor's browser. The build reads them (`src/github.mjs`: releases with their notes
rendered, the 100 newest open and closed issues) into `assets/data/`, so the pages fill in at once and
GitHub's request limit or a bad answer from it cannot break them. The page asks GitHub once in the
background whether a newer release is out; for issues it asks for the current list and falls back to the
baked one. The workflow rebuilds the site on every release event and daily. The last good answer is cached in
`.cache/`, which a build without network access uses.
