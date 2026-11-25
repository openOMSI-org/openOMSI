-- navigator_map.lua: the navigator's frame with the map in it and three cards of the plugin's own
-- over the map, in place of the navigator's: the line and where it goes (top left), the next
-- stop (top right) and the speed (bottom right). Copy it into the `plugins` folder next to the
-- game (see docs/PLUGINS.md, "In the navigator") and open the navigator (Shift+N).

-- an older openOMSI has no cards over the map: it leaves `overlay` out, and the map keeps the
-- navigator's own cards (`cards` is false only where the plugin's are there in their place)
if not (omsi.ui and omsi.ui.version >= 6) then return end

local function card(place, icon, text, color)
  return { place = place, children = {
    { type = "row", gap = 6, children = {
      { type = "icon", name = icon, size = 18, color = color },
      { type = "text", text = text, weight = "bold", wrap = false },
    } },
  } }
end

local function show()
  local i = omsi.info()
  if not omsi.has_vehicle() then
    omsi.ui.remove("map")
    return
  end
  local line = i.line and ("Line " .. i.line) or (omsi.vehicle() or "")
  local next_stop = i.next_stop or "-"
  if i.next_stop_distance_nav then
    next_stop = string.format("%s  %.0f m", next_stop, i.next_stop_distance_nav)
  end
  local ok, why = omsi.ui.set("map", { anchor = "navigator", frame = true, padding = 10, children = {
    { type = "map", height = 360, cards = omsi.ui.version < 8, overlay = {
      card("top_left", "directions_bus", line .. (i.destination and (" → " .. i.destination) or ""), "#F47F30"),
      card("top_right", "location_on", next_stop, "#8E8E8E"),
      card("bottom_right", "speed", string.format("%.0f km/h", i.speed or 0), "#8E8E8E"),
    } },
  } })
  if not ok then omsi.warn("navigator map: " .. why) end
end

omsi.every(0.5, show)
omsi.on("vehicle", show)
