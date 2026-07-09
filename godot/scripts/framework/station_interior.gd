extends Node2D
## Ring 0 — StationInterior: a station you WALK (Sprint 2). Mounted by the
## landed mode when a location carries an `interior` block — rooms, props,
## service points, and npc positions all from content, rendered by the same
## InteriorWorld as the ship. The classic StationDock panel remains the
## fallback for locations without one.
##
## What lives here: 4-direction walking with wall collision, NPCs as
## CharacterSprites you talk to (authored dialogue first, auto scenes on
## arrival, mind-carried exchange otherwise), and props that ARE the
## station's services — the bar opens the news feed, the outfitting counter
## the UpgradeShop, the market counter the MarketBoard, the airlock the
## undock/board panel. The scene names no content id.

class_name StationInterior

signal undock_requested
signal board_ship_requested

const DialogueRunnerScript := preload("res://scripts/framework/dialogue_runner.gd")
const MarketBoardScene := preload("res://scenes/framework/market_board.tscn")
const EventFeedScene := preload("res://scenes/framework/event_feed.tscn")
const ReputationPanelScene := preload("res://scenes/framework/reputation_panel.tscn")

const WALK_SPEED := 220.0
const INTERACT_RANGE := 52.0

const SERVICE_LABELS := {
	"market": "Market Counter", "bar": "The Bar", "med_bay": "Med Bay",
	"fuel": "Fuel Depot", "shipyard": "Outfitting", "salvage": "Salvage Yard",
	"mission_board": "Mission Board", "dock": "Airlock — Your Berth",
}

var _location: Dictionary = {}
var _rooms: Array = []
var _interactables: Array = []  # {kind: npc|service, name, pos, id}
var _world: InteriorWorld
var _walker: CharacterSprite
var _camera: Camera2D
var _pos := Vector2.ZERO
var _world_size := Vector2(1280, 800)
var _frozen := false

var _spawner: NpcSpawner
var _spawned: Array[SoulInstance] = []
var _runner: DialogueRunner = null

var _hud: CanvasLayer
var _log: RichTextLabel
var _hint: Label
var _dialogue_panel: DialoguePanel
var _service_panel: Control = null


func _ready() -> void:
	_spawner = NpcSpawner.new()
	add_child(_spawner)


func configure(location: Dictionary) -> void:
	_location = location
	_parse_rooms()
	_build_world()
	_build_hud()

	_spawned = _spawner.spawn_at_location(location)
	for soul in _spawned:
		soul.spoke.connect(_on_ambient_bark.bind(soul))
		soul.acted.connect(_on_soul_acted.bind(soul))
	_place_npcs()
	_rebuild_interactables()

	_spawner.broadcast_event("location.player_arrived", {"location": location.get("id", "")})
	Reputation.trigger("on_dock", {
		"location_id": location.get("id", ""),
		"faction_control": location.get("faction_control", ""),
	})
	AudioManager.door_open()
	AudioManager.play("computer_noise", 1.0, -8)
	# Scripted scenes: a dialogue marked `auto` whose guard passes plays as
	# soon as you step off the ship.
	_maybe_auto_dialogue.call_deferred()


## --- data → world ---------------------------------------------------------------


func _parse_rooms() -> void:
	_rooms.clear()
	var interior: Dictionary = _location.get("interior", {})
	var max_extent := Vector2(1280, 800)
	for entry: Dictionary in interior.get("rooms", []):
		var rect := Rect2(entry.get("x", 0.0), entry.get("y", 0.0),
			entry.get("w", 100.0), entry.get("h", 100.0))
		max_extent = max_extent.max(rect.end + Vector2(40, 40))
		var doors: Array = []
		for d: Dictionary in entry.get("doors", []):
			doors.append({
				"to": d.get("to", ""), "side": d.get("side", "right"),
				"offset": d.get("offset", 0.5), "width": d.get("width", 40.0),
			})
		var kind: String = entry.get("kind", entry.get("id", ""))
		_rooms.append({
			"id": entry.get("id", ""),
			"name": entry.get("name", str(entry.get("id", "")).capitalize().replace("_", " ")),
			"kind": kind,
			"rect": rect,
			"color": Color.from_string(str(entry.get("color", "")), _default_room_color(kind)),
			"doors": doors,
			"props": entry.get("props", []),
		})
	_world_size = max_extent


func _default_room_color(kind: String) -> Color:
	match kind:
		"bar": return Color(0.55, 0.42, 0.28)
		"office": return Color(0.45, 0.42, 0.36)
		"market": return Color(0.42, 0.46, 0.40)
		"corridor": return Color(0.38, 0.40, 0.46)
		_: return Color(0.36, 0.36, 0.40)


