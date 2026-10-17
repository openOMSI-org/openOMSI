# Add-ons

The launcher's **Add-ons** page (in the menu under Content; on a phone under More) lists
projects others make for openOMSI: tools, companions, content collections. Each entry is a
card with the project's name, author, a short description and an **Open website** button,
which opens the project's own page in the browser after saying which site it leaves for.

The page is a directory of links, nothing more. The launcher downloads, installs and runs
nothing of an add-on, and shows no picture or text from the web: everything on the cards comes
with openOMSI. An add-on is not part of openOMSI, and the maintainers do not vouch for it.

## Adding your add-on

Open a pull request that adds your entry to
[`launcher/ui/addons/addons.json`](../launcher/ui/addons/addons.json) (and, if you like, your
icon to that folder) - nothing else in the same pull request. New entries go at the end.

```json
{
  "id": "my-addon",
  "name": "My add-on",
  "author": "Your name",
  "description": "What it does, in one or two plain sentences.",
  "translations": { "de": "Was es tut, in ein oder zwei Sätzen." },
  "homepage": "https://example.org/my-addon",
  "icon": "my-addon.svg",
  "tags": ["tools"]
}
```

| Field | Required | Rules |
| --- | --- | --- |
| `id` | yes | lowercase letters, digits and `-`, at most 40; unique in the list |
| `name` | yes | at most 40 characters |
| `author` | yes | a person, group or company, at most 40 characters |
| `description` | yes | English, at most 200 characters; what it does, not why it is the best |
| `translations` | no | the description in other languages, by the codes of `crates/omsi-app/locales/app.yml` (`de`, `pt-pt`, `zh-tw`, ...); the page shows the player's language, else English |
| `homepage` | yes | an `https://` address of the project's own page on a named website, at most 200 characters: no user name or password in it, no port, no IP address or localhost |
| `icon` | no | a `.svg` or `.png` file in `launcher/ui/addons/`, square, at most 10 KB; without one the card shows a globe |
| `tags` | no | up to 5, lowercase, at most 20 characters each |

`cargo test -p omsi-launcher-api` checks these rules; an entry that breaks one is left out of
the page.

What the maintainers look for before merging:

* the homepage is a real project page for openOMSI: it says what the add-on does, who makes it
  and how to get it, and it works;
* no advertising, no tracking pixels or link shorteners, no paid-only teaser without a word
  of what it costs;
* nothing that harms players or breaks the rules of [CONTRIBUTING.md](../CONTRIBUTING.md) or
  the [trademark policy](../TRADEMARKS.md) - an add-on does not present itself as openOMSI;
* the icon is your own work (or one you may share under the project's licence).

An entry is removed when its homepage is gone or no longer matches what the entry says.
