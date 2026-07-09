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

const DialogueRunnerScript := preload("res://scripts/framework/dialogue_runner.gd")

const WALK_SPEED := 200.0
const INTERACT_RANGE := 48.0
const CREW_INTERACT_RANGE := 56.0

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

var _world: InteriorWorld
var _walker: CharacterSprite
var _camera: Camera2D
var _pos: Vector2 = INTERIOR_SIZE * 0.5
var _frozen := false
var _spawner: NpcSpawner
var _spawned: Array = []

var _hud: CanvasLayer
var _log: RichTextLabel
var _hint: Label
var _choice_box: VBoxContainer
var _thinking_label: Label = null
var _runner: DialogueRunner = null
var _crew: Array = []  # {id, name, pos:Vector2, node}


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
	_place_crew()

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
			var hex: String = entry.get("color", "")
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
				"props": entry.get("props", []),
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
	for member: Dictionary in _crew:
		_interactables.append({
			"kind": "crew",
			"name": member.name,
			"pos": member.pos,
			"station_id": "",
			"crew_id": member.id,
			"ref": member,
		})


## Put each aboard crew member in their assigned room as a visible,
## talkable figure. Data decides who is aboard and where they stand.
func _place_crew() -> void:
	_crew.clear()
	var room_counts := {}
	for crew_id: String in CrewRoster.aboard():
		var npc := DataRegistry.get_entity("npcs", crew_id)
		var room := _find_room(CrewRoster.assignment(crew_id))
		var pos: Vector2 = INTERIOR_SIZE * 0.5
		if not room.is_empty():
			var rect: Rect2 = room.rect
			var index := int(room_counts.get(room.id, 0))
			room_counts[room.id] = index + 1
			# Fan out crew sharing a room; keep clear of the station marker.
			pos = rect.position + rect.size * Vector2(0.30 + 0.22 * (index % 3), 0.62)
		var figure := CharacterSprite.new()
		figure.setup("npcs", crew_id, StandIn.character_color(npc, crew_id),
			npc.get("name", crew_id))
		figure.position = pos
		add_child(figure)
		_crew.append({
			"id": crew_id,
			"name": npc.get("name", crew_id),
			"pos": pos,
			"node": figure,
		})
	_rebuild_interactables()


func _find_room(room_id: String) -> Dictionary:
	for r: Dictionary in _rooms:
		if r.id == room_id:
			return r
	return {}


## --- world / camera ----------------------------------------------------------


func _build_world() -> void:
	# Background sky — dark space through the hull
	var bg := ColorRect.new()
	bg.color = Color(0.05, 0.06, 0.10)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(bg)

	# The shared interior renderer: tiled floors, walls, furniture,
	# walkability. Rooms and props are content; this is presentation.
	_world = InteriorWorld.new()
	add_child(_world)
	_world.setup(_rooms)

	# Player walker: the shared character sheet renderer.
	_walker = CharacterSprite.new()
	_walker.setup("player", "character", Color(0.85, 0.86, 0.9))
	_pos = _player_start()
	_walker.position = _pos
	add_child(_walker)

	# Camera
	_camera = Camera2D.new()
	_camera.position = _walker.position
	_camera.zoom = Vector2(1.5, 1.5)
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


var _drift_velocity := Vector2.ZERO  # zero-G residual motion

func _process(delta: float) -> void:
	if not _frozen:
		var move := Input.get_vector("strafe_left", "strafe_right", "thrust_forward", "thrust_back")

		# Gravity-aware movement
		var gfx := GravitySystem.apply_movement(move, delta, _drift_velocity)
		_drift_velocity = gfx.get("velocity", Vector2.ZERO)

		var effective_move: Vector2 = gfx.get("move", move)
		var effective_delta: float = gfx.get("delta", delta)

		var step := Vector2.ZERO
		if effective_move != Vector2.ZERO:
			step = effective_move * WALK_SPEED * effective_delta
		elif _drift_velocity != Vector2.ZERO:
			step = _drift_velocity * effective_delta
		if step != Vector2.ZERO:
			_try_move(step)
		_walker.set_motion(move, step != Vector2.ZERO and move != Vector2.ZERO)

		_camera.position = _camera.position.lerp(_pos, 1.0 - exp(-6.0 * delta))
		if Input.is_action_just_pressed("interact"):
			_interact()
	_update_hint()


