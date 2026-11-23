# Presence: "playing now"

A Cloudflare Worker that counts the openBus games being played right now. A running game
posts `/ping` every ten minutes and `/bye` when it ends (`crates/omsi-app/src/presence.rs`,
setting `presence`, on by default; after a 429 it waits half an hour); a session counts for
25 minutes after its last ping.
What it gets is a random id made for that session, the game's version and the kind of system -
no name, and no address is stored.

- `GET /players` - `{"players": 12, "systems": {"windows": 9, "linux": 2, "android": 1}, "updated": "...", "next": "..."}`
  (the website reads it)
- `GET /badge` - the count for a [shields.io endpoint badge](https://shields.io/badges/endpoint-badge)
  (the README shows it)

Both give the same count: it is taken once every ten minutes, on the clock (:00, :10, :20, ...),
and cached until the next one (`updated`, `next`). The website asks again right after `next`;
the badge passes through shields.io's and GitHub's caches, which keep it up to five minutes
more.

## Deploy

Cloudflare's Workers Builds deploys it from this repository on every push to `main`
(Worker `openbus`, root directory `services/presence`, deploy command `npx wrangler deploy`).
By hand: `npx wrangler login` once, then `npx wrangler deploy` in this folder.

It runs on Cloudflare's free plan (a Worker and one Durable Object, which keeps the sessions in memory and nothing in its storage): 100 000
requests a day, each ping one. That holds about 650 players at once around the clock (more
in practice: nobody plays all day); over it Cloudflare answers everything with error 1027
until midnight UTC. Workers Paid (5 USD a month, 10 million requests) lifts the limit. The address
`wrangler deploy` prints goes into `SERVICE` in `crates/omsi-app/src/presence.rs`, the badge in
`README.md` and `PRESENCE` in `site/app.js`.
