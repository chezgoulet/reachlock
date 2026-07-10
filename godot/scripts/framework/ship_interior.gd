extends Node2D
## Ring 0 — ShipInterior (Sprint 3: two decks, zero-G, damage control).
##
## The walkable top-down ship. Rooms live on DECKS (hull `decks` + rooms[].deck)
## with independent gravity — the classic layout is one deck with the hull's
## gravity block. A ladder (hull `ladders`) moves the player between decks.
##
## Zero-G is a different game: momentum drift unless the character is
## mag-locked (npc `locomotion.zero_g: magnetic`, or the mag-boots upgrade
## effect). Under gravity, each character walks at their own
## `locomotion.gravity_speed_mult` — a heavy chassis crawls.
##
## Interior damage (GameState.player.ship.damage) renders where it burns:
## fire, arcing conduits, hull breaches at positions in rooms. The player
## patches them through the repair rig; crew flagged `repairs` in data seek
## damage out on their own, crossing decks by the ladder. An unrepaired
## conduit on a grav-plated deck cuts that deck's gravity until fixed.
##
## Dialogue goes through the DialoguePanel (typewriter + mind-status lamp);
## the scrolling log keeps only system notes. When an authored scene's
## speaker IS the player character, its narration card plays instead
## (npc playable.self_dialogue_summaries) and the mission beat completes.

class_name ShipInterior

signal launch_requested
signal disembark_requested

const DialogueRunnerScript := preload("res://scripts/framework/dialogue_runner.gd")

const WALK_SPEED := 200.0
const CREW_SPEED := 90.0
const INTERACT_RANGE := 48.0
const CREW_REPAIR_RATE := 0.09   # severity per second under crew hands
const AUTO_SUPPRESS_RATE := 0.025
const LADDER_CLIMB_SECONDS := 1.1

const INTERIOR_SIZE := Vector2(1280, 800)

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
	"landing_bay": Color(0.42, 0.48, 0.55),
	"ore_processing": Color(0.62, 0.50, 0.30),
}
const NEUTRAL_ROOM := Color(0.35, 0.35, 0.38)

const DAMAGE_LABELS := {"fire": "fire", "conduit": "arcing conduit", "breach": "hull breach"}

var _hull: Dictionary = {}
var _decks: Dictionary = {}     # deck id -> {name, gravity:Dictionary, rooms:Array}
var _deck_order: Array = []     # deck ids, hull order
var _deck := ""                 # the deck the player stands on
var _ladders: Array = []        # {name, positions: {deck: Vector2}}
var _rooms: Array = []          # every parsed room (all decks)
var _stations: Array = []       # {id, name, pos, deck, data}
var _interactables: Array = []

var _worlds: Dictionary = {}    # deck id -> InteriorWorld
var _walker: CharacterSprite
var _camera: Camera2D
var _pos: Vector2 = INTERIOR_SIZE * 0.5
var _drift_velocity := Vector2.ZERO
var _frozen := false
var _climbing := false
var _stranded := false          # adrift in zero-G with nothing in reach
var _stuck_time := 0.0
var _being_towed := false       # a crewmate has you; they drive
var _has_rescuer := false       # someone aboard is mag-locked
var _prop_points: Dictionary = {}  # deck -> [Vector2] — furniture you can push off
var _spawner: NpcSpawner
var _spawned: Array = []

var _hud: CanvasLayer
var _log: RichTextLabel
var _hint: Label
var _deck_label: Label
var _damage_chip: Label
var _dialogue_panel: DialoguePanel
var _runner: DialogueRunner = null
var _crew: Array = []           # crew walker records, see _place_crew
var _damage_layer: _DamageLayer


func _ready() -> void:
	_spawner = NpcSpawner.new()
	add_child(_spawner)


## Point the interior at a hull (the ship's own dictionary).
func configure(hull: Dictionary) -> void:
	_hull = hull
	_parse_decks()
	_build_rooms()
	_build_world()
	_build_hud()

	var context: String = "Aboard the %s." % hull.get("name", "ship")
	_spawned = _spawner.spawn_souls(_npc_crew_ids(), context)
	for soul in _spawned:
		soul.spoke.connect(_on_crew_spoke.bind(soul))
		soul.concluded.connect(_on_crew_concluded.bind(soul))
	_place_crew()

	ShipOperation.reset()
	_apply_deck_gravity()
	GameState.state_changed.connect(_on_state_changed)
	_refresh_damage()
	_has_rescuer = not _rescuer_candidate().is_empty()
	_maybe_self_scene.call_deferred()


## The crew as NPCs: everyone aboard except the character the player IS.
func _npc_crew_ids() -> Array:
	var ids: Array = []
	for crew_id: String in CrewRoster.aboard():
		if crew_id != GameState.player_character():
			ids.append(crew_id)
	return ids


## --- decks & rooms ---------------------------------------------------------------


func _parse_decks() -> void:
	_decks.clear()
	_deck_order.clear()
	_ladders.clear()
	var deck_data: Dictionary = _hull.get("decks", {})
	if deck_data.is_empty():
		# Single-deck hull: one anonymous deck carrying hull gravity.
		_decks[""] = {"name": "", "gravity": _hull.get("gravity",
			{"type": "energy_plate", "strength": 1.0}), "rooms": []}
		_deck_order = [""]
	else:
		var keys := deck_data.keys()
		keys.sort_custom(func(a: String, b: String) -> bool:
			return int(deck_data[a].get("order", 0)) > int(deck_data[b].get("order", 0)))
		for deck_id: String in keys:
			var entry: Dictionary = deck_data[deck_id]
			_decks[deck_id] = {
				"name": entry.get("name", deck_id.capitalize()),
				"gravity": entry.get("gravity", _hull.get("gravity", {})),
				"rooms": [],
			}
			_deck_order.append(deck_id)
	for ladder: Dictionary in _hull.get("ladders", []):
		var positions := {}
		for deck_id: String in ladder.get("positions", {}):
			var xy: Array = ladder.positions[deck_id]
			positions[deck_id] = Vector2(float(xy[0]), float(xy[1]))
		_ladders.append({"name": ladder.get("name", "Ladder"), "positions": positions})


