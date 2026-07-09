extends PanelContainer
## Ring 0 — the StationDock framework scene (P4, Sprint 02).
##
## When you dock, you are in a place. This is that place, built entirely from
## the location's JSON: a docking bay you can see (tiled interior tinted by the
## controlling faction, your own ship parked at the airlock), the NPCs the
## location declares standing in it as recognizable stand-in figures you walk
## up to and talk to, and the service points (market, bar, ship services,
## mission board) it offers — the existing MarketBoard / EventFeed scenes
## mount at their points rather than being rebuilt.
##
## Data-driven end to end. A modder adds a station by writing a location.json
## (kind "station"/"outpost", `services`, `npcs_present`, optional `dock`
## styling) — no engine code. The scene names no content id.
##
## Mode transitions are the mode scene's job: StationDock emits intent
## (undock / board ship / reputation) the way ShipInterior emits launch.

class_name StationDock

signal undock_requested
signal board_ship_requested

const DialogueRunnerScript := preload("res://scripts/framework/dialogue_runner.gd")
const MarketBoardScene := preload("res://scenes/framework/market_board.tscn")
const EventFeedScene := preload("res://scenes/framework/event_feed.tscn")
const ReputationPanelScene := preload("res://scenes/framework/reputation_panel.tscn")

## Framework default interior palette when the controlling faction supplies no
## `color`. Neutral worn-metal — a frontier outpost with nobody polishing it.
const NEUTRAL_INTERIOR := Color(0.20, 0.21, 0.25)

## Human-readable names for the service-point vocabulary (location.services).
## Market and bar mount live scenes; the rest are visual points this sprint
## (the systems behind them are not in scope — the point is rendered, nothing
## is wired behind it).
const SERVICE_LABELS := {
	"market": "Market Counter", "bar": "The Bar", "med_bay": "Med Bay",
	"fuel": "Fuel Depot", "shipyard": "Ship Services", "salvage": "Salvage Yard",
	"mission_board": "Mission Board", "dock": "Docking Control",
}

var _location: Dictionary = {}
var _spawner: NpcSpawner
var _spawned: Array[SoulInstance] = []
var _runner: DialogueRunner = null

var _bay: _DockBay
var _log: RichTextLabel
var _choice_box: VBoxContainer
var _thinking_label: Label = null
var _market: MarketBoard = null
var _feed: EventFeed = null
var _reputation_panel: PanelContainer = null


func _ready() -> void:
	_spawner = NpcSpawner.new()
	add_child(_spawner)


## Point the dock at a location (the location's own dictionary). Spawns the
## NPCs it declares present, lays out the bay, mounts its service points.
func configure(location: Dictionary) -> void:
	_location = location
	_build_ui()
	_spawned = _spawner.spawn_at_location(location)
	for soul in _spawned:
		soul.spoke.connect(_on_ambient_bark.bind(soul))
		soul.acted.connect(_on_soul_acted.bind(soul))
		soul.concluded.connect(_on_soul_concluded.bind(soul))
	_populate_bay()
	_spawner.broadcast_event("location.player_arrived", {"location": location.get("id", "")})
	GameState.state_changed.connect(_refresh_status)
	_refresh_status()
	# Scripted scenes: a dialogue marked `auto` whose guard passes plays as
	# soon as you step off the ship (the content decides when — the guard).
	_maybe_auto_dialogue.call_deferred()
	# Fire the on_dock / on_visit_location faction action trigger.
	Reputation.trigger("on_dock", {
		"location_id": location.get("id", ""),
		"faction_control": location.get("faction_control", ""),
	})
	
	# Ambient station sound
	AudioManager.door_open()
	AudioManager.play("computer_noise", 1.0, -8)


## --- the bay -----------------------------------------------------------------


## The controlling faction's interior color (its `color`, or the neutral
## default). The station's whole character keys off this — a Corp Charter hub
## and a Reach outpost read differently because the faction data differs.
func _interior_color() -> Color:
	var faction := DataRegistry.get_entity("factions", str(_location.get("faction_control", "")))
	return Color.from_string(str(faction.get("color", "")), NEUTRAL_INTERIOR)


