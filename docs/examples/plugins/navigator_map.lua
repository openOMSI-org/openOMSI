-- navigator_map.lua: the navigator's frame as one panel - a title, the navigator's map with
-- the bus and the stops drawn the way this plugin likes them, and a line under it. Copy it into
-- the `plugins` folder next to the game (see docs/PLUGINS.md, "On-screen panels").

-- an older openOMSI has no frame panels
if not (omsi.ui and omsi.ui.version >= 6) then return end

local map = { type = "map", cards = true }
-- ui.version 8: how the map draws the bus and the stops (left out, it draws them as ever)
if omsi.ui.version >= 8 then
  map.marks = {
    bus = { color = "#1565C0", size = 14 },
    next_stop = "#2E7D32",
    last_stop = "#6A1B9A",
    stop = "#455A64",
    closed_stop = "#C62828", -- a stop closed by duty.set_closed_stops, a pin of its own
  }
end

local function show()
  local i = omsi.info()
  local ok, why = omsi.ui.set("map", {
    anchor = "navigator", frame = true, padding = 10, gap = 6, children = {
      { type = "text", text = i.line and ("Line " .. i.line) or "No duty", weight = "bold" },
      map,
      { type = "text", text = i.next_stop or "-", size = 13 },
    },
  })
  if not ok then omsi.warn("navigator map: " .. why) end
end

omsi.every(1, show)
omsi.on("vehicle", show)
