extends PanelContainer
## Ring 0 — the ShipInterior framework scene (P5, Sprint 02). The On Board
## mode's walkable space, UI-first with stand-in visuals: rooms from the
## hull data file rendered as color-coded zone panels, crew placed at
## their CrewRoster stations as recognizable colored figures, ship status
## readouts, and a log where the crew's words land.
##
## Data-driven end to end: rooms come from the hull's `interior_rooms`,
## zone colors from the hull's optional `room_zones` (hex per room) with
## a neutral framework default, crew placement from CrewRoster, crew
## colors from each npc file's `color`. A modder redesigns an interior —
## or ships a new hull — by editing data files only.
##
## Every news item and broadcast event reaches ALL crew present (the same
## perceive, each soul answering from its own persona and allegiances) and
## accretes shared history in the CrewRoster: the crew live through the
## same moments, together.

class_name ShipInterior

signal disembark_requested
signal launch_requested

const DialogueRunnerScript := preload("res://scripts/framework/dialogue_runner.gd")

## Framework default zone color per common room type; a hull overrides via
## `room_zones: {room: "#rrggbb"}`. Unknown rooms render neutral gray.
const DEFAULT_ZONE_COLORS := {
	"cockpit": Color(0.25, 0.45, 0.70), "bridge": Color(0.25, 0.45, 0.70),
	"engineering": Color(0.80, 0.45, 0.15),
	"med_bay": Color(0.85, 0.85, 0.88),
	"galley": Color(0.55, 0.65, 0.35), "crew_quarters": Color(0.35, 0.40, 0.65),
	"cargo_hold": Color(0.45, 0.40, 0.30), "airlock": Color(0.50, 0.55, 0.58),
}
const NEUTRAL_ZONE := Color(0.35, 0.35, 0.38)

var _hull: Dictionary = {}
var _spawner: NpcSpawner
var _spawned: Array[SoulInstance] = []
var _runner: DialogueRunner = null

var _rooms_grid: GridContainer
var _title: Label
var _status: Label
var _log: RichTextLabel
var _choice_box: VBoxContainer


func _ready() -> void:
	_spawner = NpcSpawner.new()
	add_child(_spawner)
	_build_ui()
	SimGateway.news.connect(_on_news)
	GameState.state_changed.connect(_refresh)


## Point the interior at a hull (the ship's own dictionary). Spawns the
## crew CrewRoster says is aboard and lays out the rooms.
func configure(hull: Dictionary) -> void:
	_hull = hull
	var context := "Aboard the %s, underway. You are at your station." % hull.get("name", "ship")
	_spawned = _spawner.spawn_souls(CrewRoster.aboard(), context)
	for soul in _spawned:
		soul.spoke.connect(_on_crew_spoke.bind(soul))
	var stations: PackedStringArray = []
	for soul in _spawned:
		stations.append("%s@%s" % [soul.soul_id, CrewRoster.assignment(soul.soul_id)])
	print("aboard: crew present: %s" % ", ".join(stations))
	_refresh()


## --- crew reactions ---------------------------------------------------------------


## The same news reaches everyone aboard — each soul reacts from its own
## persona — and living through it together accretes shared history.
func _on_news(entries: Array) -> void:
	for entry: Dictionary in entries:
		var topic := "news." + str(entry.get("kind", "unknown"))
		_spawner.broadcast_event(topic, entry,
			"React briefly, in character, to this news reaching the ship.")
		var crew_ids: Array = []
		crew_ids.assign(CrewRoster.aboard())
		if crew_ids.size() >= 2:
			CrewRoster.record_shared_event(crew_ids, topic)


func _on_crew_spoke(text: String, soul: SoulInstance) -> void:
	if text != "":
		_append_log("[b]%s:[/b] %s" % [_soul_name(soul), text])
		# Same stdout trace style as SoulGateway's decision log: makes crew
		# speech observable in headless runs and playtest logs.
		print("aboard: %s: %s" % [_soul_name(soul), text])


## --- talking ------------------------------------------------------------------------


func _talk_to(soul: SoulInstance) -> void:
	if _runner != null:
		return
	# Prefer an authored dialogue whose guard passes; otherwise open with an
	# ambient utterance and let the mind carry it.
	var context := GameState.context()
	for dialogue_id in DataRegistry.ids("dialogues"):
		var dialogue := DataRegistry.get_entity("dialogues", dialogue_id)
		if dialogue.get("npc", "") != soul.soul_id:
			continue
		var guard: String = dialogue.get("condition", "")
		if guard == "" or TriggerDSL.evaluate(guard, context):
			_runner = DialogueRunnerScript.new()
			add_child(_runner)
			_runner.line_shown.connect(func(speaker: String, text: String) -> void:
				_append_log("[b]%s:[/b] %s" % [speaker, text]))
			_runner.choices_shown.connect(_on_choices_shown)
			_runner.ended.connect(_on_dialogue_ended)
			if _runner.start(dialogue, soul):
				return
			_runner.queue_free()
			_runner = null
	_append_log("[i]You check in with %s.[/i]" % _soul_name(soul))
	soul.perceive_utterance("player", "Just checking in. How are things at your station?")


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
		MemoryStore.ingest_conversation(_runner.npc_id(), _runner.transcript(), {
			"tick": GameState.universe.tick, "location": "aboard",
		})
		_runner.queue_free()
		_runner = null