func _default_deck() -> String:
	if _decks.has("lower"):
		return "lower"
	return _deck_order[0] if not _deck_order.is_empty() else ""


func _build_rooms() -> void:
	_rooms.clear()
	_stations.clear()
	var room_data: Array = _hull.get("rooms", [])
	var zones: Dictionary = _hull.get("room_zones", {})

	if not room_data.is_empty():
		for entry: Dictionary in room_data:
			var room_id: String = entry.get("id", "")
			var room_kind: String = entry.get("kind", room_id)
			var deck: String = entry.get("deck", _default_deck())
			if not _decks.has(deck):
				deck = _default_deck()
			var rect := Rect2(
				entry.get("x", 0.0), entry.get("y", 0.0),
				entry.get("w", 100.0), entry.get("h", 100.0))
			var color: Color = DEFAULT_ROOM_COLORS.get(room_kind, NEUTRAL_ROOM)
			var hex: String = entry.get("color", "")
			if hex != "":
				color = Color.from_string(hex, color)
			elif zones.has(room_id):
				color = Color.from_string(str(zones[room_id]), color)

			var doors: Array = []
			for d: Dictionary in entry.get("doors", []):
				var door := {
					"to": d.get("to", ""),
					"side": d.get("side", ""),
					"offset": d.get("offset", 0.5),
					"width": d.get("width", 40.0),
				}
				if door.side == "":
					door.side = _auto_door_side(rect, room_id, door.to, room_data)
				doors.append(door)

			var stations: Array = []
			for s: Dictionary in entry.get("stations", []):
				stations.append({
					"id": s.get("id", ""),
					"name": s.get("name", s.get("id", "").capitalize()),
					"pos": Vector2(
						s.get("x", rect.position.x + rect.size.x * 0.5),
						s.get("y", rect.position.y + rect.size.y * 0.5)),
					"deck": deck,
					"data": s,
				})

			var room := {
				"id": room_id,
				"name": entry.get("name", room_id.capitalize().replace("_", " ")),
				"kind": room_kind,
				"deck": deck,
				"rect": rect,
				"color": color,
				"doors": doors,
				"stations": stations,
				# Duplicated: the engine injects ladder sprites below and must
				# never mutate the DataRegistry's own arrays.
				"props": (entry.get("props", []) as Array).duplicate(),
			}
			_rooms.append(room)
			(_decks[deck].rooms as Array).append(room)
			for s: Dictionary in stations:
				_stations.append(s)
		_inject_ladder_props()
	else:
		# Fallback: grid layout from interior_rooms (legacy compat)
		var room_ids: Array = _hull.get("interior_rooms", [])
		var cols := 4
		var cell_w := INTERIOR_SIZE.x / cols
		var cell_h := 160.0
		var gap := 12.0
		var deck := _default_deck()
		for i in room_ids.size():
			var room_id: String = room_ids[i]
			var col := i % cols
			var row := i / cols
			var rect := Rect2(
				Vector2(col * cell_w + gap * 0.5, row * cell_h + gap * 0.5),
				Vector2(cell_w - gap, cell_h - gap))
			var color: Color = DEFAULT_ROOM_COLORS.get(room_id, NEUTRAL_ROOM)
			if zones.has(room_id):
				color = Color.from_string(str(zones[room_id]), color)
			var room := {
				"id": room_id,
				"name": room_id.capitalize().replace("_", " "),
				"kind": room_id,
				"deck": deck,
				"rect": rect,
				"color": color,
				"doors": [],
				"stations": [],
				"props": [],
			}
			_rooms.append(room)
			(_decks[deck].rooms as Array).append(room)


## The ladder is a place you can SEE: stamp its sprite into whichever room
## holds each end, and remember every prop position per deck — furniture
## and handrails are what an adrift body pushes off.
func _inject_ladder_props() -> void:
	for ladder: Dictionary in _ladders:
		var positions: Dictionary = ladder.positions
		for deck_id: String in positions:
			var pos: Vector2 = positions[deck_id]
			for room: Dictionary in _rooms:
				if room.deck == deck_id and (room.rect as Rect2).has_point(pos):
					(room.props as Array).append({
						"sprite": "ladder", "x": pos.x, "y": pos.y - 4})
					break
	_prop_points.clear()
	for room: Dictionary in _rooms:
		var deck: String = room.deck
		if not _prop_points.has(deck):
			_prop_points[deck] = []
		for prop: Dictionary in room.props:
			(_prop_points[deck] as Array).append(
				Vector2(prop.get("x", 0.0), prop.get("y", 0.0)))


func _find_room(room_id: String) -> Dictionary:
	for r: Dictionary in _rooms:
		if r.id == room_id:
			return r
	return {}


func _rooms_on(deck: String) -> Array:
	return _decks.get(deck, {}).get("rooms", [])


## The deck the player currently walks (contract-testable).
func current_deck() -> String:
	return _deck


## The deck's live gravity strength — a conduit fault on a grav-plated deck
## cuts its plates until someone fixes it.
func deck_gravity_strength(deck: String) -> float:
	var gravity: Dictionary = _decks.get(deck, {}).get("gravity", {})
	var strength := float(gravity.get("strength", 1.0))
	if strength > 0.1 and _deck_has_conduit_fault(deck):
		return 0.0
	return strength


func _deck_has_conduit_fault(deck: String) -> bool:
	for entry: Dictionary in GameState.ship_damage():
		if entry.get("kind", "") != "conduit":
			continue
		var room := _find_room(entry.get("room", ""))
		if room.get("deck", "") == deck:
			return true
	return false


## --- world / camera ----------------------------------------------------------


func _build_world() -> void:
	var bg := ColorRect.new()
	bg.color = Color(0.05, 0.06, 0.10)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(bg)

	for deck_id: String in _deck_order:
		var world := InteriorWorld.new()
		add_child(world)
		world.setup(_rooms_on(deck_id))
		_worlds[deck_id] = world

	_damage_layer = _DamageLayer.new()
	add_child(_damage_layer)

	_walker = CharacterSprite.new()
	var character := GameState.player_character()
	if character != "":
		var npc := DataRegistry.get_entity("npcs", character)
		_walker.setup("npcs", character, StandIn.character_color(npc, character))
	else:
		_walker.setup("player", "character", Color(0.85, 0.86, 0.9))
	var start := _player_start()
	_deck = start.deck
	_pos = start.pos
	_walker.position = _pos
	add_child(_walker)

	_camera = Camera2D.new()
	_camera.position = _walker.position
	_camera.zoom = Vector2(1.5, 1.5)
	add_child(_camera)
	_camera.make_current()

	_show_deck(_deck)