func _populate_bay() -> void:
	var slots: Dictionary = _location.get("dock", {}).get("npc_positions", {})
	var present: Array = _location.get("npcs_present", [])
	for i in present.size():
		var soul_id: String = present[i]
		var pos: Vector2
		if slots.has(soul_id):
			var xy: Array = slots[soul_id]
			pos = Vector2(float(xy[0]), float(xy[1]))
		else:
			# Auto-place across the concourse when content gives no positions.
			var t := (float(i) + 1.0) / (float(present.size()) + 1.0)
			pos = Vector2(lerpf(0.18, 0.82, t), 0.62)
		_bay.add_child(_npc_marker(soul_id, pos))


## A present NPC: the shared StandIn figure at its assigned spot in the bay,
## name below, the whole marker clickable to talk. It idles with a soft bob
## (the "animated stand-in" the contract asks for) via the bay's _process.
func _npc_marker(soul_id: String, norm_pos: Vector2) -> Control:
	var npc := DataRegistry.get_entity("npcs", soul_id)
	var marker := Control.new()
	marker.set_meta("norm_pos", norm_pos)
	marker.set_meta("bob_phase", float(hash(soul_id) % 100) / 100.0 * TAU)
	marker.mouse_filter = Control.MOUSE_FILTER_PASS
	marker.custom_minimum_size = Vector2(64, 96)

	var figure := StandIn.new()
	figure.configure(soul_id, StandIn.character_color(npc, soul_id))
	figure.custom_minimum_size = Vector2(46, 74)
	figure.position = Vector2(9, 0)
	figure.mouse_filter = Control.MOUSE_FILTER_IGNORE
	marker.add_child(figure)

	var name_label := Label.new()
	name_label.text = npc.get("name", soul_id)
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	name_label.position = Vector2(0, 76)
	name_label.custom_minimum_size = Vector2(64, 0)
	name_label.add_theme_font_size_override("font_size", 13)
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	marker.add_child(name_label)

	var hit := Button.new()
	hit.flat = true
	hit.set_anchors_preset(Control.PRESET_FULL_RECT)
	hit.tooltip_text = "Talk to %s" % npc.get("name", soul_id)
	hit.pressed.connect(func() -> void:
		var soul := _spawner.get_spawned(soul_id)
		if soul != null:
			_talk_to(soul))
	marker.add_child(hit)
	return marker


## --- talking (authored dialogue first, mind-carried exchange otherwise) ------


func _talk_to(soul: SoulInstance) -> void:
	if _runner != null:
		return
	var dialogue := _find_dialogue_for(soul.soul_id)
	if not dialogue.is_empty() and _start_dialogue(dialogue, soul):
		return
	if SoulGateway.is_ready():
		_append_log("[i]You catch %s's attention.[/i]" % _soul_name(soul))
		soul.perceive_utterance("player", "Got a minute?")
	else:
		_append_log("[i]%s gives you a nod but says nothing.[/i]" % _soul_name(soul))


func _start_dialogue(dialogue: Dictionary, soul: SoulInstance) -> bool:
	_runner = DialogueRunnerScript.new()
	add_child(_runner)
	_runner.line_shown.connect(_on_line_shown)
	_runner.choices_shown.connect(_on_choices_shown)
	_runner.thinking_changed.connect(_on_thinking_changed)
	_runner.ended.connect(_on_dialogue_ended)
	if _runner.start(dialogue, soul):
		return true
	_runner.queue_free()
	_runner = null
	return false


## The mind is working: pulse a quiet ellipsis so the scene visibly breathes.
func _on_thinking_changed(thinking: bool) -> void:
	if _thinking_label != null:
		_thinking_label.visible = thinking


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


