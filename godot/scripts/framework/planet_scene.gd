extends Node2D
## Ring 0 — the PlanetScene framework scene (P7, Sprint 02).
##
## The Landed (planetside) mode as a place you walk around on: a top-down
## surface whose ground color, sky, and terrain come from the location's
## `biome`, with points of interest (landing pad, market district, ruins,
## settlements, resource nodes) placed from `points_of_interest` and NPCs
## standing at them. You walk a stand-in avatar with WASD, and R acts on the
## nearest thing — talk to a local, trade at the market district, read the
## ruins. Distinct camera from Space flight (Camera2D, top-down).
##
## Everything is explicit data — no procedural generation this sprint. A modder
## adds a planet surface by writing a location.json with a `biome` block and
## POI coordinates (normalized 0..1). The scene names no content id.
##
## Mode transitions are the mode scene's job: PlanetScene emits intent (launch
## to orbit / board ship). It also plays a brief atmosphere-descent settle on
## entry — the visible Space → Atmosphere → Surface hook.

class_name PlanetScene

signal depart_requested        # launch back to orbit (→ Space)
signal board_ship_requested    # climb back into the ship (→ On Board)

const DialogueRunnerScript := preload("res://scripts/framework/dialogue_runner.gd")
const MarketBoardScene := preload("res://scenes/framework/market_board.tscn")

const WORLD := Vector2(1680, 1080)
const WALK_SPEED := 240.0
const INTERACT_RANGE := 78.0

const DEFAULT_BIOME := {
	"ground": "#6b5a3e", "sky": "#3a4a63", "accent": "#8a7550", "terrain": "arid",
}

var _location: Dictionary = {}
var _pois: Array = []           # {id,name,kind,pos:Vector2,npc}
var _npcs: Array = []           # {id,name,pos:Vector2,color}
var _interactables: Array = []  # {kind,name,pos,ref}

var _spawner: NpcSpawner
var _spawned: Array[SoulInstance] = []
var _runner: DialogueRunner = null

var _camera: Camera2D
var _ground: _Ground
var _walker: _Walker
var _pos: Vector2 = WORLD * 0.5
var _frozen := false

var _hud: CanvasLayer
var _log: RichTextLabel
var _hint: Label
var _choice_box: VBoxContainer
var _thinking_label: Label = null
var _market_panel: PanelContainer = null
var _descent := 1.0
var _descent_rect: ColorRect
var _descent_label: Label


func _ready() -> void:
	_spawner = NpcSpawner.new()
	add_child(_spawner)


## Point the surface at a location (the location's own dictionary). Reads its
## biome and POIs, places NPCs, drops the player at the landing pad.
func configure(location: Dictionary) -> void:
	_location = location
	_read_pois()
	_spawned = _spawner.spawn_at_location(location)
	for soul in _spawned:
		soul.spoke.connect(_on_ambient_bark.bind(soul))
		soul.acted.connect(_on_soul_acted.bind(soul))
		soul.concluded.connect(_on_soul_concluded.bind(soul))
	_place_npcs()
	_build_world()
	_build_hud()
	_spawner.broadcast_event("location.player_arrived", {"location": location.get("id", "")})


## --- data → placement --------------------------------------------------------


func _biome() -> Dictionary:
	var b: Dictionary = _location.get("biome", {})
	var out := DEFAULT_BIOME.duplicate()
	for k: String in b:
		out[k] = b[k]
	return out


func _read_pois() -> void:
	for poi: Dictionary in _location.get("points_of_interest", []):
		var xy: Array = poi.get("position", [0.5, 0.5])
		_pois.append({
			"id": poi.get("id", ""), "name": poi.get("name", ""),
			"kind": poi.get("kind", "settlement"),
			"pos": Vector2(float(xy[0]) * WORLD.x, float(xy[1]) * WORLD.y),
			"npc": poi.get("npc", ""),
		})
	# Start the player at the landing pad if there is one, else world center.
	for poi: Dictionary in _pois:
		if poi.kind == "landing_pad":
			_pos = poi.pos + Vector2(0, 90)
			break