func _show_deck(deck: String) -> void:
	for deck_id: String in _worlds:
		(_worlds[deck_id] as InteriorWorld).visible = deck_id == deck
	_damage_layer.set_deck(deck)
	_refresh_crew_visibility()
	_rebuild_interactables()
	_refresh_deck_label()
	if _damage_layer != null:
		_refresh_damage()


func _refresh_deck_label() -> void:
	if _deck_label == null:
		return
	var text := str(_decks.get(_deck, {}).get("name", ""))
	if GameState.has_flag("flight_suit_on"):
		text += "   ·   FLIGHT SUIT ON"
	_deck_label.text = text


func _active_world() -> InteriorWorld:
	return _worlds.get(_deck) as InteriorWorld


func _player_start() -> Dictionary:
	for s: Dictionary in _stations:
		if s.id == "cryopod":
			return {"pos": s.pos + Vector2(0, 60), "deck": s.deck}
	if _stations.size() > 0:
		return {"pos": (_stations[0].pos as Vector2) + Vector2(0, 60),
			"deck": _stations[0].deck}
	return {"pos": INTERIOR_SIZE * 0.5, "deck": _default_deck()}


## --- movement ---------------------------------------------------------------------


## Mag-locked in zero-G? Data first (the character's own chassis), then the
## player's borrowed soles: the mag-boots upgrade, or the ship's flight suit
## (equipped at its locker; the `flight_suit_on` flag rides player.flags).
func _is_magnetic(npc_id: String) -> bool:
	if npc_id != "":
		var locomotion: Dictionary = DataRegistry.get_entity("npcs", npc_id).get("locomotion", {})
		if str(locomotion.get("zero_g", "drift")) == "magnetic":
			return true
	if npc_id == GameState.player_character():
		return GameState.upgrade_effect_bool("magnetic_soles") \
			or GameState.has_flag("flight_suit_on")
	return false


func _gravity_speed_mult(npc_id: String) -> float:
	if npc_id == "":
		return 1.0
	var locomotion: Dictionary = DataRegistry.get_entity("npcs", npc_id).get("locomotion", {})
	return float(locomotion.get("gravity_speed_mult", 1.0))


## Mag-walk speed in zero-G. A chassis built for it (Boris) walks at full
## clip; a nav chassis (Prudence) at half; borrowed soles at 0.9.
func _zero_g_speed_mult(npc_id: String) -> float:
	if npc_id != "":
		var locomotion: Dictionary = DataRegistry.get_entity("npcs", npc_id).get("locomotion", {})
		if str(locomotion.get("zero_g", "drift")) == "magnetic":
			return float(locomotion.get("zero_g_speed_mult", 1.0))
	return 0.9


func _process(delta: float) -> void:
	if not _frozen and not _climbing:
		_move_player(delta)
		if Input.is_action_just_pressed("interact"):
			_interact()
	_camera.position = _camera.position.lerp(_pos, 1.0 - exp(-6.0 * delta))
	_update_crew(delta)
	_auto_suppress(delta)
	_update_hint()


func _move_player(delta: float) -> void:
	if _being_towed:
		_walker.set_float_mode(true)
		_walker.set_motion(Vector2.ZERO, true)
		return
	var move := Input.get_vector("strafe_left", "strafe_right", "thrust_forward", "thrust_back")
	var character := GameState.player_character()
	var zero_g := deck_gravity_strength(_deck) < 0.1
	var step := Vector2.ZERO

	if zero_g and not _is_magnetic(character):
		# Adrift: thrust is only real when something is in reach to push
		# off — a wall, a handrail, a console, a crewmate. Out in the open,
		# the keys are just swimming. Think before you climb.
		var can_push := _can_push_off()
		if move != Vector2.ZERO and can_push:
			var gfx := GravitySystem.apply_movement(move, delta, _drift_velocity)
			_drift_velocity = gfx.get("move", _drift_velocity)
		# Grit steadies the body — a practiced spacer bleeds drift faster.
		var damping := 0.10 + 0.08 * float(GameState.player_stat("grit") - 2)
		_drift_velocity *= maxf(0.0, 1.0 - clampf(damping, 0.05, 0.5) * delta)
		if _drift_velocity.length() < 2.0:
			_drift_velocity = Vector2.ZERO
		step = _drift_velocity * delta
		_walker.set_float_mode(true)
		_walker.set_motion(move if move != Vector2.ZERO else _drift_velocity.normalized(),
			move != Vector2.ZERO or step.length() > 0.2)
		_update_stranded(delta, can_push)
	else:
		var speed := WALK_SPEED
		if not zero_g:
			speed *= _gravity_speed_mult(character)
		else:
			speed *= _zero_g_speed_mult(character)
		_drift_velocity = Vector2.ZERO
		_stranded = false
		_stuck_time = 0.0
		step = move * speed * delta
		_walker.set_float_mode(false)
		_walker.set_motion(move, step != Vector2.ZERO)

	if step != Vector2.ZERO:
		var moved := _try_move(_pos, step, _active_world())
		if zero_g and not _is_magnetic(character) and moved == _pos:
			_drift_velocity = Vector2.ZERO  # you hit a wall; the wall wins
		_pos = moved
		_walker.position = _pos


## Anything to push against? Walls within arm's reach, or any prop,
## station, ladder, or crew member close enough to grab.
func _can_push_off() -> bool:
	var world := _active_world()
	if world != null:
		for i in 12:
			var probe := _pos + Vector2.RIGHT.rotated(TAU * i / 12.0) * 36.0
			if not world.is_walkable(probe):
				return true
	for point: Vector2 in _prop_points.get(_deck, []):
		if _pos.distance_to(point) < 48.0:
			return true
	for member: Dictionary in _crew:
		if member.deck == _deck and _pos.distance_to(member.pos) < 44.0:
			return true
	return false


