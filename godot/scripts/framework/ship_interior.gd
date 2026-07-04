extends Node2D
## Ring 0 — ShipInterior (REBUILT, Sprint 03, P10).
##
## The walkable 2D top-down ship interior. Rooms are physical areas on a
## tiled floor. Stations (pilot console, weapons, engineering, scanner,
## cargo) are interactable objects the player walks up to and activates.
## Crew member sprites stand at their assigned stations.
##
## Everything is data-driven: rooms from hull.interior_rooms, stations
## from hull.stations (or a default set), crew placement from CrewRoster.
## A modder redesigns an interior by editing data files only.
##
## This replaces the old PanelContainer-based ShipInterior entirely.

class_name ShipInterior

signal launch_requested
signal disembark_requested

const WALK_SPEED := 200.0
const INTERACT_RANGE := 48.0

const INTERIOR_SIZE := Vector2(1280, 800)

## Room type → tile color (stand-in until Kenney tiles are integrated).
## The framework default when a hull supplies no room_zones.
const DEFAULT_ROOM_COLORS := {
	"cockpit": Color(0.25, 0.45, 0.70),
	"bridge": Color(0.25, 0.45, 0.70),
	"engineering": Color(0.80, 0.45, 0.15),
	"med_bay": Color(0.85, 0.85, 0.88),
	"galley": Color(0.55, 0.65, 0.35),
	"crew_quarters": Color(0.35, 0.40, 0.65),
	"cargo_hold": Color(0.45, 0.40, 0.30),
	"airlock": Color(0.50, 0.55, 0.58),
	"cryo": Color(0.52, 0.70, 0.78),
}
const NEUTRAL_ROOM := Color(0.35, 0.35, 0.38)

## Default station positions (normalized 0..1 within the room rect)
## for common station types. Mod overrides via hull.stations[].position.
const DEFAULT_STATION_POSITIONS := {
	"pilot": Vector2(0.5, 0.30),
	"weapons": Vector2(0.5, 0.65),
	"engineering": Vector2(0.5, 0.70),
	"scanner": Vector2(0.70, 0.40),
	"cargo": Vector2(0.20, 0.55),
	"cryopod": Vector2(0.50, 0.40),
}

var _hull: Dictionary = {}
var _rooms: Array = []          # {id, name, kind, rect:Rect2, color}
var _stations: Array = []       # {id, name, kind, pos:Vector2, room_id}
var _interactables: Array = []  # {kind, name, pos, ref, station_id}

var _walker: _Walker
var _camera: Camera2D
var _pos: Vector2 = INTERIOR_SIZE * 0.5
var _frozen := false
var _spawner: NpcSpawner
var _spawned: Array = []

var _hud: CanvasLayer
var _log: RichTextLabel
var _hint: Label


func _ready() -> void:
	_spawner = NpcSpawner.new()
	add_child(_spawner)


## Point the interior at a hull (the ship's own dictionary). Builds rooms
## from interior_rooms and stations from hull.stations (or defaults).
func configure(hull: Dictionary) -> void:
	_hull = hull
	_build_rooms()
	_build_stations()
	_build_world()
	_build_hud()

	# Spawn crew at their stations
	var context: String = "Aboard the %s." % hull.get("name", "ship")
	_spawned = _spawner.spawn_souls(CrewRoster.aboard(), context)
	for soul in _spawned:
		soul.spoke.connect(_on_crew_spoke.bind(soul))
		soul.concluded.connect(_on_crew_concluded.bind(soul))

	# Reset ShipOperation for this session
	ShipOperation.reset()
	
	# Configure gravity from hull
	var grav_config: Dictionary = hull.get("gravity", {"type": "none", "strength": 0.0, "safe": false})
	GravitySystem.configure(grav_config)
	
	_refresh()