## --- ui ------------------------------------------------------------------------------


func _build_ui() -> void:
	var root := VBoxContainer.new()
	root.add_theme_constant_override("separation", 10)
	add_child(root)

	_title = Label.new()
	_title.add_theme_font_size_override("font_size", 30)
	root.add_child(_title)

	_status = Label.new()
	root.add_child(_status)

	_rooms_grid = GridContainer.new()
	_rooms_grid.columns = 4
	_rooms_grid.add_theme_constant_override("h_separation", 10)
	_rooms_grid.add_theme_constant_override("v_separation", 10)
	root.add_child(_rooms_grid)

	_log = RichTextLabel.new()
	_log.bbcode_enabled = true
	_log.scroll_following = true
	_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_log.custom_minimum_size = Vector2(0, 120)
	root.add_child(_log)

	_choice_box = VBoxContainer.new()
	root.add_child(_choice_box)

	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 8)
	root.add_child(actions)
	var fly := Button.new()
	fly.text = "Take the stick (fly)"
	fly.pressed.connect(func() -> void: launch_requested.emit())
	actions.add_child(fly)
	if GameState.is_docked():
		var disembark := Button.new()
		disembark.text = "Disembark"
		disembark.pressed.connect(func() -> void: disembark_requested.emit())
		actions.add_child(disembark)
	var save := Button.new()
	save.text = "Save"
	save.pressed.connect(func() -> void:
		GameState.save_game()
		_append_log("[i]Game saved.[/i]"))
	actions.add_child(save)


func _refresh() -> void:
	if _hull.is_empty():
		return
	_title.text = _hull.get("name", "Ship")
	var cargo_units := 0
	for qty in GameState.player.ship.cargo.values():
		cargo_units += int(qty)
	_status.text = "Hull: %d%%    Cargo: %d / %d    Credits: %d    Tick: %d" % [
		int(GameState.player.ship.hull_integrity * 100.0),
		cargo_units, int(_hull.get("stats", {}).get("cargo_capacity", 0)),
		GameState.player.credits, int(GameState.universe.tick),
	]
	_rebuild_rooms()


func _rebuild_rooms() -> void:
	for child in _rooms_grid.get_children():
		child.queue_free()
	var overrides: Dictionary = _hull.get("room_zones", {})
	for room: String in _hull.get("interior_rooms", []):
		var panel := PanelContainer.new()
		var style := StyleBoxFlat.new()
		var zone: Color = DEFAULT_ZONE_COLORS.get(room, NEUTRAL_ZONE)
		if overrides.has(room):
			zone = Color.from_string(str(overrides[room]), zone)
		style.bg_color = zone.darkened(0.55)
		style.border_color = zone
		style.set_border_width_all(2)
		style.set_content_margin_all(8)
		panel.add_theme_stylebox_override("panel", style)
		panel.custom_minimum_size = Vector2(190, 110)
		var box := VBoxContainer.new()
		panel.add_child(box)
		var room_label := Label.new()
		room_label.text = room.capitalize()
		room_label.add_theme_color_override("font_color", zone.lightened(0.4))
		box.add_child(room_label)
		for soul_id in CrewRoster.assigned_to(room):
			box.add_child(_crew_figure(soul_id))
		_rooms_grid.add_child(panel)


## A recognizable stand-in figure: the npc's color as a body swatch, the
## name beside it, and a talk button. Sprite art replaces the swatch later;
## a mod overrides by adding `color` (and eventually sprites) to its npc.
func _crew_figure(soul_id: String) -> Control:
	var npc := DataRegistry.get_entity("npcs", soul_id)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 6)
	var swatch := ColorRect.new()
	swatch.custom_minimum_size = Vector2(14, 26)
	swatch.color = Color.from_string(str(npc.get("color", "")), _hash_color(soul_id))
	row.add_child(swatch)
	var button := Button.new()
	button.text = npc.get("name", soul_id)
	button.flat = true
	button.pressed.connect(func() -> void:
		var soul := _spawner.get_spawned(soul_id)
		if soul != null:
			_talk_to(soul))
	row.add_child(button)
	return row


## Deterministic fallback color per crew member when content supplies none.
func _hash_color(soul_id: String) -> Color:
	var h := float(hash(soul_id) % 360) / 360.0
	return Color.from_hsv(h, 0.55, 0.85)


func _soul_name(soul: SoulInstance) -> String:
	return DataRegistry.get_entity("npcs", soul.soul_id).get("name", soul.soul_id)


func _append_log(bbcode: String) -> void:
	_log.append_text(bbcode + "\n")


func _clear_choices() -> void:
	for child in _choice_box.get_children():
		child.queue_free()