## Adrift with nothing in reach and no momentum: after a beat, whoever
## aboard is mag-locked comes to get you. If nobody can, weak swimming
## authority keeps the stranding survivable (a modded crew of all organics).
func _update_stranded(delta: float, can_push: bool) -> void:
	_stranded = _drift_velocity == Vector2.ZERO and not can_push
	if not _stranded or _being_towed or _rescue_underway():
		_stuck_time = 0.0
		return
	_stuck_time += delta
	if not _has_rescuer:
		# Nobody aboard can walk out here: flailing barely works, but works.
		var move := Input.get_vector("strafe_left", "strafe_right", "thrust_forward", "thrust_back")
		_drift_velocity += move * 14.0 * delta
		return
	if _stuck_time > 2.5:
		_begin_rescue()


func _try_move(from: Vector2, step: Vector2, world: InteriorWorld) -> Vector2:
	if world == null:
		return from
	var next := (from + step).clamp(Vector2(20, 20), INTERIOR_SIZE - Vector2(20, 20))
	if world.is_walkable(next):
		return next
	if world.is_walkable(Vector2(next.x, from.y)):
		return Vector2(next.x, from.y)
	if world.is_walkable(Vector2(from.x, next.y)):
		return Vector2(from.x, next.y)
	return from


func _auto_door_side(from_rect: Rect2, _from_id: String, to_id: String, all_rooms: Array) -> String:
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


## --- the ladder --------------------------------------------------------------------


func _use_ladder(ladder: Dictionary) -> void:
	var positions: Dictionary = ladder.positions
	var other := ""
	for deck_id: String in positions:
		if deck_id != _deck:
			other = deck_id
			break
	if other == "":
		return
	_climbing = true
	AudioManager.play("ui_switch")
	var going_up := _deck_order.find(other) < _deck_order.find(_deck)
	_append_log("[i]You %s the ladder.[/i]" % ("haul yourself up" if going_up else "climb down"))
	await get_tree().create_timer(LADDER_CLIMB_SECONDS).timeout
	_deck = other
	_pos = positions[other] + Vector2(0, 34)
	_drift_velocity = Vector2.ZERO
	_walker.position = _pos
	_camera.position = _pos
	_apply_deck_gravity()
	_show_deck(_deck)
	_climbing = false
	# The teaching beat: stepping off the ladder into zero-G without soles.
	if deck_gravity_strength(_deck) < 0.1 and not _is_magnetic(GameState.player_character()):
		_append_log("[i]Your boots leave the deck. Zero-G — you'll drift between handholds up here. The flight suit hangs below decks.[/i]")


func _apply_deck_gravity() -> void:
	var gravity: Dictionary = (_decks.get(_deck, {}).get("gravity", {}) as Dictionary).duplicate()
	gravity["strength"] = deck_gravity_strength(_deck)
	if float(gravity.get("strength", 1.0)) < 0.1:
		gravity["type"] = "none"
	GravitySystem.configure(gravity)


## --- interactables ------------------------------------------------------------------


func _rebuild_interactables() -> void:
	_interactables.clear()
	for s: Dictionary in _stations:
		if s.deck != _deck:
			continue
		_interactables.append({"kind": "station", "name": s.name, "pos": s.pos,
			"station_id": s.id, "ref": s})
	for member: Dictionary in _crew:
		if member.deck != _deck:
			continue
		_interactables.append({"kind": "crew", "name": member.name, "pos": member.pos,
			"station_id": "", "crew_id": member.id, "ref": member})
	for ladder: Dictionary in _ladders:
		if not (ladder.positions as Dictionary).has(_deck):
			continue
		_interactables.append({"kind": "ladder", "name": ladder.get("name", "Ladder"),
			"pos": ladder.positions[_deck], "station_id": "", "ref": ladder})
	for entry: Dictionary in GameState.ship_damage():
		var room := _find_room(entry.get("room", ""))
		if room.get("deck", "") != _deck:
			continue
		var pos_arr: Array = entry.get("pos", [])
		var pos := Vector2(pos_arr[0], pos_arr[1]) if pos_arr.size() == 2 else Vector2.ZERO
		_interactables.append({"kind": "damage",
			"name": "%s (%s)" % [DAMAGE_LABELS.get(entry.get("kind", ""), "damage"),
				room.get("name", "?")],
			"pos": pos, "station_id": "", "ref": entry})


func _nearest() -> Dictionary:
	var best := {}
	var best_d := INTERACT_RANGE
	for it: Dictionary in _interactables:
		# Crew walk; read their live position.
		var pos: Vector2 = it.ref.pos if it.kind == "crew" else it.pos
		var d: float = _pos.distance_to(pos)
		if d < best_d:
			best_d = d
			best = it
	return best


func _update_hint() -> void:
	if _hint == null:
		return
	if _frozen or _climbing:
		_hint.text = ""
		return
	if _being_towed:
		_hint.text = "hold on"
		return
	if _stranded:
		if _rescue_underway():
			var rescuer := _rescuer_candidate()
			_hint.text = "adrift — %s is coming. Hold on." % rescuer.get("name", "someone")
		else:
			_hint.text = "adrift — nothing close enough to push off. (This is what the flight suit was for.)"
		return
	var target := _nearest()
	var world := _active_world()
	if target.is_empty():
		_hint.text = ""
		if world != null:
			world.clear_highlight()
		return
	match target.kind:
		"crew":
			_hint.text = "R — talk to %s" % target.name
		"ladder":
			_hint.text = "R — %s" % target.name
		"damage":
			_hint.text = "R — patch the %s" % target.name
		_:
			_hint.text = "R — use %s" % target.name
	if world != null:
		world.set_highlight(target.ref.pos if target.kind == "crew" else target.pos)


func _interact() -> void:
	var target := _nearest()
	if target.is_empty():
		return
	match target.kind:
		"station":
			_use_station(target)
		"crew":
			_talk_to_crew(target.crew_id)
		"ladder":
			_use_ladder(target.ref)
		"damage":
			_open_repair_rig(target.ref)


