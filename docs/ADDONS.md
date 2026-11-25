# Add-ons

The launcher's **Add-ons** page lists projects other people make for openOMSI: tools, companions, content collections. This page explains what the page does and how to get your own project listed.

## The Add-ons page

Find it in the launcher's menu under **Content → Add-ons** (on a phone: **More → Add-ons**).

Each entry is a card with the project's name, author, a short description and an **Open website** button. The button says which site you are going to, then opens the project's page in your browser.

The page is a directory of links, nothing more:

- The launcher downloads, installs and runs nothing of an add-on.
- It shows no picture or text from the web. Everything on the cards ships with openOMSI, so the list changes only with a new openOMSI version.
- An add-on is not part of openOMSI, and the maintainers do not vouch for it.

The description is shown in your language when the entry has a translation, else in English.

## Get your add-on listed

1. Open a pull request that adds your entry at the end of the `addons` list in `launcher/ui/addons/addons.json`.
2. Optionally add your icon to `launcher/ui/addons/`.
3. Change nothing else in the same pull request.

The file looks like this; your entry goes into the `addons` array:

```json
{
  "version": 1,
  "addons": [
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
  ]
}
```

### Fields

| Field | Required | Rules |
|---|---|---|
| `id` | yes | Lowercase letters, digits and `-`, at most 40. Unique in the list. |
| `name` | yes | At most 40 characters. |
| `author` | yes | A person, group or company, at most 40 characters. |
| `description` | yes | English, at most 200 characters. What it does, not why it is the best. |
| `translations` | no | The description in other languages, keyed by the game's language codes (`de`, `pt-pt`, `zh-tw`, ...), each at most 200 characters. |
| `homepage` | yes | See below. |
| `icon` | no | A `.svg` or `.png` in `launcher/ui/addons/`, square, at most 10 KB. Without one the card shows a globe. |
| `tags` | no | Up to 5; lowercase letters, digits and `-`; at most 20 characters each. |

The `homepage` must be:

- an `https://` address of the project's own page on a named website, at most 200 characters;
- without a user name or password, a port, an IP address or `localhost`;
- without spaces, quotes, backslashes or `< > ^ { } |`.

An entry that breaks a rule is left out of the page. `cargo test -p omsi-launcher-api` checks the rules.

### Review

Before merging, the maintainers check that:

- the homepage is a real project page for openOMSI: it says what the add-on does, who makes it and how to get it, and it works;
- there is no advertising, no tracking pixels or link shorteners, and no paid-only teaser without a word of what it costs;
- nothing harms players or breaks the project's contributing rules or trademark policy (`CONTRIBUTING.md` and `TRADEMARKS.md` in the repository). An add-on does not present itself as openOMSI;
- the icon is your own work, or one you may share under the project's licence.

An entry is removed when its homepage is gone or no longer matches what the entry says.