## Place each present NPC: at the POI that names it, else spread near the
## first settlement/market POI (or the center). Generic — reads npc `color`.
func _place_npcs() -> void:
	var present: Array = _location.get("npcs_present", [])
	var placed := {}
	for poi: Dictionary in _pois:
		if poi.npc != "" and poi.npc in present:
			_npcs.append(_npc_entry(poi.npc, poi.pos + Vector2(64, 0)))
			placed[poi.npc] = true
	var anchor := _pos
	for poi: Dictionary in _pois:
		if poi.kind in ["market_district", "settlement"]:
			anchor = poi.pos
			break
	var spread := 0
	for npc_id: String in present:
		if placed.has(npc_id):
			continue
		_npcs.append(_npc_entry(npc_id, anchor + Vector2(80 + spread * 60, 60)))
		spread += 1


func _npc_entry(npc_id: String, pos: Vector2) -> Dictionary:
	var npc := DataRegistry.get_entity("npcs", npc_id)
	return {"id": npc_id, "name": npc.get("name", npc_id), "pos": pos,
		"color": StandIn.character_color(npc, npc_id)}


func _rebuild_interactables() -> void:
	_interactables.clear()
	for poi: Dictionary in _pois:
		_interactables.append({"kind": "poi", "name": poi.name, "pos": poi.pos, "ref": poi})
	for npc: Dictionary in _npcs:
		_interactables.append({"kind": "npc", "name": npc.name, "pos": npc.pos, "ref": npc})


## --- world / camera ----------------------------------------------------------


func _build_world() -> void:
	var bg := CanvasLayer.new()
	bg.layer = -1
	add_child(bg)
	var sky := ColorRect.new()
	sky.set_anchors_preset(Control.PRESET_FULL_RECT)
	sky.color = Color.from_string(str(_biome().sky), Color(0.23, 0.29, 0.39))
	bg.add_child(sky)

	_ground = _Ground.new()
	_ground.setup(_biome(), _pois, _npcs)
	add_child(_ground)

	_walker = _Walker.new()
	_walker.color = Color(0.85, 0.86, 0.9)
	_walker.position = _pos
	add_child(_walker)

	_camera = Camera2D.new()
	_camera.position = _pos
	_camera.zoom = Vector2(1.0, 1.0)
	add_child(_camera)
	_camera.make_current()
	_rebuild_interactables()


func _process(delta: float) -> void:
	if _descent > 0.0:
		_descent = maxf(0.0, _descent - delta / 1.2)
		if _descent_rect != null:
			_descent_rect.color.a = _descent
			_descent_label.modulate.a = _descent
	if not _frozen:
		var move := Input.get_vector("strafe_left", "strafe_right", "thrust_forward", "thrust_back")
		if move != Vector2.ZERO:
			_pos += move * WALK_SPEED * delta
			_pos = _pos.clamp(Vector2(40, 40), WORLD - Vector2(40, 40))
			_walker.position = _pos
			_walker.facing = signf(move.x) if absf(move.x) > 0.1 else _walker.facing
		_camera.position = _camera.position.lerp(_pos, 1.0 - exp(-6.0 * delta))
		if Input.is_action_just_pressed("interact"):
			_interact()
	_update_hint()


## --- interaction -------------------------------------------------------------


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
	var verb := "talk to" if target.kind == "npc" else _poi_verb(target.ref.get("kind", ""))
	_hint.text = "R — %s %s" % [verb, target.name]


func _poi_verb(kind: String) -> String:
	match kind:
		"market_district": return "trade at"
		"landing_pad": return "board ship at"
		"ruins": return "examine"
		"resource_node": return "survey"
		_: return "visit"


func _interact() -> void:
	var target := _nearest()
	if target.is_empty():
		return
	if target.kind == "npc":
		var soul := _spawner.get_spawned(target.ref.id)
		if soul != null:
			_talk_to(soul)
		return
	var poi: Dictionary = target.ref
	match poi.kind:
		"market_district":
			_open_market()
		"landing_pad":
			board_ship_requested.emit()
		"ruins":
			_append_log("[i]%s. The Predecessor stone is warm to the touch, and older than any language you know.[/i]" % poi.name)
		"resource_node":
			_append_log("[i]%s — a surveyor's readout would love this. Not today's job.[/i]" % poi.name)
		_:
			_append_log("[i]You look over %s.[/i]" % poi.name)


