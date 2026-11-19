-- navigator_map.lua: the navigator's frame is a panel of the plugin with the navigator's map
-- in it, and only the cards over the map that the plugin wants: here the speed and the next
-- stop, without the next turn and the clock (see docs/PLUGINS.md, "On-screen panels"). Copy it
-- into the `plugins` folder next to the game.

-- an openOMSI before version 8 has no `show`: it keeps all four cards over the map, and one
-- before version 6 has no panel in the navigator's frame at all, so nothing is done there
if not (omsi.ui and omsi.ui.version >= 6) then
  return
end

local function show()
  local i = omsi.info()
  if not omsi.has_vehicle() then
    omsi.ui.remove("navigator")
    return
  end
  local map = { type = "map", height = 200 }
  if omsi.ui.version >= 8 then
    -- each card is on unless it says false
    map.show = { turn = false, clock = false }
  end
  omsi.ui.set("navigator", {
    anchor = "navigator", frame = true, padding = 10, gap = 6, children = {
      { type = "text", text = i.line and ("Line " .. i.line) or "Free drive", weight = "bold" },
      map,
      { type = "text", text = i.next_stop or "-" },
    },
  })
end

omsi.every(1, show)
show()
