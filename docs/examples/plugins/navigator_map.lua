-- navigator_map.lua: the navigator's whole frame for a plugin's panel, with its map showing
-- only the route and the stops: no traffic, no other players, no icons (see docs/PLUGINS.md,
-- "In the navigator"). The layers are this map's alone; the player's settings stay as they
-- are, and the city map and the cockpit screens keep following them.

if not (omsi.ui and omsi.ui.version >= 6) then
  return
end

local map = { type = "map", height = 220 }
-- a layer left out follows the player's setting; `layers` needs ui.version 8
if omsi.ui.version >= 8 then
  map.layers = { cars = false, transit = false, jams = false, players = false, icons = false, stops = true }
end

omsi.ui.set("route_map", {
  anchor = "navigator",
  frame = true,
  padding = 10,
  children = {
    { type = "text", text = "Route", weight = "bold" },
    map,
    { type = "text", text = "Only the route and its stops", size = 12, color = "#8E8E8E" },
  },
})