func _build_rooms() -> void:
	var room_data: Array = _hull.get("rooms", [])
	var zones: Dictionary = _hull.get("room_zones", {})
	
	_rooms.clear()
	
	if not room_data.is_empty():
		# Freeform rooms from hull data
		for entry: Dictionary in room_data:
			var room_id: String = entry.get("id", "")
			var room_kind: String = entry.get("kind", room_id)
			var rect := Rect2(
				entry.get("x", 0.0), entry.get("y", 0.0),
				entry.get("w", 100.0), entry.get("h", 100.0)
			)
			# Color: room-specified > room_zones > DEFAULT_ROOM_COLORS by kind
			var color: Color = DEFAULT_ROOM_COLORS.get(room_kind, NEUTRAL_ROOM)
			var hex := entry.get("color", "")
			if hex != "":
				color = Color.from_string(hex, color)
			elif zones.has(room_id):
				color = Color.from_string(str(zones[room_id]), color)
			
			# Parse doors
			var doors: Array = []
			for d: Dictionary in entry.get("doors", []):
				var door := {
					"to": d.get("to", ""),
					"side": d.get("side", ""),
					"offset": d.get("offset", 0.5),
					"width": d.get("width", 40.0),
				}
				# Auto-detect side from relative positions if omitted
				if door.side == "":
					door.side = _auto_door_side(rect, room_id, door.to, room_data)
				doors.append(door)
			
			# Build stations inside this room
			var stations: Array = []
			for s: Dictionary in entry.get("stations", []):
				stations.append({
					"id": s.get("id", ""),
					"name": s.get("name", s.get("id", "").capitalize()),
					"pos": Vector2(
						s.get("x", rect.position.x + rect.size.x * 0.5),
						s.get("y", rect.position.y + rect.size.y * 0.5),
					),
				})
			
			_rooms.append({
				"id": room_id,
				"name": entry.get("name", room_id.capitalize().replace("_", " ")),
				"kind": room_kind,
				"rect": rect,
				"color": color,
				"doors": doors,
				"stations": stations,
			})
			
			# Register stations in the global station list
			for s: Dictionary in stations:
				_stations.append(s)
	else:
		# Fallback: grid layout from interior_rooms (legacy compat)
		var room_ids: Array = _hull.get("interior_rooms", [])
		var cols := 4
		var cell_w := INTERIOR_SIZE.x / cols
		var cell_h := 160.0
		var gap := 12.0
		for i in room_ids.size():
			var room_id: String = room_ids[i]
			var col := i % cols
			var row := i / cols
			var rect := Rect2(
				Vector2(col * cell_w + gap * 0.5, row * cell_h + gap * 0.5),
				Vector2(cell_w - gap, cell_h - gap)
			)
			var color: Color = DEFAULT_ROOM_COLORS.get(room_id, NEUTRAL_ROOM)
			if zones.has(room_id):
				color = Color.from_string(str(zones[room_id]), color)
			_rooms.append({
				"id": room_id,
				"name": room_id.capitalize().replace("_", " "),
				"kind": room_id,
				"rect": rect,
				"color": color,
				"doors": [],
				"stations": [],
			})


func _build_stations() -> void:
	# Stations are now embedded in rooms. Called only for legacy compat
	# when rooms have no stations defined. Keep empty — stations are
	# populated during _build_rooms().
	pass


func _rebuild_interactables() -> void:
	_interactables.clear()
	for s: Dictionary in _stations:
		_interactables.append({
			"kind": "station",
			"name": s.name,
			"pos": s.pos,
			"station_id": s.id,
			"ref": s,
		})


func _find_room(room_id: String) -> Dictionary:
	for r: Dictionary in _rooms:
		if r.id == room_id:
			return r
	return {}


## --- world / camera ----------------------------------------------------------


func _build_world() -> void:
	# Background sky — dark space through the hull
	var bg := ColorRect.new()
	bg.color = Color(0.08, 0.10, 0.14)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(bg)

	# Floor: render rooms as colored rects (placeholder until Kenney tiles)
	var floor := _Floor.new()
	floor.setup(_rooms, _stations)
	add_child(floor)

	# Player walker
	_walker = _Walker.new()
	_walker.position = _player_start()
	add_child(_walker)

	# Camera
	_camera = Camera2D.new()
	_camera.position = _walker.position
	_camera.zoom = Vector2(1.2, 1.2)
	add_child(_camera)
	_camera.make_current()

	_rebuild_interactables()


func _player_start() -> Vector2:
	# Start at the cryopod station, or the first station
	for s: Dictionary in _stations:
		if s.id == "cryopod":
			return s.pos + Vector2(0, 60)
	if _stations.size() > 0:
		return _stations[0].pos + Vector2(0, 60)
	return INTERIOR_SIZE * 0.5


