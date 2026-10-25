-- A barrier arm that lifts when a vehicle is near and lowers again.
-- Mesh 2 (arm.o3d) turns about the point (0, 0, 1.2) on the post, 90 degrees up.
local state = {}

omsi.on("object_init", function(id, kind, x, y, z, heading)
  state[id] = { angle = 0 }
end)

omsi.on("object_frame", function(id, dt)
  local s = state[id]
  if not s then return end
  local wanted = omsi.object.var(id, "gate_open") or 0
  local step = 60 * dt
  if s.angle < wanted * 90 then s.angle = math.min(s.angle + step, wanted * 90)
  elseif s.angle > wanted * 90 then s.angle = math.max(s.angle - step, wanted * 90) end
  omsi.object.set_mesh(id, "arm", { ry = -s.angle, pz = 1.2 })
end)

omsi.on("object_click", function(id, event)
  local open = omsi.object.var(id, "gate_open") or 0
  omsi.object.set_var(id, "gate_open", 1 - open)
end)

omsi.on("object_gone", function(id) state[id] = nil end)
