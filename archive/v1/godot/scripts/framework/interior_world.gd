extends Node2D
## Ring 0 — InteriorWorld: the shared renderer + walkability model for every
## walkable interior (the ship, station concourses). Give it parsed rooms —
## [{id, name, kind, rect:Rect2, color:Color, doors:[{to, side, offset,
## width}], props:[{sprite, x, y, scale?, condition?}]}] — and it draws a
## tiled floor per room kind, walls with door gaps, furniture props, and
## room names; and it answers is_walkable() so hosts can slide the player
## along walls and through doorways.
##
## Props may carry a trigger-DSL `condition`: set dressing that appears or
## vanishes with story state (the Interval bar before and after the fight).
## Conditions re-evaluate on GameState.state_changed.
##
## Art comes through AssetLibrary (assets/tiles/<name>.png,
## assets/props/<name>.png) with flat-color fallbacks, so the scene runs
## with or without the art pass.

class_name InteriorWorld

const TriggerDSLScript := preload("res://scripts/framework/trigger_dsl.gd")

const WALL_T := 3.0          # wall half-thickness (matches classic _Floor)
const DOOR_BRIDGE_PAD := 8.0 # how far a door bridge reaches into each room
const WALK_MARGIN := 4.0     # keep feet off the walls
const PROP_SCALE := 2.0

## room kind -> floor tile (assets/tiles). Fallback: floor_deck.
const TILE_BY_KIND := {
	"cockpit": "floor_deck", "bridge": "floor_deck", "airlock": "floor_grate",
	"engineering": "floor_grate", "med_bay": "floor_med", "galley": "floor_galley",
	"crew_quarters": "floor_quarters", "cargo_hold": "floor_cargo",
	"cryo": "floor_cryo", "bar": "floor_bar", "office": "floor_office",
	"corridor": "floor_deck", "market": "floor_deck",
	"landing_bay": "floor_grate", "ore_processing": "floor_cargo",
}
const TILE_SIZE := 32.0  # 16px tiles drawn at 2x

var _rooms: Array = []
var _walkable: Array[Rect2] = []
var _tiles: Dictionary = {}   # tile name -> Texture2D or null
var _props: Dictionary = {}   # sprite name -> Texture2D or null
var _visible_props: Array = []  # {tex, sprite, pos:Vector2, scale}
var _highlight: Vector2 = Vector2.INF
var _clock := 0.0


func setup(rooms: Array) -> void:
	_rooms = rooms
	_build_walkable()
	_load_art()
	_refresh_props()
	GameState.state_changed.connect(_refresh_props)


func _process(delta: float) -> void:
	_clock += delta
	queue_redraw()


## The affordance glow: hosts point it at the nearest interactable.
func set_highlight(pos: Vector2) -> void:
	_highlight = pos


func clear_highlight() -> void:
	_highlight = Vector2.INF


## --- walkability ---------------------------------------------------------------


func is_walkable(p: Vector2) -> bool:
	for r: Rect2 in _walkable:
		if r.has_point(p):
			return true
	return false


func _build_walkable() -> void:
	_walkable.clear()
	for room: Dictionary in _rooms:
		_walkable.append((room.rect as Rect2).grow(-WALK_MARGIN))
	# Door bridges: a walkable strip spanning the two walls (and any hull
	# gap between adjacent room rects).
	for room: Dictionary in _rooms:
		var rect: Rect2 = room.rect
		for door: Dictionary in room.get("doors", []):
			var other := _room_rect(door.get("to", ""))
			if other == Rect2():
				continue
			var offset: float = door.get("offset", 0.5)
			var width: float = door.get("width", 40.0)
			match door.get("side", ""):
				"right":
					var cy := rect.position.y + rect.size.y * offset
					_walkable.append(Rect2(rect.end.x - DOOR_BRIDGE_PAD, cy - width * 0.5,
						(other.position.x - rect.end.x) + DOOR_BRIDGE_PAD * 2, width))
				"left":
					var cy2 := rect.position.y + rect.size.y * offset
					_walkable.append(Rect2(other.end.x - DOOR_BRIDGE_PAD, cy2 - width * 0.5,
						(rect.position.x - other.end.x) + DOOR_BRIDGE_PAD * 2, width))
				"bottom":
					var cx := rect.position.x + rect.size.x * offset
					_walkable.append(Rect2(cx - width * 0.5, rect.end.y - DOOR_BRIDGE_PAD,
						width, (other.position.y - rect.end.y) + DOOR_BRIDGE_PAD * 2))
				"top":
					var cx2 := rect.position.x + rect.size.x * offset
					_walkable.append(Rect2(cx2 - width * 0.5, other.end.y - DOOR_BRIDGE_PAD,
						width, (rect.position.y - other.end.y) + DOOR_BRIDGE_PAD * 2))


func _room_rect(room_id: String) -> Rect2:
	for room: Dictionary in _rooms:
		if room.id == room_id:
			return room.rect
	return Rect2()


## --- art -----------------------------------------------------------------------


func _load_art() -> void:
	for room: Dictionary in _rooms:
		var tile: String = TILE_BY_KIND.get(room.get("kind", ""), "floor_deck")
		if not _tiles.has(tile):
			_tiles[tile] = AssetLibrary.texture("tiles", tile)
	if not _tiles.has("wall"):
		_tiles["wall"] = AssetLibrary.texture("tiles", "wall")