func _process(delta: float) -> void:
	if not _frozen:
		var move := Input.get_vector("strafe_left", "strafe_right", "thrust_forward", "thrust_back")
		
		# Gravity-aware movement
		var gfx := GravitySystem.apply_movement(move, delta, _walker.velocity)
		_walker.velocity = gfx.get("velocity", Vector2.ZERO)
		
		var effective_move: Vector2 = gfx.get("move", move)
		var effective_delta: float = gfx.get("delta", delta)
		
		if effective_move != Vector2.ZERO:
			_pos += effective_move * WALK_SPEED * effective_delta
			_pos = _pos.clamp(Vector2(20, 20), INTERIOR_SIZE - Vector2(20, 20))
			_walker.position = _pos
			if move != Vector2.ZERO:
				_walker.facing = signf(move.x) if absf(move.x) > 0.1 else _walker.facing
		elif _walker.velocity != Vector2.ZERO:
			# Zero-G drift
			_pos += _walker.velocity * effective_delta
			_pos = _pos.clamp(Vector2(20, 20), INTERIOR_SIZE - Vector2(20, 20))
			_walker.position = _pos
		
		_camera.position = _camera.position.lerp(_pos, 1.0 - exp(-6.0 * delta))
		if Input.is_action_just_pressed("interact"):
			_interact()
	_update_hint()


func _auto_door_side(from_rect: Rect2, from_id: String, to_id: String, all_rooms: Array) -> String:
	# Auto-detect which side a door should be on by comparing room positions
	for r: Dictionary in all_rooms:
		if r.get("id", "") == to_id:
			var to_rect := Rect2(r.get("x", 0.0), r.get("y", 0.0),
				r.get("w", 100.0), r.get("h", 100.0))
			var dx := (to_rect.position.x + to_rect.size.x * 0.5) - (from_rect.position.x + from_rect.size.x * 0.5)
			var dy := (to_rect.position.y + to_rect.size.y * 0.5) - (from_rect.position.y + from_rect.size.y * 0.5)
			if absf(dx) >= absf(dy):
				return "right" if dx > 0.0 else "left"
			else:
				return "bottom" if dy > 0.0 else "top"
	return "right"


## --- handle station interaction ---------------------------------------------


func _nearest() -> Dictionary:
	var best := {}
	var best_d := INTERACT_RANGE
	for it: Dictionary in _interactables:
		var d: float = _pos.distance_to(it.pos)
		if d < best_d:
			best_d = d
			best = it
	return best


func _update_hint() -> void:
	if _hint == null:
		return
	if _frozen:
		_hint.text = ""
		return
	var target := _nearest()
	if target.is_empty():
		_hint.text = ""
		return
	_hint.text = "R — use %s" % target.name


func _interact() -> void:
	var target := _nearest()
	if target.is_empty():
		return
	if target.kind == "station":
		_use_station(target)


func _use_station(target: Dictionary) -> void:
	var station_id: String = target.station_id
	if station_id == "":
		return

	# Player occupies this station
	ShipOperation.occupy(station_id, "player")

	match station_id:
		"pilot":
			_append_log("[i]You take the pilot's seat.[/i]")
			# Player is now piloting. WASD now controls ship, not walker.
			_frozen = true
			# On press of R or Esc, release station
		"weapons":
			_append_log("[i]You man the weapons station.[/i]")
			_frozen = true
		"engineering":
			_append_log("[i]You check the engineering console. Power distribution nominal.[/i]")
		"scanner":
			_append_log("[i]You power up the scanner array.[/i]")
		"cargo":
			_append_log("[i]You inspect the cargo manifest.[/i]")
		"cryopod":
			_append_log("[i]The cryopod is empty. Boris maintains it between jumps.[/i]")
		_:
			_append_log("[i]You interact with the %s station.[/i]" % station_id.capitalize())
	
	# Auto-release station after a moment for non-interactive stations
	if station_id not in ["pilot", "weapons"]:
		_auto_vacate(station_id, 2.0)


func _auto_vacate(station_id: String, delay: float) -> void:
	await get_tree().create_timer(delay).timeout
	ShipOperation.vacate(station_id)


## --- crew reactions ----------------------------------------------------------


func _on_crew_spoke(text: String, soul: SoulInstance) -> void:
	if text != "":
		_append_log("[b]%s:[/b] %s" % [_soul_name(soul), text])
		print("aboard: %s: %s" % [_soul_name(soul), text])


func _on_crew_concluded(outcome: String, soul: SoulInstance) -> void:
	if outcome == "abandoned":
		_append_log("[i]%s gets back to work.[/i]" % _soul_name(soul))


func _refresh() -> void:
	pass


## --- hud ---------------------------------------------------------------------