func _open_market() -> void:
	if _market_panel != null or not ("market" in _location.get("services", [])):
		if not ("market" in _location.get("services", [])):
			_append_log("[i]Stalls are shuttered — no market trading here today.[/i]")
		return
	_frozen = true
	_market_panel = PanelContainer.new()
	_market_panel.set_anchors_preset(Control.PRESET_CENTER)
	_market_panel.position = Vector2(200, 120)
	_market_panel.custom_minimum_size = Vector2(640, 420)
	var box := VBoxContainer.new()
	_market_panel.add_child(box)
	var market: MarketBoard = MarketBoardScene.instantiate()
	market.traded.connect(_on_traded)
	box.add_child(market)
	var close := Button.new()
	close.text = "Leave the market"
	close.pressed.connect(func() -> void:
		_market_panel.queue_free()
		_market_panel = null
		_frozen = false)
	box.add_child(close)
	_hud.add_child(_market_panel)
	market.configure(_location)


## --- talking (authored dialogue first, mind-carried exchange otherwise) ------


func _talk_to(soul: SoulInstance) -> void:
	if _runner != null:
		return
	_frozen = true
	var dialogue := _find_dialogue_for(soul.soul_id)
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
	if SoulGateway.is_ready():
		_append_log("[i]You fall into step with %s.[/i]" % _soul_name(soul))
		soul.perceive_utterance("player", "Got a minute?")
	else:
		_append_log("[i]%s nods at you but keeps their peace.[/i]" % _soul_name(soul))
		_frozen = false


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
		var npc_id: String = _runner.npc_id()
		MemoryStore.ingest_conversation(npc_id, _runner.transcript(), {
			"tick": GameState.universe.tick, "location": _location.get("id", ""),
		})
		_runner.queue_free()
		_runner = null
		MissionManager.report_event("dialogue_end", {"npc_id": npc_id})
	_frozen = false


func _on_ambient_bark(text: String, soul: SoulInstance) -> void:
	if _runner == null and text != "":
		_append_log("[b]%s:[/b] %s" % [_soul_name(soul), text])


func _on_soul_concluded(outcome: String, soul: SoulInstance) -> void:
	if _runner == null and outcome == "abandoned":
		_append_log("[i]%s starts to answer, then loses the thread.[/i]" % _soul_name(soul))
		_frozen = false


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


func _on_traded(good_id: String, amount: int, price: int) -> void:
	var good_name: String = DataRegistry.get_entity("goods", good_id).get("name", good_id)
	_append_log(("Sold %d %s at %d cr each." % [amount, good_name, price]) if amount > 0
		else ("Bought %d %s at %d cr each." % [-amount, good_name, price]))


## --- hud ---------------------------------------------------------------------