func _build_world() -> void:
	var bg := ColorRect.new()
	bg.color = Color(0.05, 0.055, 0.08)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(bg)

	_world = InteriorWorld.new()
	add_child(_world)
	_world.setup(_rooms)

	var spawn: Array = _location.get("interior", {}).get("spawn", [])
	_pos = Vector2(spawn[0], spawn[1]) if spawn.size() == 2 else _world_size * 0.5

	_walker = CharacterSprite.new()
	var character := GameState.player_character()
	if character != "":
		var npc := DataRegistry.get_entity("npcs", character)
		_walker.setup("npcs", character, StandIn.character_color(npc, character))
	else:
		_walker.setup("player", "character", Color(0.85, 0.86, 0.9))
	_walker.position = _pos
	add_child(_walker)

	_camera = Camera2D.new()
	_camera.position = _pos
	_camera.zoom = Vector2(1.5, 1.5)
	add_child(_camera)
	_camera.make_current()


func _place_npcs() -> void:
	var positions: Dictionary = _location.get("interior", {}).get("npc_positions", {})
	var fallback_x := _pos.x
	for npc_id: String in _location.get("npcs_present", []):
		var npc := DataRegistry.get_entity("npcs", npc_id)
		var pos: Vector2
		if positions.has(npc_id):
			var xy: Array = positions[npc_id]
			pos = Vector2(float(xy[0]), float(xy[1]))
		else:
			fallback_x += 70.0
			pos = Vector2(fallback_x, _pos.y - 40.0)
		var figure := CharacterSprite.new()
		figure.setup("npcs", npc_id, StandIn.character_color(npc, npc_id), npc.get("name", npc_id))
		figure.position = pos
		add_child(figure)
		_interactables.append({"kind": "npc", "name": npc.get("name", npc_id),
			"pos": pos, "id": npc_id})


func _rebuild_interactables() -> void:
	# NPCs were appended in _place_npcs; add every service prop.
	for room: Dictionary in _rooms:
		for prop: Dictionary in room.get("props", []):
			var service: String = prop.get("service", "")
			if service == "":
				continue
			_interactables.append({
				"kind": "service",
				"name": prop.get("name", SERVICE_LABELS.get(service, service.capitalize())),
				"pos": Vector2(prop.get("x", 0.0), prop.get("y", 0.0)),
				"id": service,
			})


## --- walking ---------------------------------------------------------------------


func _process(delta: float) -> void:
	if not _frozen:
		var move := Input.get_vector("strafe_left", "strafe_right", "thrust_forward", "thrust_back")
		if move != Vector2.ZERO:
			_try_move(move * WALK_SPEED * delta)
		_walker.set_motion(move, move != Vector2.ZERO)
		_camera.position = _camera.position.lerp(_pos, 1.0 - exp(-6.0 * delta))
		if Input.is_action_just_pressed("interact"):
			_interact()
	_update_hint()


func _try_move(step: Vector2) -> void:
	var next := (_pos + step).clamp(Vector2(20, 20), _world_size - Vector2(20, 20))
	if _world.is_walkable(next):
		_pos = next
	elif _world.is_walkable(Vector2(next.x, _pos.y)):
		_pos.x = next.x
	elif _world.is_walkable(Vector2(_pos.x, next.y)):
		_pos.y = next.y
	_walker.position = _pos


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
		_world.clear_highlight()
		return
	_hint.text = ("R — talk to %s" if target.kind == "npc" else "R — use %s") % target.name
	_world.set_highlight(target.pos)


func _interact() -> void:
	var target := _nearest()
	if target.is_empty():
		return
	if target.kind == "npc":
		var soul := _spawner.get_spawned(target.id)
		if soul != null:
			_talk_to(soul)
	else:
		_open_service(target.id)


## --- services ---------------------------------------------------------------------


func _open_service(service: String) -> void:
	if _service_panel != null:
		return
	match service:
		"market":
			var market: MarketBoard = MarketBoardScene.instantiate()
			market.traded.connect(_on_traded)
			_mount_service_panel(market, "Market")
			market.configure(_location)
		"shipyard":
			var shop := UpgradeShop.new()
			shop.purchased.connect(func(upgrade_id: String) -> void:
				var upgrade := DataRegistry.get_entity("upgrades", upgrade_id)
				_append_log("[i]Purchased %s.[/i]" % upgrade.get("name", upgrade_id)))
			_mount_service_panel(shop, "Outfitting")
			shop.configure()
		"bar":
			var feed: EventFeed = EventFeedScene.instantiate()
			feed.item_added.connect(_on_news_item)
			_mount_service_panel(feed, "The word going around")
			feed.configure()
		"dock":
			_open_dock_panel()
		_:
			_append_log("[i]The %s is quiet today.[/i]" % SERVICE_LABELS.get(service, service))