## Play the first `auto` dialogue whose npc is present and whose guard
## passes — a scripted scene the player walks into rather than starts.
func _maybe_auto_dialogue() -> void:
	if _runner != null:
		return
	var context := GameState.context()
	for dialogue_id in DataRegistry.ids("dialogues"):
		var dialogue := DataRegistry.get_entity("dialogues", dialogue_id)
		if not dialogue.get("auto", false):
			continue
		var soul := _spawner.get_spawned(dialogue.get("npc", ""))
		if soul == null:
			continue
		var guard: String = dialogue.get("condition", "")
		if guard != "" and not TriggerDSL.evaluate(guard, context):
			continue
		if _start_dialogue(dialogue, soul):
			return


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
		var npc_id: String = _runner.npc_id()
		MemoryStore.ingest_conversation(npc_id, _runner.transcript(), {
			"tick": GameState.universe.tick, "location": _location.get("id", ""),
		})
		_runner.queue_free()
		_runner = null
		MissionManager.report_event("dialogue_end", {"npc_id": npc_id})
		# One scene can hand off to the next (guards decide).
		_maybe_auto_dialogue.call_deferred()


func _on_ambient_bark(text: String, soul: SoulInstance) -> void:
	if _runner == null and text != "":
		_append_log("[b]%s:[/b] %s" % [_soul_name(soul), text])


func _on_soul_concluded(outcome: String, soul: SoulInstance) -> void:
	if _runner == null and outcome == "abandoned":
		_append_log("[i]%s starts to say something, then thinks better of it.[/i]" % _soul_name(soul))


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
		_:
			_append_log("[i]%s %s(%s)[/i]" % [_soul_name(soul), capability, JSON.stringify(args)])


## --- market / news / reputation -----------------------------------------------


func _on_traded(good_id: String, amount: int, price: int) -> void:
	var good_name: String = DataRegistry.get_entity("goods", good_id).get("name", good_id)
	if amount > 0:
		_append_log("Sold %d %s at %d cr each." % [amount, good_name, price])
	else:
		_append_log("Bought %d %s at %d cr each." % [-amount, good_name, price])


## Every news item is also a soul perceive: the locals read the same feed you
## do (news.<kind> topics, P3 contract).
func _on_news_item(entry: Dictionary) -> void:
	_spawner.broadcast_event("news." + str(entry.get("kind", "unknown")), entry)


## Open the reputation panel as a floating overlay. Shows the player's
## standing with every known faction, price modifiers, and relationship
## stances (P8 contract).
func _open_reputation() -> void:
	if _reputation_panel != null:
		_reputation_panel.queue_free()
		_reputation_panel = null
		return
	_reputation_panel = PanelContainer.new()
	_reputation_panel.set_anchors_preset(Control.PRESET_CENTER)
	_reputation_panel.position = Vector2(160, 80)
	_reputation_panel.custom_minimum_size = Vector2(700, 400)
	var box := VBoxContainer.new()
	_reputation_panel.add_child(box)
	var panel: ReputationPanel = ReputationPanelScene.instantiate()
	box.add_child(panel)
	var close := Button.new()
	close.text = "Close"
	close.pressed.connect(func() -> void:
		_reputation_panel.queue_free()
		_reputation_panel = null)
	box.add_child(close)
	add_child(_reputation_panel)
	panel.configure()


## --- ui ----------------------------------------------------------------------