func _build_hud() -> void:
	_hud = CanvasLayer.new()
	add_child(_hud)

	var top := VBoxContainer.new()
	top.position = Vector2(16, 12)
	_hud.add_child(top)
	var title := Label.new()
	title.text = _location.get("name", "Surface")
	title.add_theme_font_size_override("font_size", 30)
	top.add_child(title)
	var sub := Label.new()
	var biome := _biome()
	sub.text = "Surface: %s   ·   walk: WASD   ·   act: R" % str(biome.get("terrain", "unknown")).capitalize()
	top.add_child(sub)

	var actions := HBoxContainer.new()
	actions.position = Vector2(16, 84)
	actions.add_theme_constant_override("separation", 8)
	_hud.add_child(actions)
	var board := Button.new()
	board.text = "Board ship"
	board.pressed.connect(func() -> void: board_ship_requested.emit())
	actions.add_child(board)
	var launch := Button.new()
	launch.text = "Launch to orbit"
	launch.pressed.connect(func() -> void: depart_requested.emit())
	actions.add_child(launch)
	var save := Button.new()
	save.text = "Save"
	save.pressed.connect(func() -> void:
		GameState.save_game()
		_append_log("[i]Game saved.[/i]"))
	actions.add_child(save)

	_choice_box = VBoxContainer.new()
	_choice_box.position = Vector2(16, 124)
	_hud.add_child(_choice_box)

	_thinking_label = Label.new()
	_thinking_label.text = "· · ·"
	_thinking_label.add_theme_font_size_override("font_size", 18)
	_thinking_label.add_theme_color_override("font_color", Color(0.6, 0.65, 0.75))
	_thinking_label.position = Vector2(16, 100)
	_thinking_label.visible = false
	_hud.add_child(_thinking_label)

	var log_panel := PanelContainer.new()
	log_panel.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	log_panel.offset_top = -132
	log_panel.offset_left = 12
	log_panel.offset_right = -12
	log_panel.offset_bottom = -40
	_hud.add_child(log_panel)
	_log = RichTextLabel.new()
	_log.bbcode_enabled = true
	_log.scroll_following = true
	log_panel.add_child(_log)

	_hint = Label.new()
	_hint.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	_hint.offset_top = -30
	_hint.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_hint.add_theme_font_size_override("font_size", 18)
	_hud.add_child(_hint)

	# Atmosphere-descent settle: the visible Space → Surface hook.
	_descent_rect = ColorRect.new()
	_descent_rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	_descent_rect.color = Color.from_string(str(_biome().sky), Color(0.23, 0.29, 0.39))
	_descent_rect.color.a = 1.0
	_descent_rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_hud.add_child(_descent_rect)
	_descent_label = Label.new()
	_descent_label.set_anchors_preset(Control.PRESET_CENTER)
	_descent_label.text = "Entering atmosphere — %s" % _location.get("name", "")
	_descent_label.add_theme_font_size_override("font_size", 28)
	_hud.add_child(_descent_label)


func _clear_choices() -> void:
	for child in _choice_box.get_children():
		child.queue_free()


func _soul_name(soul: SoulInstance) -> String:
	return DataRegistry.get_entity("npcs", soul.soul_id).get("name", soul.soul_id)


func _append_log(bbcode: String) -> void:
	_log.append_text(bbcode + "\n")