## Walls are real: move where the interior allows, slide along what it
## doesn't, and never leave the hull.
func _try_move(step: Vector2) -> void:
	var next := (_pos + step).clamp(Vector2(20, 20), INTERIOR_SIZE - Vector2(20, 20))
	if _world.is_walkable(next):
		_pos = next
	elif _world.is_walkable(Vector2(next.x, _pos.y)):
		_pos.x = next.x
	elif _world.is_walkable(Vector2(_pos.x, next.y)):
		_pos.y = next.y
	_walker.position = _pos


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
		if _world != null:
			_world.clear_highlight()
		return
	if target.kind == "crew":
		_hint.text = "R — talk to %s" % target.name
	else:
		_hint.text = "R — use %s" % target.name
	if _world != null:
		_world.set_highlight(target.pos)


func _interact() -> void:
	var target := _nearest()
	if target.is_empty():
		return
	if target.kind == "station":
		_use_station(target)
	elif target.kind == "crew":
		_talk_to_crew(target.crew_id)


## --- talking (authored dialogue first, mind-carried exchange otherwise) ------


func _talk_to_crew(crew_id: String) -> void:
	if _runner != null:
		return
	var soul := _spawner.get_spawned(crew_id)
	var dialogue := _find_dialogue_for(crew_id)
	if not dialogue.is_empty():
		_runner = DialogueRunnerScript.new()
		add_child(_runner)
		_runner.line_shown.connect(_on_line_shown)
		_runner.choices_shown.connect(_on_choices_shown)
		_runner.thinking_changed.connect(func(thinking: bool) -> void:
			if _thinking_label != null:
				_thinking_label.visible = thinking)
		_runner.ended.connect(_on_dialogue_ended)
		if _runner.start(dialogue, soul):
			return
		_runner.queue_free()
		_runner = null
	if soul != null and SoulGateway.is_ready():
		_append_log("[i]You catch %s's attention.[/i]" % _crew_name(crew_id))
		soul.perceive_utterance("player", "Got a minute?")
	else:
		_append_log("[i]%s gives you a nod and keeps working.[/i]" % _crew_name(crew_id))


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


func _on_line_shown(speaker: String, text: String) -> void:
	_append_log("[b]%s:[/b] %s" % [speaker, text])


func _on_choices_shown(choices: Array) -> void:
	for choice: Dictionary in choices:
		var button := Button.new()
		button.text = choice.text
		button.pressed.connect(func() -> void:
			_clear_choices()
			if _runner != null:
				_runner.choose(int(choice.index)))
		_choice_box.add_child(button)


func _on_dialogue_ended() -> void:
	_clear_choices()
	if _runner != null:
		var npc_id := _runner.npc_id()
		MemoryStore.ingest_conversation(npc_id, _runner.transcript(), {
			"tick": GameState.universe.tick, "location": _hull.get("id", ""),
		})
		_runner.queue_free()
		_runner = null
		MissionManager.report_event("dialogue_end", {"npc_id": npc_id})


func _clear_choices() -> void:
	for child in _choice_box.get_children():
		child.queue_free()


func _crew_name(crew_id: String) -> String:
	return DataRegistry.get_entity("npcs", crew_id).get("name", crew_id)


func _use_station(target: Dictionary) -> void:
	var station_id: String = target.station_id
	if station_id == "":
		return

	# Player occupies this station
	ShipOperation.occupy(station_id, "player")

	# The pilot's seat IS the flight mode: sitting down takes her out.
	if station_id == "pilot":
		AudioManager.play("ui_switch")
		_append_log("[i]You take the pilot's seat.[/i]")
		launch_requested.emit()
		return

	# Console stations open their minigame panel (power grid, scope,
	# manifest, gunnery). Esc or Close resumes walking.
	var panel := StationMinigames.build(station_id)
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
			_append_log("[i]The pods idle at standby chill. Boris keeps them ready between jumps — the next crossing, you'll be inside one.[/i]")
		_:
			_append_log("[i]You interact with the %s station.[/i]" % station_id.capitalize())
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
	sub.text = "walk: WASD   ·   interact: R   ·   close panel: Esc"
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

	# You can only step off the ship somewhere there is a somewhere.
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

	_choice_box = VBoxContainer.new()
	_choice_box.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT)
	_choice_box.offset_left = -460
	_choice_box.offset_right = -12
	_choice_box.offset_top = -320
	_choice_box.offset_bottom = -110
	_choice_box.alignment = BoxContainer.ALIGNMENT_END
	_hud.add_child(_choice_box)

	_thinking_label = Label.new()
	_thinking_label.text = "· · ·"
	_thinking_label.add_theme_font_size_override("font_size", 18)
	_thinking_label.add_theme_color_override("font_color", Color(0.6, 0.65, 0.75))
	_thinking_label.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT)
	_thinking_label.offset_left = -60
	_thinking_label.offset_top = -130
	_thinking_label.visible = false
	_hud.add_child(_thinking_label)

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