func _build_hud() -> void:
	_hud = CanvasLayer.new()
	add_child(_hud)

	var top := VBoxContainer.new()
	top.position = Vector2(16, 12)
	_hud.add_child(top)
	var title := Label.new()
	title.text = _hull.get("name", "Ship Interior")
	title.add_theme_font_size_override("font_size", 26)
	top.add_child(title)
	var sub := Label.new()
	sub.text = "walk: WASD   ·   interact: R   ·   leave station: Esc"
	sub.add_theme_font_size_override("font_size", 14)
	top.add_child(sub)

	var actions := HBoxContainer.new()
	actions.position = Vector2(16, 64)
	actions.add_theme_constant_override("separation", 8)
	_hud.add_child(actions)
	
	var launch_btn := Button.new()
	launch_btn.text = "Launch"
	launch_btn.pressed.connect(func() -> void: launch_requested.emit())
	actions.add_child(launch_btn)

	var dock_btn := Button.new()
	dock_btn.text = "Disembark"
	dock_btn.pressed.connect(func() -> void: disembark_requested.emit())
	actions.add_child(dock_btn)

	var save_btn := Button.new()
	save_btn.text = "Save"
	save_btn.pressed.connect(func() -> void:
		GameState.save_game()
		_append_log("[i]Game saved.[/i]"))
	actions.add_child(save_btn)

	_log = RichTextLabel.new()
	_log.bbcode_enabled = true
	_log.scroll_following = true
	_log.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	_log.offset_top = -100
	_log.offset_left = 12
	_log.offset_right = -12
	_log.offset_bottom = -40
	_log.custom_minimum_size = Vector2(0, 80)
	_hud.add_child(_log)

	_hint = Label.new()
	_hint.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	_hint.offset_top = -30
	_hint.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_hint.add_theme_font_size_override("font_size", 18)
	_hud.add_child(_hint)


func _soul_name(soul: SoulInstance) -> String:
	return DataRegistry.get_entity("npcs", soul.soul_id).get("name", soul.soul_id)


func _append_log(bbcode: String) -> void:
	_log.append_text(bbcode + "\n")


## --- floor rendering ---------------------------------------------------------
##
## Stand-in renderer: colored room rectangles with wall lines, door gaps,
## room name labels, and station markers. Will be replaced by Kenney tiles
## in the asset pass.