## --- the surface (ground, terrain, POIs, NPCs) -------------------------------
##
## Draws the biome ground and everything standing on it, top-down, in the same
## flat solid-color language as the character stand-ins. Static: placements are
## explicit data, so one draw suffices.
class _Ground extends Node2D:
	var _biome: Dictionary = {}
	var _pois: Array = []
	var _npcs: Array = []

	func setup(biome: Dictionary, pois: Array, npcs: Array) -> void:
		_biome = biome
		_pois = pois
		_npcs = npcs
		queue_redraw()

	func _draw() -> void:
		var ground := Color.from_string(str(_biome.get("ground", "")), Color(0.42, 0.35, 0.24))
		var accent := Color.from_string(str(_biome.get("accent", "")), ground.lightened(0.2))
		draw_rect(Rect2(Vector2.ZERO, WORLD), ground)
		# Terrain scatter — deterministic, so the surface is hand-placed, not
		# procedural: a fixed seed over a fixed field.
		var rng := RandomNumberGenerator.new()
		rng.seed = 0x50414E  # "PAN"
		for i in 340:
			var p := Vector2(rng.randf() * WORLD.x, rng.randf() * WORLD.y)
			var r := rng.randf_range(3.0, 11.0)
			var c := ground.lerp(accent, rng.randf()) if rng.randf() < 0.7 else ground.darkened(0.15)
			draw_circle(p, r, c)
		# A distant Predecessor skyline along the top edge (the ruins "in the
		# distance") when the biome asks for it.
		if _biome.get("horizon", "") == "ruins":
			_draw_horizon_ruins(accent.darkened(0.3))
		for poi: Dictionary in _pois:
			_draw_poi(poi)
		for npc: Dictionary in _npcs:
			_draw_figure(npc.pos, npc.color, npc.name, npc.id)

	func _draw_horizon_ruins(stone: Color) -> void:
		var rng := RandomNumberGenerator.new()
		rng.seed = 0x5255494E  # "RUIN"
		for i in 9:
			var x := 60.0 + i * (WORLD.x - 120.0) / 9.0
			var hh := rng.randf_range(40.0, 120.0)
			draw_rect(Rect2(x, 8, rng.randf_range(24.0, 48.0), hh), stone)

	func _draw_poi(poi: Dictionary) -> void:
		var pos: Vector2 = poi.pos
		var stone := Color(0.72, 0.70, 0.63)
		match poi.kind:
			"landing_pad":
				draw_circle(pos, 92, Color(0.16, 0.17, 0.2))
				draw_arc(pos, 92, 0, TAU, 40, Color(0.9, 0.75, 0.2), 3.0)
				for a in range(0, 360, 45):
					var d := Vector2.from_angle(deg_to_rad(a))
					draw_line(pos + d * 40, pos + d * 84, Color(0.9, 0.75, 0.2, 0.6), 2.0)
				_draw_parked_ship(pos)
			"market_district":
				for i in 4:
					var s := pos + Vector2(-90 + i * 60, 0)
					draw_rect(Rect2(s, Vector2(46, 30)), Color(0.5, 0.4, 0.3))
					draw_rect(Rect2(s - Vector2(4, 14), Vector2(54, 14)),
						Color(0.75, 0.3, 0.28) if i % 2 == 0 else Color(0.3, 0.55, 0.5))
			"settlement":
				for i in 3:
					var s := pos + Vector2(-70 + i * 70, 0)
					draw_rect(Rect2(s, Vector2(48, 40)), Color(0.6, 0.52, 0.4))
					draw_colored_polygon(PackedVector2Array([
						s + Vector2(-6, 0), s + Vector2(54, 0), s + Vector2(24, -26)]),
						Color(0.45, 0.36, 0.28))
			"ruins":
				for i in 5:
					var c := pos + Vector2(-80 + i * 40, sin(i) * 10)
					var toppled := i == 2
					if toppled:
						draw_rect(Rect2(c, Vector2(64, 16)), stone.darkened(0.1))
					else:
						draw_rect(Rect2(c, Vector2(16, -70)), stone)
						draw_rect(Rect2(c + Vector2(-4, -78), Vector2(24, 12)), stone.lightened(0.1))
			"resource_node":
				for i in 5:
					var c := pos + Vector2(cos(i) * 34, sin(i * 1.7) * 26)
					draw_circle(c, 14, Color(0.5, 0.42, 0.3))
					draw_circle(c, 6, Color(0.85, 0.6, 0.25, 0.8))
			_:
				draw_circle(pos, 20, stone)
		if poi.name != "":
			draw_string(ThemeDB.fallback_font, pos + Vector2(-60, -100),
				str(poi.name), HORIZONTAL_ALIGNMENT_CENTER, 120, 15, Color(0.95, 0.95, 0.95))

	func _draw_parked_ship(center: Vector2) -> void:
		var body := Color(0.44, 0.47, 0.54)
		draw_colored_polygon(PackedVector2Array([
			center + Vector2(0, -46), center + Vector2(20, 20),
			center + Vector2(0, 10), center + Vector2(-20, 20)]), body)
		draw_circle(center + Vector2(0, -12), 7, body.lightened(0.4))

	func _draw_figure(pos: Vector2, color: Color, fig_name: String, id: String) -> void:
		StandIn.paint_character(self, Rect2(pos - Vector2(23, 60), Vector2(46, 74)), color, id)
		draw_string(ThemeDB.fallback_font, pos + Vector2(-40, 24), fig_name,
			HORIZONTAL_ALIGNMENT_CENTER, 80, 13, Color(0.95, 0.95, 0.95))


## The player avatar — the same stand-in vocabulary, drawn each frame so it
## can flip to face travel direction.
class _Walker extends Node2D:
	var color: Color = Color(0.85, 0.86, 0.9)
	var facing: float = 1.0

	func _draw() -> void:
		StandIn.paint_character(self, Rect2(Vector2(-23, -60), Vector2(46, 74)), color, "player")
