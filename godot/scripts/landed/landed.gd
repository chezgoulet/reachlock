extends Node2D
## Landed mode — the walkable-station slice, UI-first for Sprint 01. Shows the
## location (from GameState/start config via DataRegistry), spawns its souls,
## offers dialogue, a minimal market (sell cargo), save, and undock. Engine
## code: every name on screen comes from content.

const DialogueRunnerScript := preload("res://scripts/framework/dialogue_runner.gd")
const MarketBoardScene := preload("res://scenes/framework/market_board.tscn")
const EventFeedScene := preload("res://scenes/framework/event_feed.tscn")

## In-game minutes that pass while the ship runs its departure sequence —
## the batch time-skip the M6 contract requires on undock.
const UNDOCK_DEPARTURE_TICKS := 30

var _location: Dictionary = {}
var _spawner: NpcSpawner
var _spawned: Array[SoulInstance] = []
var _runner: DialogueRunner = null

var _root: VBoxContainer
var _log: RichTextLabel
var _choice_box: VBoxContainer
var _npc_row: HBoxContainer
var _status: Label
var _market: MarketBoard = null
var _feed: EventFeed = null


func _ready() -> void:
	var location_id: String = GameState.player.location
	if location_id == "":
		location_id = DataRegistry.start_config().get("location", "")
		GameState.player.location = location_id
	_location = DataRegistry.get_entity("locations", location_id)
	_spawner = NpcSpawner.new()
	add_child(_spawner)
	_build_ui()
	_spawned = _spawner.spawn_at_location(_location)
	for soul in _spawned:
		soul.spoke.connect(_on_ambient_bark.bind(soul))
		soul.acted.connect(_on_soul_acted.bind(soul))
		soul.concluded.connect(_on_soul_concluded.bind(soul))
	_rebuild_npc_row()
	_spawner.broadcast_event("location.player_arrived", {"location": _location.get("id", "")})
	GameState.state_changed.connect(_refresh_status)
	_refresh_status()


## --- dialogue ----------------------------------------------------------------


func _talk_to(soul: SoulInstance) -> void:
	if _runner != null:
		return
	var dialogue := _find_dialogue_for(soul.soul_id)
	if not dialogue.is_empty():
		_runner = DialogueRunnerScript.new()
		add_child(_runner)
		_runner.line_shown.connect(_on_line_shown)
		_runner.choices_shown.connect(_on_choices_shown)
		_runner.ended.connect(_on_dialogue_ended)
		if _runner.start(dialogue, soul):
			return
		_runner.queue_free()
		_runner = null
	# No authored dialogue applies right now: open an unscripted exchange
	# and let the soul's mind carry it (M7). The reply lands as a bark via
	# `spoke`; with no mind daemon the soul stays quiet and the log says so.
	if SoulGateway.is_ready():
		_append_log("[i]You catch %s's attention.[/i]" % _soul_name(soul))
		soul.perceive_utterance("player", "Got a minute?")
	else:
		_append_log("[i]%s has nothing to say right now.[/i]" % _soul_name(soul))


## First dialogue whose npc matches and whose guard passes. Generic: iterates
## whatever dialogues mods loaded.
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
		button.pressed.connect(_on_choice_pressed.bind(int(choice.index)))
		_choice_box.add_child(button)


func _on_choice_pressed(index: int) -> void:
	_clear_choices()
	if _runner != null:
		_runner.choose(index)


func _on_dialogue_ended() -> void:
	_clear_choices()
	if _runner != null:
		# The soul remembers what was said: distill the transcript to facts.
		MemoryStore.ingest_conversation(_runner.npc_id(), _runner.transcript(), {
			"tick": GameState.universe.tick,
			"location": _location.get("id", ""),
		})
		_runner.queue_free()
		_runner = null
	_rebuild_npc_row()  # guards may have changed (flags, trust)


func _on_ambient_bark(text: String, soul: SoulInstance) -> void:
	if _runner == null and text != "":
		_append_log("[b]%s:[/b] %s" % [_soul_name(soul), text])


## The mind gave up outside a dialogue (inference down, model overloaded):
## give the silence a face instead of leaving the player hanging. The real
## cause is in the pan log; the dev stack's model probe points at it.
func _on_soul_concluded(outcome: String, soul: SoulInstance) -> void:
	if _runner == null and outcome == "abandoned":
		_append_log("[i]%s starts to answer, then loses the thread.[/i]" % _soul_name(soul))