class _Floor extends Node2D:
	var _rooms: Array = []
	var _stations: Array = []
	# For rendering: wall clipping offset for doors
	var _wall_thickness := 3.0

	func setup(rooms: Array, stations: Array) -> void:
		_rooms = rooms
		_stations = stations
		queue_redraw()

	func _draw() -> void:
		for r: Dictionary in _rooms:
			var rect: Rect2 = r.rect
			var color: Color = r.color
			var doors: Array = r.get("doors", [])
			
			# Room background
			draw_rect(rect, color.darkened(0.5))
			
			# Walls with door gaps
			# Top wall
			_draw_wall(rect, "top", color.lightened(0.2), doors)
			# Bottom wall
			_draw_wall(rect, "bottom", color.lightened(0.2), doors)
			# Left wall
			_draw_wall(rect, "left", color.lightened(0.2), doors)
			# Right wall
			_draw_wall(rect, "right", color.lightened(0.2), doors)
			
			# Room name
			draw_string(ThemeDB.fallback_font,
				Vector2(rect.position.x + 8, rect.position.y + 20),
				r.name, HORIZONTAL_ALIGNMENT_LEFT, -1, 16, color.lightened(0.6))
		
		# Station markers
		for s: Dictionary in _stations:
			var station_color := Color(0.6, 0.8, 1.0, 0.8)
			match s.get("id", ""):
				"pilot": station_color = Color(0.3, 0.6, 0.9)
				"weapons": station_color = Color(0.9, 0.3, 0.3)
				"engineering": station_color = Color(0.9, 0.6, 0.2)
				"scanner": station_color = Color(0.3, 0.9, 0.6)
				"cargo": station_color = Color(0.7, 0.6, 0.3)
				"cryopod": station_color = Color(0.4, 0.7, 0.8)
			draw_circle(s.pos, 8, station_color)
			draw_circle(s.pos, 8, station_color.darkened(0.5), false, 1.5)
			draw_string(ThemeDB.fallback_font,
				Vector2(s.pos.x - 20, s.pos.y + 22),
				s.name, HORIZONTAL_ALIGNMENT_CENTER, 40, 12, Color(0.9, 0.9, 0.9))

	## Draw a wall segment with an optional door gap.
	func _draw_wall(rect: Rect2, side: String, color: Color, doors: Array) -> void:
		var door_hw := 20.0  # half-door-width
		var segments: Array = _wall_segments(rect, side, doors, door_hw)
		for seg: Dictionary in segments:
			draw_rect(seg.rect, color)

	## Calculate wall segments: the full wall minus door openings.
	func _wall_segments(rect: Rect2, side: String, doors: Array, half_width: float) -> Array:
		var result: Array = []
		var wall_rect: Rect2
		match side:
			"top":
				wall_rect = Rect2(rect.position.x, rect.position.y - _wall_thickness, rect.size.x, _wall_thickness * 2)
			"bottom":
				wall_rect = Rect2(rect.position.x, rect.position.y + rect.size.y - _wall_thickness, rect.size.x, _wall_thickness * 2)
			"left":
				wall_rect = Rect2(rect.position.x - _wall_thickness, rect.position.y, _wall_thickness * 2, rect.size.y)
			"right":
				wall_rect = Rect2(rect.position.x + rect.size.x - _wall_thickness, rect.position.y, _wall_thickness * 2, rect.size.y)
			_:
				return []
		
		# Find doors on this side and clip them out
		var gaps: Array[Rect2] = []
		for d: Dictionary in doors:
			if d.get("side", "") != side:
				continue
			var door_off: float = d.get("offset", 0.5)
			var door_w: float = d.get("width", 40.0)
			var cx: float
			var cy: float
			match side:
				"top", "bottom":
					cx = wall_rect.position.x + wall_rect.size.x * door_off
					cy = wall_rect.position.y + wall_rect.size.y * 0.5
				"left", "right":
					cx = wall_rect.position.x + wall_rect.size.x * 0.5
					cy = wall_rect.position.y + wall_rect.size.y * door_off
			var gap := Rect2(cx - door_w * 0.5, cy - _wall_thickness, door_w, _wall_thickness * 2)
			gaps.append(gap)
		
		if gaps.is_empty():
			result.append({"rect": wall_rect})
			return result
		
		# Clip gaps from the wall - split into left and right/top and bottom segments
		match side:
			"top", "bottom":
				gaps.sort_custom(func(a: Rect2, b: Rect2): return a.position.x < b.position.x)
				var start_x := wall_rect.position.x
				for g: Rect2 in gaps:
					if g.position.x > start_x:
						result.append({"rect": Rect2(start_x, wall_rect.position.y, g.position.x - start_x, wall_rect.size.y)})
					start_x = g.position.x + g.size.x
				if start_x < wall_rect.position.x + wall_rect.size.x:
					result.append({"rect": Rect2(start_x, wall_rect.position.y, wall_rect.position.x + wall_rect.size.x - start_x, wall_rect.size.y)})
			"left", "right":
				gaps.sort_custom(func(a: Rect2, b: Rect2): return a.position.y < b.position.y)
				var start_y := wall_rect.position.y
				for g: Rect2 in gaps:
					if g.position.y > start_y:
						result.append({"rect": Rect2(wall_rect.position.x, start_y, wall_rect.size.x, g.position.y - start_y)})
					start_y = g.position.y + g.size.y
				if start_y < wall_rect.position.y + wall_rect.size.y:
					result.append({"rect": Rect2(wall_rect.position.x, start_y, wall_rect.size.x, wall_rect.position.y + wall_rect.size.y - start_y)})
		
		return result


## --- player walker -----------------------------------------------------------

class _Walker extends Node2D:
	var color := Color(0.85, 0.86, 0.9)
	var facing := 1.0  # 1 = right, -1 = left
	var velocity := Vector2.ZERO  # persistent velocity for zero-G drift
	var _texture: Texture2D = null

	func _ready() -> void:
		_texture = AssetLibrary.texture("player", "character")

	func _draw() -> void:
		if _texture != null:
			var tex_size := _texture.get_size()
			var scale_factor := 48.0 / maxf(tex_size.x, tex_size.y)
			var dest := Rect2(-tex_size.x * scale_factor * 0.5, -tex_size.y * scale_factor * 0.5,
				tex_size.x * scale_factor, tex_size.y * scale_factor)
			draw_texture_rect(_texture, dest, false)
			return
		# Fallback: procedural stand-in
		draw_circle(Vector2(0, 12), 8, Color(0, 0, 0, 0.3))
		var body := Rect2(-8, -10, 16, 20)
		draw_rect(body, color)
		draw_rect(Rect2(body.position.x + body.size.x * 0.5, body.position.y,
			body.size.x * 0.5, body.size.y), color.darkened(0.3))
		draw_circle(Vector2(0, -14), 6, Color(0.86, 0.72, 0.6))
		draw_circle(Vector2(facing * 6, -14), 2, Color(0.3, 0.3, 0.3))
