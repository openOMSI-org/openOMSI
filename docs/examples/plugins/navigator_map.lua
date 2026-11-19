-- navigator_map.lua: a panel that fills the navigator's frame - the line on top, the map under
-- it in sand and sea colours of its own, with dark cards over it, the next stop below. Copy it
-- into the `plugins` folder next to the game (see docs/PLUGINS.md, "In the navigator").

-- an older openOMSI has no panel in the navigator's frame, or no colours for its map: it
-- leaves the navigator as it is
if not (omsi.ui and (omsi.ui.version or 0) >= 8) then return end

local function show()
  local i = omsi.info()
  omsi.ui.set("navigator_map", { anchor = "navigator", frame = true, padding = 10, children = {
    { type = "text", text = i.line and ("Line " .. i.line) or "No duty", weight = "bold" },
    { type = "map", height = 220, colors = {
      land = "#E8E4D8", grass = "#E8E4D8", water = "#7FB2E5", road_edge = "#B9B4A8",
      card = "#1C1C1EF0", card_text = "#FFFFFF", card_dim = "#A1A1A8",
    } },
    { type = "text", text = "Next stop: " .. (i.next_stop or "-"), size = 13 },
  } })
end

omsi.every(1, show)