func _build_ui() -> void:
	var root := VBoxContainer.new()
	root.add_theme_constant_override("separation", 8)
	add_child(root)

	_bay = _DockBay.new()
	_bay.custom_minimum_size = Vector2(0, 300)
	_bay.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_bay.setup(_location, _interior_color(), GameState.player.ship.hull_id)
	root.add_child(_bay)

	_log = RichTextLabel.new()
	_log.bbcode_enabled = true
	_log.scroll_following = true
	_log.custom_minimum_size = Vector2(0, 96)
	_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add_child(_log)

	_thinking_label = Label.new()
	_thinking_label.text = "· · ·"
	_thinking_label.add_theme_font_size_override("font_size", 18)
	_thinking_label.add_theme_color_override("font_color", Color(0.6, 0.65, 0.75))
	_thinking_label.visible = false
	root.add_child(_thinking_label)

	_choice_box = VBoxContainer.new()
	root.add_child(_choice_box)

	# Service points. Market and bar mount their live framework scenes; the
	# rest are labeled points (visual this sprint).
	var services: Array = _location.get("services", [])
	var points := HBoxContainer.new()
	points.add_theme_constant_override("separation", 6)
	root.add_child(points)
	for service: String in services:
		if service in ["market", "bar", "shipyard"]:
			continue  # mounted as full panels below
		points.add_child(_service_point(service))

	if "shipyard" in services:
		var shop := UpgradeShop.new()
		shop.purchased.connect(func(upgrade_id: String) -> void:
			var upgrade := DataRegistry.get_entity("upgrades", upgrade_id)
			_append_log("[i]Purchased %s.[/i]" % upgrade.get("name", upgrade_id)))
		root.add_child(shop)
		shop.configure()

	if "market" in services:
		_market = MarketBoardScene.instantiate()
		_market.traded.connect(_on_traded)
		root.add_child(_market)
		_market.configure(_location)
	if "bar" in services:
		_feed = EventFeedScene.instantiate()
		_feed.item_added.connect(_on_news_item)
		root.add_child(_feed)
		_feed.configure()

	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 8)
	root.add_child(actions)
	_add_action(actions, "Board ship", func() -> void: board_ship_requested.emit())
	_add_action(actions, "Save", func() -> void:
		GameState.save_game()
		_append_log("[i]Game saved.[/i]"))
	_add_action(actions, "Reputation", func() -> void:
		_open_reputation())
	_add_action(actions, "Undock", func() -> void: undock_requested.emit())


func _service_point(service: String) -> Control:
	var panel := PanelContainer.new()
	var style := StyleBoxFlat.new()
	var tint := _interior_color().lightened(0.15)
	style.bg_color = tint.darkened(0.5)
	style.border_color = tint
	style.set_border_width_all(2)
	style.set_content_margin_all(8)
	panel.add_theme_stylebox_override("panel", style)
	var label := Label.new()
	label.text = SERVICE_LABELS.get(service, service.capitalize())
	panel.add_child(label)
	return panel


func _add_action(row: HBoxContainer, text: String, on_press: Callable) -> void:
	var button := Button.new()
	button.text = text
	button.pressed.connect(on_press)
	row.add_child(button)


func _refresh_status() -> void:
	if _bay != null:
		_bay.queue_redraw()


func _clear_choices() -> void:
	for child in _choice_box.get_children():
		child.queue_free()


func _soul_name(soul: SoulInstance) -> String:
	return DataRegistry.get_entity("npcs", soul.soul_id).get("name", soul.soul_id)


func _append_log(bbcode: String) -> void:
	_log.append_text(bbcode + "\n")