## --- crew walkers & damage-control AI ------------------------------------------------


## Everyone aboard except the player's own character, standing at their
## assigned room. Crew flagged `repairs` in data run damage control.
func _place_crew() -> void:
	_crew.clear()
	var room_counts := {}
	for crew_id: String in _npc_crew_ids():
		var npc := DataRegistry.get_entity("npcs", crew_id)
		var room := _find_room(CrewRoster.assignment(crew_id))
		var pos: Vector2 = INTERIOR_SIZE * 0.5
		var deck := _default_deck()
		if not room.is_empty():
			var rect: Rect2 = room.rect
			var index := int(room_counts.get(room.id, 0))
			room_counts[room.id] = index + 1
			pos = rect.position + rect.size * Vector2(0.30 + 0.22 * (index % 3), 0.62)
			deck = room.get("deck", deck)
		var figure := CharacterSprite.new()
		figure.setup("npcs", crew_id, StandIn.character_color(npc, crew_id),
			npc.get("name", crew_id))
		figure.position = pos
		add_child(figure)
		_crew.append({
			"id": crew_id,
			"name": npc.get("name", crew_id),
			"pos": pos,
			"home": pos,
			"deck": deck,
			"home_deck": deck,
			"node": figure,
			"repairs": bool(npc.get("repairs", false)),
			"state": "idle",       # idle | to_ladder | climbing | to_damage | repairing | to_home
			"target": Vector2.ZERO,
			"damage_id": -1,
			"climb_left": 0.0,
		})
	_refresh_crew_visibility()
	_rebuild_interactables()


func _refresh_crew_visibility() -> void:
	for member: Dictionary in _crew:
		(member.node as CharacterSprite).visible = member.deck == _deck


func _update_crew(delta: float) -> void:
	for member: Dictionary in _crew:
		_update_crew_member(member, delta)


func _update_crew_member(member: Dictionary, delta: float) -> void:
	var figure: CharacterSprite = member.node
	var zero_g := deck_gravity_strength(member.deck) < 0.1
	figure.set_float_mode(zero_g and not _is_magnetic(member.id))

	if member.repairs and member.state in ["idle", "to_home"]:
		var job := _closest_damage_for(member)
		if not job.is_empty():
			member.damage_id = int(job.entry.id)
			if job.deck == member.deck:
				member.state = "to_damage"
				member.target = job.pos
			else:
				var ladder := _ladder_between(member.deck, job.deck)
				if not ladder.is_empty():
					member.state = "to_ladder"
					member.target = ladder.positions[member.deck]

	match member.state:
		"to_damage", "to_ladder", "to_home":
			var speed := CREW_SPEED
			if not zero_g:
				speed *= _gravity_speed_mult(member.id)
			elif _is_magnetic(member.id):
				speed *= _zero_g_speed_mult(member.id)
			else:
				speed *= 0.7  # hand-over-hand along the rails
			var to_target: Vector2 = member.target - member.pos
			if to_target.length() < 8.0:
				_crew_arrived(member)
			else:
				var step := to_target.normalized() * speed * delta
				member.pos = _try_move(member.pos, step, _worlds.get(member.deck))
				figure.position = member.pos
				figure.set_motion(to_target.normalized(), true)
		"climbing":
			member.climb_left -= delta
			if member.climb_left <= 0.0:
				var job := _damage_by_id(member.damage_id)
				var other := _other_deck(member.deck)
				var ladder := _ladder_between(member.deck, other)
				member.deck = other
				if not ladder.is_empty():
					member.pos = (ladder.positions[other] as Vector2) + Vector2(24, 30)
				figure.position = member.pos
				_refresh_crew_visibility()
				if job.is_empty():
					member.state = "to_home"
					member.target = member.home
				else:
					member.state = "to_damage"
					var pos_arr: Array = job.get("pos", [])
					member.target = Vector2(pos_arr[0], pos_arr[1]) if pos_arr.size() == 2 \
						else member.home
		"rescue_to_ladder":
			member.target = (_ladder_between(member.deck, _deck).get("positions", {}) as Dictionary) \
				.get(member.deck, member.home)
			if _crew_step(member, delta) < 8.0:
				member.state = "rescue_climbing"
				member.climb_left = LADDER_CLIMB_SECONDS
		"rescue_climbing":
			member.climb_left -= delta
			if member.climb_left <= 0.0:
				var ladder := _ladder_between(member.deck, _deck)
				member.deck = _deck
				if not ladder.is_empty():
					member.pos = (ladder.positions[_deck] as Vector2) + Vector2(24, 30)
				figure.position = member.pos
				_refresh_crew_visibility()
				member.state = "rescue_approach"
		"rescue_approach":
			if member.deck != _deck:
				# You climbed away on your own: stand down.
				_crew_go_home(member)
				return
			member.target = _pos
			if _crew_step(member, delta) < 26.0:
				member.state = "rescue_tow"
				_being_towed = true
				_drift_velocity = Vector2.ZERO
				_append_log("[i]%s clamps a hand around your arm.[/i]" % member.name)
		"rescue_tow":
			var ladder := _ladder_between(_deck, _other_deck(_deck))
			var drop: Vector2 = (ladder.get("positions", {}) as Dictionary) \
				.get(_deck, member.home) if not ladder.is_empty() else member.home
			member.target = drop + Vector2(-30, 0)
			var dist := _crew_step(member, delta)
			# You ride their grip, one pace behind.
			_pos = member.pos + Vector2(26, 4)
			_walker.position = _pos
			if dist < 12.0:
				_being_towed = false
				_stuck_time = 0.0
				_bark_from_data(member.id, "rescue_done",
					"There. The ladder. Use it — or the suit, next time.")
				var character := GameState.player_character()
				if character != "":
					CrewRoster.record_shared_event([character, member.id],
						"zero_g_retrieval", 1)
				# The rescuer remembers — and a mind that remembers, teases.
				GameState.apply_soul_mutation(member.id, {"op": "add_memory",
					"text": "Retrieved a crewmate adrift on the zero-G deck. No suit, no handhold, considerable flailing. Towed them to the ladder. Filed for future reference and future teasing.",
					"importance": 0.55, "tags": ["rescue", "zero_g", "player"]})
				_crew_go_home(member)
		"repairing":
			figure.set_motion(Vector2.ZERO, false)
			var entry := _damage_by_id(member.damage_id)
			if entry.is_empty():
				_crew_go_home(member)
				return
			entry["severity"] = float(entry.get("severity", 1.0)) - CREW_REPAIR_RATE * delta
			if float(entry.severity) <= 0.0:
				GameState.repair_ship_damage(int(entry.id))
				var room := _find_room(entry.get("room", ""))
				_append_log("[i]%s seals the %s in the %s.[/i]" % [member.name,
					DAMAGE_LABELS.get(entry.get("kind", ""), "damage"),
					room.get("name", "?")])
				_crew_go_home(member)
		_:
			figure.set_motion(Vector2.ZERO, false)