func _mount_service_panel(inner: Control, title: String) -> void:
	_frozen = true
	var panel := PanelContainer.new()
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0.07, 0.08, 0.11, 0.97)
	style.border_color = Color(0.45, 0.55, 0.70, 0.7)
	style.set_border_width_all(2)
	style.set_content_margin_all(12)
	panel.add_theme_stylebox_override("panel", style)
	panel.set_anchors_preset(Control.PRESET_CENTER)
	panel.custom_minimum_size = Vector2(640, 0)
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 8)
	panel.add_child(box)
	var header := HBoxContainer.new()
	box.add_child(header)
	var label := Label.new()
	label.text = title
	label.add_theme_font_size_override("font_size", 19)
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(label)
	var close := Button.new()
	close.text = "Close  (Esc)"
	close.pressed.connect(_close_service_panel)
	header.add_child(close)
	box.add_child(inner)
	_hud.add_child(panel)
	_service_panel = panel


func _close_service_panel() -> void:
	if _service_panel != null:
		_service_panel.queue_free()
		_service_panel = null
	_frozen = false


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel") and _service_panel != null:
		_close_service_panel()


func _open_dock_panel() -> void:
	var inner := VBoxContainer.new()
	inner.add_theme_constant_override("separation", 8)
	var blurb := Label.new()
	blurb.text = "The ship waits at the berth, patched and patient."
	blurb.add_theme_color_override("font_color", Color(0.62, 0.66, 0.74))
	inner.add_child(blurb)
	var board := Button.new()
	board.text = "Board ship"
	board.pressed.connect(func() -> void:
		_close_service_panel()
		board_ship_requested.emit())
	inner.add_child(board)
	var undock := Button.new()
	undock.text = "Undock"
	undock.pressed.connect(func() -> void:
		_close_service_panel()
		undock_requested.emit())
	inner.add_child(undock)
	var save := Button.new()
	save.text = "Save"
	save.pressed.connect(func() -> void:
		GameState.save_game()
		_append_log("[i]Game saved.[/i]"))
	inner.add_child(save)
	var reputation := Button.new()
	reputation.text = "Reputation"
	reputation.pressed.connect(func() -> void:
		_close_service_panel()
		var wrap := VBoxContainer.new()
		var panel: ReputationPanel = ReputationPanelScene.instantiate()
		wrap.add_child(panel)
		_mount_service_panel(wrap, "Standing")
		panel.configure())
	inner.add_child(reputation)
	_mount_service_panel(inner, str(_location.get("name", "Berth")))


## --- talking (same contract as every dialogue host) --------------------------------


func _talk_to(soul: SoulInstance) -> void:
	if _runner != null:
		return
	var dialogue := _find_dialogue_for(soul.soul_id)
	if not dialogue.is_empty() and _start_dialogue(dialogue, soul):
		return
	if SoulGateway.is_ready():
		_dialogue_panel.bark("You", "Got a minute?")
		soul.perceive_utterance("player", "Got a minute?")
	else:
		_dialogue_panel.bark(_soul_name(soul), "*gives you a nod but says nothing*")


func _start_dialogue(dialogue: Dictionary, soul: SoulInstance) -> bool:
	_runner = DialogueRunnerScript.new()
	add_child(_runner)
	_runner.line_shown.connect(_dialogue_panel.show_line)
	_runner.choices_shown.connect(_dialogue_panel.show_choices)
	_runner.thinking_changed.connect(_dialogue_panel.set_thinking)
	_runner.ended.connect(_on_dialogue_ended)
	var npc_name: String = DataRegistry.get_entity("npcs", dialogue.get("npc", "")) \
		.get("name", dialogue.get("npc", "?"))
	_dialogue_panel.open(npc_name,
		"linked" if soul != null and SoulGateway.is_ready() else "offline")
	if _runner.start(dialogue, soul):
		_frozen = true
		return true
	_dialogue_panel.close()
	_runner.queue_free()
	_runner = null
	return false


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