func _refresh_props() -> void:
	_visible_props.clear()
	var context := GameState.context()
	for room: Dictionary in _rooms:
		for prop: Dictionary in room.get("props", []):
			var condition: String = prop.get("condition", "")
			if condition != "" and not TriggerDSLScript.evaluate(condition, context):
				continue
			var sprite: String = prop.get("sprite", "")
			if not _props.has(sprite):
				_props[sprite] = AssetLibrary.texture("props", sprite)
			_visible_props.append({
				"tex": _props[sprite], "sprite": sprite,
				"pos": Vector2(prop.get("x", 0.0), prop.get("y", 0.0)),
				"scale": float(prop.get("scale", 1.0)) * PROP_SCALE,
			})
	queue_redraw()


## --- drawing --------------------------------------------------------------------


func _draw() -> void:
	for room: Dictionary in _rooms:
		_draw_room_floor(room)
	for room: Dictionary in _rooms:
		var doors: Array = room.get("doors", [])
		var color: Color = room.color
		for side in ["top", "bottom", "left", "right"]:
			_draw_wall(room.rect, side, color.lightened(0.15), doors)
	for entry: Dictionary in _visible_props:
		_draw_prop(entry)
	for room: Dictionary in _rooms:
		var rect: Rect2 = room.rect
		draw_string(ThemeDB.fallback_font,
			Vector2(rect.position.x + 8, rect.position.y + 18),
			room.name, HORIZONTAL_ALIGNMENT_LEFT, -1, 13,
			(room.color as Color).lightened(0.55))
	if _highlight != Vector2.INF:
		var pulse := 0.5 + 0.5 * sin(_clock * 5.0)
		draw_arc(_highlight, 16.0 + pulse * 3.0, 0, TAU, 24,
			Color(1.0, 0.9, 0.55, 0.55 + pulse * 0.3), 2.0)


func _draw_room_floor(room: Dictionary) -> void:
	var rect: Rect2 = room.rect
	var tile_name: String = TILE_BY_KIND.get(room.get("kind", ""), "floor_deck")
	var tex: Texture2D = _tiles.get(tile_name)
	if tex != null:
		# Tile the room by hand so partial edge tiles clip to the rect.
		var y := rect.position.y
		while y < rect.end.y:
			var x := rect.position.x
			var h := minf(TILE_SIZE, rect.end.y - y)
			while x < rect.end.x:
				var w := minf(TILE_SIZE, rect.end.x - x)
				draw_texture_rect_region(tex, Rect2(x, y, w, h),
					Rect2(0, 0, w / 2.0, h / 2.0))
				x += TILE_SIZE
			y += TILE_SIZE
	else:
		draw_rect(rect, (room.color as Color).darkened(0.5))
	# A whisper of the room's identity color over the tiles.
	var tint: Color = room.color
	tint.a = 0.10
	draw_rect(rect, tint)


func _draw_prop(entry: Dictionary) -> void:
	var tex: Texture2D = entry.tex
	var pos: Vector2 = entry.pos
	if tex == null:
		draw_rect(Rect2(pos - Vector2(10, 8), Vector2(20, 16)), Color(0.5, 0.5, 0.55))
		return
	var size := tex.get_size() * float(entry.scale)
	draw_texture_rect(tex, Rect2(pos - size * 0.5, size), false)


## Draw a wall segment with door gaps (classic _Floor logic, kept exact).
func _draw_wall(rect: Rect2, side: String, color: Color, doors: Array) -> void:
	for seg: Dictionary in _wall_segments(rect, side, doors):
		draw_rect(seg.rect, color)


func _wall_segments(rect: Rect2, side: String, doors: Array) -> Array:
	var result: Array = []
	var wall_rect: Rect2
	match side:
		"top":
			wall_rect = Rect2(rect.position.x, rect.position.y - WALL_T, rect.size.x, WALL_T * 2)
		"bottom":
			wall_rect = Rect2(rect.position.x, rect.position.y + rect.size.y - WALL_T, rect.size.x, WALL_T * 2)
		"left":
			wall_rect = Rect2(rect.position.x - WALL_T, rect.position.y, WALL_T * 2, rect.size.y)
		"right":
			wall_rect = Rect2(rect.position.x + rect.size.x - WALL_T, rect.position.y, WALL_T * 2, rect.size.y)
		_:
			return []

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
		gaps.append(Rect2(cx - door_w * 0.5, cy - WALL_T, door_w, WALL_T * 2))

	if gaps.is_empty():
		result.append({"rect": wall_rect})
		return result

	match side:
		"top", "bottom":
			gaps.sort_custom(func(a: Rect2, b: Rect2) -> bool: return a.position.x < b.position.x)
			var start_x := wall_rect.position.x
			for g: Rect2 in gaps:
				if g.position.x > start_x:
					result.append({"rect": Rect2(start_x, wall_rect.position.y, g.position.x - start_x, wall_rect.size.y)})
				start_x = g.position.x + g.size.x
			if start_x < wall_rect.position.x + wall_rect.size.x:
				result.append({"rect": Rect2(start_x, wall_rect.position.y, wall_rect.position.x + wall_rect.size.x - start_x, wall_rect.size.y)})
		"left", "right":
			gaps.sort_custom(func(a: Rect2, b: Rect2) -> bool: return a.position.y < b.position.y)
			var start_y := wall_rect.position.y
			for g: Rect2 in gaps:
				if g.position.y > start_y:
					result.append({"rect": Rect2(wall_rect.position.x, start_y, wall_rect.size.x, g.position.y - start_y)})
				start_y = g.position.y + g.size.y
			if start_y < wall_rect.position.y + wall_rect.size.y:
				result.append({"rect": Rect2(wall_rect.position.x, start_y, wall_rect.size.x, wall_rect.position.y + wall_rect.size.y - start_y)})

	return result