func _crew_arrived(member: Dictionary) -> void:
	match member.state:
		"to_ladder":
			member.state = "climbing"
			member.climb_left = LADDER_CLIMB_SECONDS * (1.0 if _is_magnetic(member.id)
				else _gravity_speed_mult(member.id))
		"to_damage":
			if _damage_by_id(member.damage_id).is_empty():
				_crew_go_home(member)
			else:
				member.state = "repairing"
		"to_home":
			member.state = "idle"
			(member.node as CharacterSprite).set_motion(Vector2.ZERO, false)


## --- the zero-G rescue ---------------------------------------------------------
## You floated somewhere with nothing in reach. A mag-locked crewmate walks
## out, clamps on, and tows you to the ladder — then files it under humor
## (npc `barks`: rescue_start / rescue_done).


func _rescue_underway() -> bool:
	for member: Dictionary in _crew:
		if str(member.state).begins_with("rescue"):
			return true
	return false


## The best-built rescuer aboard: mag-locked, fastest in zero-G.
func _rescuer_candidate() -> Dictionary:
	var best := {}
	var best_speed := 0.0
	for member: Dictionary in _crew:
		if not _is_magnetic(member.id):
			continue
		var speed := _zero_g_speed_mult(member.id)
		if speed > best_speed:
			best_speed = speed
			best = member
	return best


func _begin_rescue() -> void:
	var member := _rescuer_candidate()
	if member.is_empty():
		return
	member.damage_id = -1
	_bark_from_data(member.id, "rescue_start", "Hold on. I'm coming to you.")
	if member.deck == _deck:
		member.state = "rescue_approach"
	else:
		var ladder := _ladder_between(member.deck, _deck)
		if ladder.is_empty():
			return
		member.state = "rescue_to_ladder"
		member.target = ladder.positions[member.deck]


func _bark_from_data(npc_id: String, key: String, fallback: String) -> void:
	var lines: Array = DataRegistry.get_entity("npcs", npc_id).get("barks", {}).get(key, [])
	var text: String = lines[randi() % lines.size()] if not lines.is_empty() else fallback
	_dialogue_panel.bark(_crew_name(npc_id), text)


## One frame of walking toward member.target at the member's own speed for
## the deck's gravity regime. Returns the remaining distance.
func _crew_step(member: Dictionary, delta: float) -> float:
	var figure: CharacterSprite = member.node
	var zero_g := deck_gravity_strength(member.deck) < 0.1
	var speed := CREW_SPEED
	if not zero_g:
		speed *= _gravity_speed_mult(member.id)
	elif _is_magnetic(member.id):
		speed *= _zero_g_speed_mult(member.id)
	else:
		speed *= 0.7
	var to_target: Vector2 = member.target - member.pos
	if to_target.length() > 4.0:
		member.pos = _try_move(member.pos, to_target.normalized() * speed * delta,
			_worlds.get(member.deck))
		figure.position = member.pos
		figure.set_motion(to_target.normalized(), true)
	return (member.target - member.pos).length()


func _crew_go_home(member: Dictionary) -> void:
	member.damage_id = -1
	if member.deck == member.home_deck:
		member.state = "to_home"
		member.target = member.home
	else:
		var ladder := _ladder_between(member.deck, member.home_deck)
		if ladder.is_empty():
			member.state = "idle"
		else:
			member.state = "to_ladder"
			member.target = ladder.positions[member.deck]


func _closest_damage_for(member: Dictionary) -> Dictionary:
	# Same-deck damage first; a cross-deck job only for crew who can get there.
	var best := {}
	var best_d := INF
	var claimed := _claimed_damage_ids(member)
	for entry: Dictionary in GameState.ship_damage():
		if int(entry.get("id", -1)) in claimed:
			continue
		var room := _find_room(entry.get("room", ""))
		var deck: String = room.get("deck", "")
		var pos_arr: Array = entry.get("pos", [])
		if pos_arr.size() != 2:
			continue
		var pos := Vector2(pos_arr[0], pos_arr[1])
		var d: float = (member.pos as Vector2).distance_to(pos) \
			+ (900.0 if deck != member.deck else 0.0)
		if deck != member.deck and _ladder_between(member.deck, deck).is_empty():
			continue
		if d < best_d:
			best_d = d
			best = {"entry": entry, "pos": pos, "deck": deck}
	return best


func _claimed_damage_ids(except_member: Dictionary) -> Array:
	var ids: Array = []
	for member: Dictionary in _crew:
		if member != except_member and int(member.damage_id) >= 0:
			ids.append(int(member.damage_id))
	return ids


func _damage_by_id(damage_id: int) -> Dictionary:
	for entry: Dictionary in GameState.ship_damage():
		if int(entry.get("id", -1)) == damage_id:
			return entry
	return {}


func _ladder_between(a: String, b: String) -> Dictionary:
	for ladder: Dictionary in _ladders:
		var positions: Dictionary = ladder.positions
		if positions.has(a) and positions.has(b):
			return ladder
	return {}


func _other_deck(deck: String) -> String:
	for deck_id: String in _deck_order:
		if deck_id != deck:
			return deck_id
	return deck


## --- damage rendering, repair, suppression --------------------------------------------


func _on_state_changed() -> void:
	_refresh_damage()
	_apply_deck_gravity()
	_refresh_deck_label()