func _on_soul_acted(capability: String, args: Dictionary, soul: SoulInstance) -> void:
	match capability:
		"npc.remember":
			GameState.apply_soul_mutation(soul.soul_id, {
				"op": "add_memory",
				"text": args.get("text", ""),
				"importance": args.get("importance", 0.5),
				"tags": args.get("tags", []),
			})
		"npc.adjust_relationship":
			GameState.apply_soul_mutation(soul.soul_id, {
				"op": "adjust_relationship",
				"target": args.get("toward", "player"),
				"axis": args.get("axis", "trust"),
				"amount": int(args.get("amount", 0)),
			})
		_:
			_append_log("[i]%s %s(%s)[/i]" % [_soul_name(soul), capability, JSON.stringify(args)])


## --- market / news / actions ----------------------------------------------------


func _on_traded(good_id: String, amount: int, price: int) -> void:
	var good_name: String = DataRegistry.get_entity("goods", good_id).get("name", good_id)
	if amount > 0:
		_append_log("Sold %d %s at %d cr each." % [amount, good_name, price])
	else:
		_append_log("Bought %d %s at %d cr each." % [-amount, good_name, price])


## Every news item is also a soul perceive event: the crew and the locals
## read the same feed you do (news.<kind> topics, P3 contract).
func _on_news_item(entry: Dictionary) -> void:
	_spawner.broadcast_event("news." + str(entry.get("kind", "unknown")), entry)


func _undock() -> void:
	# Departure clearance takes in-game time; the universe moves through
	# it in one deterministic batch (M6: time passes while you fly).
	SimGateway.advance_batch(UNDOCK_DEPARTURE_TICKS)
	GameState.player.location = ""
	GameState.clear_flag("survived_ambush")  # each flight earns its own stories
	GameManager.request_mode(GameManager.Mode.SPACE_FLIGHT)


## --- ui ------------------------------------------------------------------------


func _build_ui() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)
	var panel := PanelContainer.new()
	panel.set_anchors_preset(Control.PRESET_FULL_RECT)
	layer.add_child(panel)
	_root = VBoxContainer.new()
	_root.add_theme_constant_override("separation", 10)
	panel.add_child(_root)

	var title := Label.new()
	title.text = _location.get("name", "Somewhere")
	title.add_theme_font_size_override("font_size", 34)
	_root.add_child(title)

	var description := Label.new()
	description.text = _location.get("description", "")
	description.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_root.add_child(description)

	_status = Label.new()
	_root.add_child(_status)

	_npc_row = HBoxContainer.new()
	_npc_row.add_theme_constant_override("separation", 8)
	_root.add_child(_npc_row)

	_log = RichTextLabel.new()
	_log.bbcode_enabled = true
	_log.scroll_following = true
	_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_root.add_child(_log)

	_choice_box = VBoxContainer.new()
	_root.add_child(_choice_box)

	var services: Array = _location.get("services", [])
	# Service panels are framework scenes configured from location data —
	# a mod adds a market or a news bar by editing its location file.
	if "market" in services:
		_market = MarketBoardScene.instantiate()
		_market.traded.connect(_on_traded)
		_root.add_child(_market)
		_market.configure(_location)
	if "bar" in services:
		_feed = EventFeedScene.instantiate()
		_feed.item_added.connect(_on_news_item)
		_root.add_child(_feed)
		_feed.configure()

	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 8)
	_root.add_child(actions)
	var save := Button.new()
	save.text = "Save"
	save.pressed.connect(func() -> void:
		GameState.save_game()
		_append_log("[i]Game saved.[/i]"))
	actions.add_child(save)
	var undock := Button.new()
	undock.text = "Undock"
	undock.pressed.connect(_undock)
	actions.add_child(undock)


func _rebuild_npc_row() -> void:
	for child in _npc_row.get_children():
		child.queue_free()
	for soul in _spawned:
		var button := Button.new()
		button.text = "Talk to %s" % _soul_name(soul)
		button.pressed.connect(_talk_to.bind(soul))
		_npc_row.add_child(button)


func _refresh_status() -> void:
	var cargo_units := 0
	for qty in GameState.player.ship.cargo.values():
		cargo_units += int(qty)
	_status.text = "Credits: %d    Cargo: %d    Hull: %d%%    Tick: %d%s" % [
		GameState.player.credits, cargo_units,
		int(GameState.player.ship.hull_integrity * 100.0),
		int(GameState.universe.tick),
		"" if SimGateway.is_ready() else " (static)",
	]


func _clear_choices() -> void:
	for child in _choice_box.get_children():
		child.queue_free()


func _soul_name(soul: SoulInstance) -> String:
	return DataRegistry.get_entity("npcs", soul.soul_id).get("name", soul.soul_id)


func _append_log(bbcode: String) -> void:
	_log.append_text(bbcode + "\n")


func on_ship_entered() -> void:
	GameManager.request_mode(GameManager.Mode.ON_BOARD)


func on_undock() -> void:
	_undock()
