extends Control
## Ring 0 — CharacterSelect: who are you this run? (Sprint 3)
##
## A fresh playthrough opens here: every crew member whose npc file carries a
## `playable` block is a seat you can take. The screen shows the character,
## their stats (the five framework stats as pips), what they're good at and
## bad at, and the merchant-honest tagline. Confirming sets
## GameState.player.character, rolls the opening text scrawl (the manifest's
## shared paragraphs, then the character's own), and hands control back to
## the boot flow via `finished`.
##
## Everything shown is data; the engine never names a character.

class_name CharacterSelect

signal finished

const STAT_LABELS := {
	"piloting": "Piloting", "engineering": "Engineering", "medicine": "Medicine",
	"grit": "Grit", "savvy": "Savvy",
}
const STAT_ORDER := ["piloting", "engineering", "medicine", "grit", "savvy"]
const SCRAWL_SPEED := 36.0  # px/s upward

var _roster: Array = []       # [{id, npc}]
var _selected := 0
var _list: VBoxContainer
var _portrait: _Portrait
var _name_label: Label
var _tagline: Label
var _stats_box: VBoxContainer
var _traits: RichTextLabel
var _scrawl_layer: Control = null
var _scrawl_text: RichTextLabel = null
var _done := false


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_load_roster()
	_build_ui()
	if _roster.is_empty():
		_finish("")
		return
	_select(0)


func _load_roster() -> void:
	_roster.clear()
	var hull_id: String = GameState.player.ship.hull_id
	for npc_id in DataRegistry.ids("npcs"):
		var npc := DataRegistry.get_entity("npcs", npc_id)
		if not npc.has("playable"):
			continue
		if npc.get("ship", "") != hull_id or not npc.get("aboard", false):
			continue
		_roster.append({"id": npc_id, "npc": npc})
	_roster.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return str(a.npc.get("name", "")) < str(b.npc.get("name", "")))


## Roster size (contract-testable).
func roster_size() -> int:
	return _roster.size()


func selected_id() -> String:
	if _roster.is_empty():
		return ""
	return _roster[_selected].id


## --- ui ---------------------------------------------------------------------------


func _build_ui() -> void:
	var bg := ColorRect.new()
	bg.color = Color(0.04, 0.05, 0.08)
	bg.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(bg)

	var title := Label.new()
	title.text = "WHO WAKES FIRST?"
	title.add_theme_font_size_override("font_size", 30)
	title.add_theme_color_override("font_color", Color(0.95, 0.9, 0.7))
	title.position = Vector2(60, 34)
	add_child(title)
	var subtitle := Label.new()
	subtitle.text = "Seven souls on the manifest. Pick the one whose boots you're in."
	subtitle.add_theme_font_size_override("font_size", 15)
	subtitle.add_theme_color_override("font_color", Color(0.6, 0.65, 0.75))
	subtitle.position = Vector2(60, 74)
	add_child(subtitle)

	_list = VBoxContainer.new()
	_list.position = Vector2(60, 120)
	_list.custom_minimum_size = Vector2(300, 0)
	_list.add_theme_constant_override("separation", 4)
	add_child(_list)
	for i in _roster.size():
		var entry: Dictionary = _roster[i]
		var button := Button.new()
		button.text = "%s   —   %s" % [entry.npc.get("name", entry.id),
			str(entry.npc.get("role", "")).capitalize()]
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.custom_minimum_size = Vector2(300, 40)
		button.pressed.connect(_select.bind(i))
		_list.add_child(button)

	var begin := Button.new()
	begin.text = "BEGIN  (Enter)"
	begin.custom_minimum_size = Vector2(300, 52)
	begin.position = Vector2(60, 130 + _roster.size() * 44 + 30)
	begin.pressed.connect(func() -> void: _begin())
	add_child(begin)

	var sheet := VBoxContainer.new()
	sheet.position = Vector2(440, 120)
	sheet.custom_minimum_size = Vector2(720, 0)
	sheet.add_theme_constant_override("separation", 8)
	add_child(sheet)

	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", 20)
	sheet.add_child(head)
	_portrait = _Portrait.new()
	_portrait.custom_minimum_size = Vector2(120, 160)
	head.add_child(_portrait)
	var head_text := VBoxContainer.new()
	head.add_child(head_text)
	_name_label = Label.new()
	_name_label.add_theme_font_size_override("font_size", 26)
	head_text.add_child(_name_label)
	_tagline = Label.new()
	_tagline.add_theme_font_size_override("font_size", 15)
	_tagline.add_theme_color_override("font_color", Color(0.7, 0.75, 0.85))
	_tagline.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_tagline.custom_minimum_size = Vector2(540, 0)
	head_text.add_child(_tagline)
	_stats_box = VBoxContainer.new()
	_stats_box.add_theme_constant_override("separation", 2)
	head_text.add_child(_stats_box)

	_traits = RichTextLabel.new()
	_traits.bbcode_enabled = true
	_traits.fit_content = true
	_traits.custom_minimum_size = Vector2(700, 220)
	_traits.add_theme_font_size_override("normal_font_size", 15)
	sheet.add_child(_traits)