func _refresh_damage() -> void:
	if _damage_layer == null:
		return
	var entries: Array = []
	for entry: Dictionary in GameState.ship_damage():
		var room := _find_room(entry.get("room", ""))
		entries.append({"entry": entry, "deck": room.get("deck", "")})
	_damage_layer.set_entries(entries)
	_rebuild_interactables()
	if _damage_chip != null:
		var count := GameState.ship_damage().size()
		_damage_chip.visible = count > 0
		_damage_chip.text = "⚠ SHIP DAMAGE ×%d" % count


## Fires burn themselves down when the suppression net is fitted.
func _auto_suppress(delta: float) -> void:
	if not GameState.upgrade_effect_bool("auto_suppress"):
		return
	for entry: Dictionary in GameState.ship_damage():
		if entry.get("kind", "") != "fire":
			continue
		entry["severity"] = float(entry.get("severity", 1.0)) - AUTO_SUPPRESS_RATE * delta
		if float(entry.severity) <= 0.0:
			var room := _find_room(entry.get("room", ""))
			_append_log("[i]The suppression net smothers the fire in the %s.[/i]"
				% room.get("name", "?"))
			GameState.repair_ship_damage(int(entry.id))
			return  # list mutated; next frame handles the rest


func _open_repair_rig(entry: Dictionary) -> void:
	var panel := StationMinigames.repair_rig(entry)
	if panel == null:
		return
	AudioManager.play("computer_noise", 0.9)
	_frozen = true
	_hud.add_child(panel)
	panel.connect("closed", func() -> void: _frozen = false)


## --- talking ---------------------------------------------------------------------


func _talk_to_crew(crew_id: String) -> void:
	if _runner != null:
		return
	var soul := _spawner.get_spawned(crew_id)
	var dialogue := _find_dialogue_for(crew_id)
	if not dialogue.is_empty():
		_runner = DialogueRunnerScript.new()
		add_child(_runner)
		_runner.line_shown.connect(_dialogue_panel.show_line)
		_runner.choices_shown.connect(_dialogue_panel.show_choices)
		_runner.thinking_changed.connect(_dialogue_panel.set_thinking)
		_runner.ended.connect(_on_dialogue_ended)
		_dialogue_panel.open(_crew_name(crew_id), _link_state_for(soul))
		if _runner.start(dialogue, soul):
			_frozen = true
			return
		_dialogue_panel.close()
		_runner.queue_free()
		_runner = null
	if soul != null and SoulGateway.is_ready():
		_dialogue_panel.bark("You", "Got a minute?")
		soul.perceive_utterance("player", "Got a minute?")
	else:
		_dialogue_panel.bark(_crew_name(crew_id), "*gives you a nod and keeps working*")


func _link_state_for(soul: SoulInstance) -> String:
	if soul == null:
		return "scripted"
	return "linked" if SoulGateway.is_ready() else "offline"


func _find_dialogue_for(soul_id: String) -> Dictionary:
	var context := GameState.context()
	for dialogue_id in DataRegistry.ids("dialogues"):
		var dialogue := DataRegistry.get_entity("dialogues", dialogue_id)
		if dialogue.get("npc", "") != soul_id:
			continue
		var guard: String = dialogue.get("condition", "")
		if guard == "" or TriggerDSL.evaluate(guard, context):
			return dialogue
	return {}


func _on_dialogue_ended() -> void:
	_frozen = false
	_dialogue_panel.close()
	if _runner != null:
		var npc_id := _runner.npc_id()
		MemoryStore.ingest_conversation(npc_id, _runner.transcript(), {
			"tick": GameState.universe.tick, "location": _hull.get("id", ""),
		})
		_runner.queue_free()
		_runner = null
		MissionManager.report_event("dialogue_end", {"npc_id": npc_id})
		_maybe_self_scene.call_deferred()


## A scene whose speaker IS the player can't play as a conversation: its
## narration card (playable.self_dialogue_summaries) shows instead, applies
## the scene's story mutations, and completes the mission beat.
func _maybe_self_scene() -> void:
	if _runner != null or _dialogue_panel.is_open():
		return
	var character := GameState.player_character()
	if character == "":
		return
	var summaries: Dictionary = DataRegistry.get_entity("npcs", character) \
		.get("playable", {}).get("self_dialogue_summaries", {})
	var context := GameState.context()
	for dialogue_id in DataRegistry.ids("dialogues"):
		if not summaries.has(dialogue_id):
			continue
		var dialogue := DataRegistry.get_entity("dialogues", dialogue_id)
		if dialogue.get("npc", "") != character:
			continue
		var guard: String = dialogue.get("condition", "")
		if guard != "" and not TriggerDSL.evaluate(guard, context):
			continue
		var card: Dictionary = summaries[dialogue_id]
		_frozen = true
		for mutation: Dictionary in card.get("mutations", []):
			GameState.apply_soul_mutation(character, mutation)
		# The beat completes NOW (mutations + mission event); dismissing the
		# card only hands back control — a save mid-card can't strand a stage.
		MissionManager.report_event("dialogue_end", {"npc_id": character})
		_dialogue_panel.show_narration(_crew_name(character), card.get("text", ""))
		_dialogue_panel.narration_done.connect(func() -> void:
			_frozen = false,
			CONNECT_ONE_SHOT)
		return


func _crew_name(crew_id: String) -> String:
	return DataRegistry.get_entity("npcs", crew_id).get("name", crew_id)


func _use_station(target: Dictionary) -> void:
	var station_id: String = target.station_id
	if station_id == "":
		return

	ShipOperation.occupy(station_id, "player")

	# The pilot's seat IS the flight mode: sitting down takes her out.
	if station_id == "pilot":
		AudioManager.play("ui_switch")
		_append_log("[i]You take the pilot's seat.[/i]")
		launch_requested.emit()
		return

	var panel := StationMinigames.build(station_id, target.get("ref", {}).get("data", {}))
	if panel != null:
		AudioManager.play("computer_noise", 0.9)
		_frozen = true
		_hud.add_child(panel)
		panel.connect("closed", func() -> void:
			_frozen = false
			ShipOperation.vacate(station_id))
		return

	match station_id:
		"cryopod":
			AudioManager.play("force_field", 0.7)
			_append_log("[i]The pods idle at standby chill. The next crossing, you'll be inside one.[/i]")
		"flight_suit":
			if GameState.has_flag("flight_suit_on"):
				GameState.clear_flag("flight_suit_on")
				_append_log("[i]You rack the flight suit.[/i]")
			else:
				GameState.set_flag("flight_suit_on")
				AudioManager.play("force_field", 0.6)
				_append_log("[i]You pull on the flight suit. The mag-soles hum against the deck — the zero-G level is walkable now.[/i]")
		_:
			_append_log("[i]You interact with the %s station.[/i]" % station_id.capitalize())
	_auto_vacate(station_id, 2.0)


