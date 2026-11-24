# Public server list

A Cloudflare Worker with one Durable Object (the same shape as `../presence`) that lists the
openOMSI servers whose owners set `public_server = 1` in `server.cfg`. The launcher
(Multiplayer -> Servers -> Public servers) and the game ask it for the list.

```
POST /v1/register   {name, motd, map, max_players, version, protocol, web_port, address?, icon?}
                    -> {id, token, challenge, ttl, interval}
POST /v1/heartbeat  {id, token, players, map}      -> {verified, error?}     (every minute)
POST /v1/unlist     {id, token}                    -> 204
GET  /v1/servers    -> {servers: [{id, address, name, motd, map, players, max_players,
                                   version, protocol, has_icon, age}]}      (cached 15 s)
GET  /v1/servers/<id>/icon.png
```

## What keeps it honest

* **Reachable, and the server's own.** A registration gets a random challenge. The server
  puts it on its own `GET /status` (`list_challenge`); the first heartbeat makes the service
  fetch `<address>/status` and look for it, with the name. Only then is the server listed, and
  the page is read again every 30 minutes. The address is the server's tunnel
  (`https://….trycloudflare.com`, a plain https host name) or **the address the request came
  from** plus the `web_port` it names - nobody can list an address that is not theirs, and
  the service fetches nothing anybody else chose except the https host name of a tunnel (no
  redirects, 5 s, 64 KB). Addresses of private networks are refused.
* **Alive.** A listing is forgotten 3 minutes after its last heartbeat; heartbeats faster than
  every 10 s are refused. Updating needs the token the registration returned (kept as a
  SHA-256 only).
* **Abuse.** At most 8 registrations an hour and 3 listings per source address, 2000 in all,
  bodies of 32 KB, an icon of 16 KB and 64x64 at most, names of 48 characters and messages of
  120 without hidden characters, addresses or invitations; `BLOCKED_TERMS` (a Worker variable,
  comma separated) adds the operator's own words.
* **Privacy.** Nothing about players is sent or kept: only the server's name, message, map,
  player count, version and address. The sender's address is kept as a hash for the rate limit
  and in the listing only as the server's address when it has no tunnel. The listings are
  kept in the Durable Object's storage (Cloudflare hibernates an idle object within seconds
  and its memory goes with it) and deleted with the listing: on unlist, or 3 minutes after
  the last heartbeat.
* **The service is not the only check.** The server's name, message and icon pass the game's
  moderation (`crates/omsi-serverlist`) before it registers, and every launcher and game checks
  each listing again before showing it (a flagged text is covered, an icon is shown only if the
  client cleared it itself). A custom icon is therefore only uploaded when the server's image
  check cleared it, and shown only when the viewer's check does.

## Deploy

As `../presence`: `npx wrangler deploy` in this folder (Workers Builds on the repository, root
directory `services/serverlist`). The address it prints is `DEFAULT_SERVICE` in
`crates/omsi-serverlist/src/lib.rs`; a server can name another with `public_list_url`.
`node --test logic.test.mjs` tries the rules.

Open points for the first deploy: whether the Worker may fetch `http://<ip>:<web_port>/status`
(a port that is not 80/443) on the plan in use - if not, only tunnelled servers (https, 443)
get verified, which is what most hosts have anyway; and the free plan's 100 000 requests a
day (a server's heartbeat is one request a minute, so about 60 servers around the clock leave
room for the launchers' reads, which the 15 s edge cache keeps small).