func _select(index: int) -> void:
	_selected = clampi(index, 0, _roster.size() - 1)
	var entry: Dictionary = _roster[_selected]
	var npc: Dictionary = entry.npc
	var playable: Dictionary = npc.get("playable", {})

	for i in _list.get_child_count():
		(_list.get_child(i) as Button).modulate = \
			Color(1, 1, 0.85) if i == _selected else Color(0.75, 0.78, 0.85)

	_portrait.configure(entry.id, npc)
	_name_label.text = npc.get("name", entry.id)
	_tagline.text = str(playable.get("tagline", ""))

	for child in _stats_box.get_children():
		child.queue_free()
	var stats: Dictionary = playable.get("stats", {})
	for stat: String in STAT_ORDER:
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 8)
		_stats_box.add_child(row)
		var stat_name := Label.new()
		stat_name.text = STAT_LABELS.get(stat, stat.capitalize())
		stat_name.custom_minimum_size = Vector2(110, 0)
		stat_name.add_theme_font_size_override("font_size", 14)
		row.add_child(stat_name)
		var pips := _Pips.new()
		pips.value = int(stats.get(stat, 2))
		pips.custom_minimum_size = Vector2(110, 16)
		row.add_child(pips)

	var text := ""
	for advantage: String in playable.get("advantages", []):
		text += "[color=#7dd87d]▲[/color]  %s\n" % advantage
	for disadvantage: String in playable.get("disadvantages", []):
		text += "[color=#d87d6d]▼[/color]  %s\n" % disadvantage
	var locomotion: Dictionary = npc.get("locomotion", {})
	if str(locomotion.get("zero_g", "")) == "magnetic":
		text += "\n[color=#8fb8e8]◆  Mag-locked: walks zero-G decks"
		if float(locomotion.get("zero_g_speed_mult", 1.0)) < 0.7:
			text += " (slowly — a nav chassis, not a deck chassis)"
		if float(locomotion.get("gravity_speed_mult", 1.0)) < 0.7:
			text += "; crawls under gravity"
		text += ".[/color]\n"
	else:
		text += "\n[color=#8fb8e8]◆  Drifts in zero-G — grab the flight suit below decks before climbing the ladder.[/color]\n"
	_traits.text = text


func _input(event: InputEvent) -> void:
	if _done:
		return
	if _scrawl_layer != null:
		if event.is_action_pressed("ui_accept") or event.is_action_pressed("ui_cancel") \
				or event.is_action_pressed("interact"):
			accept_event()
			_finish(selected_id())
		return
	if event.is_action_pressed("ui_down"):
		accept_event()
		_select(_selected + 1)
	elif event.is_action_pressed("ui_up"):
		accept_event()
		_select(_selected - 1)
	elif event.is_action_pressed("ui_accept"):
		accept_event()
		_begin()


## --- the scrawl -------------------------------------------------------------------


