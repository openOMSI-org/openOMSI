# Add-ons

The launcher's **Add-ons** page lists projects other people make for openOMSI: tools, companions, content collections. This page explains what the page does and how to get your own project listed.

## The Add-ons page

Find it in the launcher's menu under **Content → Add-ons** (on a phone: **More → Add-ons**).

Each entry is a card with the project's name, author, a short description and an **Open website** button. The button says which site you are going to, then opens the project's page in your browser.

The page is a directory of links, nothing more:

- The launcher downloads, installs and runs nothing of an add-on.
- The list is [openOMSI-org/add-ons-list](https://github.com/openOMSI-org/add-ons-list). The launcher reads it each time the page opens, so an entry added there shows up for everyone, and an entry taken out disappears, without a new openOMSI version.
- An icon is downloaded from the same repository and shown only when it is exactly the file the list names (its size and SHA-256). The page loads nothing else from the web.
- Without an internet connection the page says the list could not be loaded, with a **Refresh** button.
- An add-on is not part of openOMSI, and the maintainers do not vouch for it.

The description is shown in your language when the entry has a translation, else in English.

## Get your add-on listed

Open a pull request in [openOMSI-org/add-ons-list](https://github.com/openOMSI-org/add-ons-list) that adds a folder `addons/<id>/` with your `addon.json` and, optionally, `icon.svg` or `icon.png`, and the rebuilt `addons.json` (`python3 tools/build_list.py`). Its [CONTRIBUTING.md](https://github.com/openOMSI-org/add-ons-list/blob/main/CONTRIBUTING.md) lists the fields and their rules; the repository's checks run them on every pull request, and the launcher checks them again and leaves out an entry that breaks one.

### Review

Before merging, the maintainers check that:

- the homepage is a real project page for openOMSI: it says what the add-on does, who makes it and how to get it, and it works;
- there is no advertising, no tracking pixels or link shorteners, and no paid-only teaser without a word of what it costs;
- nothing harms players or breaks the project's contributing rules or trademark policy (`CONTRIBUTING.md` and `TRADEMARKS.md` in the repository). An add-on does not present itself as openOMSI;
- the icon is your own work, or one you may share under the project's licence.

An entry is removed when its homepage is gone or no longer matches what the entry says. The launcher stops showing it as soon as the removal is merged.