func _maybe_auto_dialogue() -> void:
	if _runner != null or _dialogue_panel.is_open():
		return
	var context := GameState.context()
	for dialogue_id in DataRegistry.ids("dialogues"):
		var dialogue := DataRegistry.get_entity("dialogues", dialogue_id)
		if not dialogue.get("auto", false):
			continue
		var guard: String = dialogue.get("condition", "")
		if guard != "" and not TriggerDSL.evaluate(guard, context):
			continue
		# The scene's speaker IS the player: the narration card carries the
		# beat instead (playable.self_dialogue_summaries), flags included.
		if dialogue.get("npc", "") == GameState.player_character() \
				and _play_self_scene(dialogue):
			return
		var soul := _spawner.get_spawned(dialogue.get("npc", ""))
		if soul == null:
			continue
		if _start_dialogue(dialogue, soul):
			return


func _play_self_scene(dialogue: Dictionary) -> bool:
	var character := GameState.player_character()
	var card: Dictionary = DataRegistry.get_entity("npcs", character) \
		.get("playable", {}).get("self_dialogue_summaries", {}) \
		.get(dialogue.get("id", ""), {})
	if card.is_empty():
		return false
	_frozen = true
	for mutation: Dictionary in card.get("mutations", []):
		GameState.apply_soul_mutation(character, mutation)
	# The beat completes NOW (mutations + mission event); dismissing the
	# card only hands back control — a save mid-card can't strand a stage.
	MissionManager.report_event("dialogue_end", {"npc_id": character})
	var npc_name: String = DataRegistry.get_entity("npcs", character).get("name", character)
	_dialogue_panel.show_narration(npc_name, card.get("text", ""))
	_dialogue_panel.narration_done.connect(func() -> void:
		_frozen = false
		_maybe_auto_dialogue.call_deferred(),
		CONNECT_ONE_SHOT)
	return true


func _on_dialogue_ended() -> void:
	_frozen = false
	_dialogue_panel.close()
	if _runner != null:
		var npc_id: String = _runner.npc_id()
		MemoryStore.ingest_conversation(npc_id, _runner.transcript(), {
			"tick": GameState.universe.tick, "location": _location.get("id", ""),
		})
		_runner.queue_free()
		_runner = null
		MissionManager.report_event("dialogue_end", {"npc_id": npc_id})
		_maybe_auto_dialogue.call_deferred()


func _on_ambient_bark(text: String, soul: SoulInstance) -> void:
	if _runner == null and text != "":
		_dialogue_panel.bark(_soul_name(soul), text)


func _on_soul_acted(capability: String, args: Dictionary, soul: SoulInstance) -> void:
	match capability:
		"npc.remember":
			GameState.apply_soul_mutation(soul.soul_id, {
				"op": "add_memory", "text": args.get("text", ""),
				"importance": args.get("importance", 0.5), "tags": args.get("tags", [])})
		"npc.adjust_relationship":
			GameState.apply_soul_mutation(soul.soul_id, {
				"op": "adjust_relationship", "target": args.get("toward", "player"),
				"axis": args.get("axis", "trust"), "amount": int(args.get("amount", 0))})


func _on_traded(good_id: String, amount: int, price: int) -> void:
	var good_name: String = DataRegistry.get_entity("goods", good_id).get("name", good_id)
	if amount > 0:
		_append_log("Sold %d %s at %d cr each." % [amount, good_name, price])
	else:
		_append_log("Bought %d %s at %d cr each." % [-amount, good_name, price])


func _on_news_item(entry: Dictionary) -> void:
	_spawner.broadcast_event("news." + str(entry.get("kind", "unknown")), entry)


## --- hud -------------------------------------------------------------------------


func _build_hud() -> void:
	_hud = CanvasLayer.new()
	add_child(_hud)

	var title := Label.new()
	title.text = str(_location.get("name", "Station"))
	title.position = Vector2(16, 12)
	title.add_theme_font_size_override("font_size", 24)
	_hud.add_child(title)
	var sub := Label.new()
	sub.text = "walk: WASD   ·   interact: R"
	sub.position = Vector2(16, 44)
	sub.add_theme_font_size_override("font_size", 13)
	sub.add_theme_color_override("font_color", Color(0.6, 0.63, 0.7))
	_hud.add_child(sub)

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
	_hud.add_child(_dialogue_panel)

	_hint = Label.new()
	_hint.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	_hint.offset_top = -28
	_hint.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_hint.add_theme_font_size_override("font_size", 17)
	_hud.add_child(_hint)


func _soul_name(soul: SoulInstance) -> String:
	return DataRegistry.get_entity("npcs", soul.soul_id).get("name", soul.soul_id)


func _append_log(bbcode: String) -> void:
	_log.append_text(bbcode + "\n")