func _begin() -> void:
	if _roster.is_empty() or _scrawl_layer != null:
		return
	var character := selected_id()
	GameState.set_player_character(character)
	AudioManager.play("ui_switch")

	_scrawl_layer = Control.new()
	_scrawl_layer.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(_scrawl_layer)
	var black := ColorRect.new()
	black.color = Color(0.01, 0.015, 0.03)
	black.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_scrawl_layer.add_child(black)

	var paragraphs: Array = []
	paragraphs.append_array(DataRegistry.start_config().get("scrawl", []))
	paragraphs.append_array(DataRegistry.get_entity("npcs", character)
		.get("playable", {}).get("scrawl", []))

	# Anchor-centered rather than positioned from a size read at creation
	# time: horizontal centering must hold regardless of when layout catches
	# up, so pin both edges to the midline instead of computing an offset.
	_scrawl_text = RichTextLabel.new()
	_scrawl_text.bbcode_enabled = true
	_scrawl_text.fit_content = true
	_scrawl_text.custom_minimum_size = Vector2(680, 0)
	_scrawl_text.anchor_left = 0.5
	_scrawl_text.anchor_right = 0.5
	_scrawl_text.grow_horizontal = Control.GROW_DIRECTION_BOTH
	_scrawl_text.offset_left = -340
	_scrawl_text.offset_right = 340
	_scrawl_text.offset_top = size.y
	_scrawl_text.add_theme_font_size_override("normal_font_size", 19)
	_scrawl_text.add_theme_color_override("default_color", Color(0.85, 0.88, 0.95))
	_scrawl_text.text = "[center]" + "\n\n".join(PackedStringArray(paragraphs)) + "[/center]"
	_scrawl_layer.add_child(_scrawl_text)

	var skip := Label.new()
	skip.text = "Enter — begin"
	skip.add_theme_font_size_override("font_size", 13)
	skip.add_theme_color_override("font_color", Color(0.5, 0.55, 0.65))
	skip.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT)
	skip.offset_left = -160
	skip.offset_top = -36
	_scrawl_layer.add_child(skip)


func _process(delta: float) -> void:
	if _scrawl_text == null or _done:
		return
	_scrawl_text.offset_top -= SCRAWL_SPEED * delta
	# The whole text has climbed past the fold: the story starts itself.
	if _scrawl_text.offset_top + _scrawl_text.size.y < size.y * 0.25:
		_finish(selected_id())


func _finish(character: String) -> void:
	if _done:
		return
	_done = true
	if character != "" and GameState.player_character() != character:
		GameState.set_player_character(character)
	finished.emit()


## The chosen character at 5x, drawn from their sheet's idle-down frame.
class _Portrait extends Control:
	var _tex: Texture2D = null
	var _color := Color(0.7, 0.7, 0.75)
	var _id := ""

	func configure(npc_id: String, npc: Dictionary) -> void:
		_id = npc_id
		_color = StandIn.character_color(npc, npc_id)
		_tex = AssetLibrary.texture("npcs", npc_id + "_sheet")
		queue_redraw()

	func _draw() -> void:
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.08, 0.09, 0.13))
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.42, 0.52, 0.68, 0.7), false, 2.0)
		if _tex != null:
			var frame := Rect2(Vector2.ZERO, CharacterSprite.FRAME)
			var dest_size := CharacterSprite.FRAME * 3.0
			draw_texture_rect_region(_tex,
				Rect2((size - dest_size) * 0.5, dest_size), frame)
		elif _id != "":
			StandIn.paint_character(self, Rect2(size.x * 0.3, size.y * 0.15,
				size.x * 0.4, size.y * 0.7), _color, _id)


class _Pips extends Control:
	var value := 2

	func _draw() -> void:
		for i in 5:
			var rect := Rect2(i * 20, 3, 14, 10)
			if i < value:
				draw_rect(rect, Color(0.95, 0.8, 0.4))
			else:
				draw_rect(rect, Color(0.25, 0.28, 0.35))
				draw_rect(rect, Color(0.4, 0.44, 0.52), false, 1.0)