## --- the docking bay view ----------------------------------------------------
##
## A recognizable interior space, not a gray box: a tiled deck and back wall
## tinted by the controlling faction, an airlock, and the player's own ship
## parked at it. NPC markers are added as children by StationDock and bob
## gently in place here so the bay reads as inhabited.
class _DockBay extends Control:
	var _loc: Dictionary = {}
	var _tint: Color = StationDock.NEUTRAL_INTERIOR
	var _hull_id: String = ""
	var _t: float = 0.0

	func setup(location: Dictionary, tint: Color, hull_id: String) -> void:
		_loc = location
		_tint = tint
		_hull_id = hull_id
		queue_redraw()

	func _process(delta: float) -> void:
		_t += delta
		# Idle bob for each NPC marker — placement from its normalized slot.
		for child in get_children():
			if not child.has_meta("norm_pos"):
				continue
			var np: Vector2 = child.get_meta("norm_pos")
			var phase: float = child.get_meta("bob_phase")
			var base := Vector2(np.x * size.x - 32.0, np.y * size.y - 48.0)
			child.position = base + Vector2(0, sin(_t * 2.0 + phase) * 3.0)
		queue_redraw()

	func _draw() -> void:
		var w := size.x
		var h := size.y
		# Back wall + deck: two tinted bands, tiled with a grid so it reads as
		# a built interior rather than a flat fill.
		var wall := _tint.darkened(0.15)
		var deck := _tint.darkened(0.45)
		var floor_y := h * 0.58
		draw_rect(Rect2(0, 0, w, floor_y), wall)
		draw_rect(Rect2(0, floor_y, w, h - floor_y), deck)
		var line := _tint.lightened(0.1)
		line.a = 0.30
		for x in range(0, int(w) + 48, 48):
			draw_line(Vector2(x, 0), Vector2(x, floor_y), line, 1.0)
		# Deck plating: perspective-ish horizontal courses.
		for i in range(1, 6):
			var y := floor_y + (h - floor_y) * (float(i) / 6.0)
			draw_line(Vector2(0, y), Vector2(w, y), line, 1.0)
		# Airlock on the back wall — a lit doorway the ship sits against.
		var lock := Rect2(w * 0.60, floor_y - h * 0.34, w * 0.20, h * 0.34)
		draw_rect(lock, _tint.darkened(0.6))
		draw_rect(lock, _tint.lightened(0.35), false, 2.0)
		var glow := _tint.lightened(0.5)
		glow.a = 0.35 + 0.15 * sin(_t * 1.5)
		draw_rect(lock.grow(-4), glow)
		# The player's ship, parked at the airlock — visibly present.
		_draw_ship(Vector2(w * 0.32, floor_y - h * 0.06))
		# Station name, over the bay.
		var font := ThemeDB.fallback_font
		draw_string(font, Vector2(16, 30), str(_loc.get("name", "Docking Bay")),
			HORIZONTAL_ALIGNMENT_LEFT, -1, 26, _tint.lightened(0.6))
		var faction := DataRegistry.get_entity("factions",
			str(_loc.get("faction_control", "")))
		var sub := "%s — %s" % [str(_loc.get("kind", "station")).capitalize(),
			faction.get("short_name", faction.get("name", "independent"))]
		draw_string(font, Vector2(16, 52), sub,
			HORIZONTAL_ALIGNMENT_LEFT, -1, 14, _tint.lightened(0.4))

	## The parked ship: a flat top-down hull in the same solid-color language
	## as the character stand-ins. Hull color from the ship's `color`, or a
	## deterministic per-id metal.
	func _draw_ship(center: Vector2) -> void:
		var ship := DataRegistry.get_entity("ships", _hull_id)
		var body := Color.from_string(str(ship.get("color", "")),
			Color(0.42, 0.45, 0.52))
		var s := 1.0
		draw_circle(center + Vector2(0, 30), 60 * s, Color(0, 0, 0, 0.25))
		# Fuselage (nose left, toward the airlock), a diamond-ish hull.
		var pts := PackedVector2Array([
			center + Vector2(70, 0) * s, center + Vector2(10, -22) * s,
			center + Vector2(-64, -14) * s, center + Vector2(-64, 14) * s,
			center + Vector2(10, 22) * s])
		draw_colored_polygon(pts, body)
		# Wing + shaded half for the flat-shading look.
		draw_colored_polygon(PackedVector2Array([
			center + Vector2(10, -22) * s, center + Vector2(-64, -14) * s,
			center + Vector2(-64, 14) * s, center + Vector2(10, 22) * s]),
			body.darkened(0.22))
		# Canopy + engine glow.
		draw_circle(center + Vector2(28, 0) * s, 10 * s, body.lightened(0.4))
		var eng := Color(1.0, 0.55, 0.2, 0.9)
		draw_circle(center + Vector2(-64, -8) * s, 5 * s, eng)
		draw_circle(center + Vector2(-64, 8) * s, 5 * s, eng)
		draw_polyline(pts + PackedVector2Array([pts[0]]), body.darkened(0.6), 1.5)