func _auto_vacate(station_id: String, delay: float) -> void:
	await get_tree().create_timer(delay).timeout
	ShipOperation.vacate(station_id)


## --- crew reactions ----------------------------------------------------------


func _on_crew_spoke(text: String, soul: SoulInstance) -> void:
	if text != "":
		if _runner == null:
			_dialogue_panel.bark(_soul_name(soul), text)
		print("aboard: %s: %s" % [_soul_name(soul), text])


func _on_crew_concluded(outcome: String, soul: SoulInstance) -> void:
	if outcome == "abandoned" and _runner == null:
		_dialogue_panel.bark(_soul_name(soul), "*gets back to work*")


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
	_deck_label = Label.new()
	_deck_label.text = str(_decks.get(_deck, {}).get("name", ""))
	_deck_label.add_theme_font_size_override("font_size", 16)
	_deck_label.add_theme_color_override("font_color", Color(0.75, 0.82, 0.95))
	top.add_child(_deck_label)
	var sub := Label.new()
	sub.text = "walk: WASD   ·   interact: R   ·   close panel: Esc"
	sub.add_theme_font_size_override("font_size", 14)
	top.add_child(sub)

	_damage_chip = Label.new()
	_damage_chip.add_theme_font_size_override("font_size", 16)
	_damage_chip.add_theme_color_override("font_color", Color(1.0, 0.45, 0.35))
	_damage_chip.set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT)
	_damage_chip.offset_left = -230
	_damage_chip.offset_top = 14
	_damage_chip.visible = false
	_hud.add_child(_damage_chip)

	var actions := HBoxContainer.new()
	actions.position = Vector2(16, 96)
	actions.add_theme_constant_override("separation", 8)
	_hud.add_child(actions)

	var launch_btn := Button.new()
	launch_btn.text = "Launch"
	launch_btn.pressed.connect(func() -> void: launch_requested.emit())
	actions.add_child(launch_btn)

	if GameState.is_docked():
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

	# System notes only — conversation lives on the DialoguePanel.
	_log = RichTextLabel.new()
	_log.bbcode_enabled = true
	_log.scroll_following = true
	_log.set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT)
	_log.offset_left = -420
	_log.offset_right = -12
	_log.offset_top = 44
	_log.offset_bottom = 160
	_log.add_theme_font_size_override("normal_font_size", 13)
	_log.modulate = Color(1, 1, 1, 0.85)
	_hud.add_child(_log)

	_dialogue_panel = DialoguePanel.new()
	_dialogue_panel.choice_picked.connect(func(index: int) -> void:
		if _runner != null:
			_runner.choose(index))
	_dialogue_panel.free_speech.connect(func(text: String) -> void:
		if _runner != null:
			_runner.speak_freely(text))
	_hud.add_child(_dialogue_panel)

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


## Draws every unrepaired damage entry on the active deck: flickering fire,
## arcing conduits, breach holes — plus a scorch under fires. Textures come
## through AssetLibrary like every other prop.
class _DamageLayer extends Node2D:
	var _entries: Array = []  # {entry:Dictionary, deck:String}
	var _deck := ""
	var _clock := 0.0
	var _textures: Dictionary = {}

	func set_entries(entries: Array) -> void:
		_entries = entries
		queue_redraw()

	func set_deck(deck: String) -> void:
		_deck = deck
		queue_redraw()

	func _texture(sprite: String) -> Texture2D:
		if not _textures.has(sprite):
			_textures[sprite] = AssetLibrary.texture("props", sprite)
		return _textures[sprite]

	func _process(delta: float) -> void:
		_clock += delta
		queue_redraw()

	func _draw() -> void:
		var flicker := int(_clock * 7.0) % 2 == 0
		for item: Dictionary in _entries:
			if item.deck != _deck:
				continue
			var entry: Dictionary = item.entry
			var pos_arr: Array = entry.get("pos", [])
			if pos_arr.size() != 2:
				continue
			var pos := Vector2(pos_arr[0], pos_arr[1])
			var severity := float(entry.get("severity", 1.0))
			match entry.get("kind", ""):
				"fire":
					_stamp(pos + Vector2(0, 10), "scorch_mark", 2.0, 1.0)
					_stamp(pos, "damage_fire_a" if flicker else "damage_fire_b",
						1.4 + severity, 1.0)
					var glow := Color(1.0, 0.55, 0.2, 0.10 + 0.08 * sin(_clock * 9.0))
					draw_circle(pos, 34.0 + severity * 14.0, glow)
				"conduit":
					_stamp(pos, "damage_sparks_a" if flicker else "damage_sparks_b", 2.0, 1.0)
					if flicker:
						draw_circle(pos, 20.0, Color(0.7, 0.85, 1.0, 0.12))
				"breach":
					_stamp(pos, "damage_breach", 2.0, 1.0)
					var pull := 0.5 + 0.5 * sin(_clock * 3.0)
					draw_arc(pos, 16.0 + pull * 5.0, 0, TAU, 24,
						Color(0.8, 0.9, 1.0, 0.25), 1.5)

	func _stamp(pos: Vector2, sprite: String, scale_mult: float, alpha: float) -> void:
		var tex := _texture(sprite)
		if tex == null:
			draw_rect(Rect2(pos - Vector2(10, 8), Vector2(20, 16)),
				Color(0.9, 0.3, 0.2, 0.6))
			return
		var size := tex.get_size() * scale_mult
		draw_texture_rect(tex, Rect2(pos - size * 0.5, size), false,
			Color(1, 1, 1, alpha))
