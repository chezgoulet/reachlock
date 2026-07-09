extends Control
## Ring 0 — DialoguePanel: THE dialogue surface (Sprint 3). Every host mounts
## one on its HUD layer; conversation text no longer shares a scrolling log
## with system noise. One isolated element: nameplate, mind-status lamp,
## a typewriter body, and the choice list.
##
## The typewriter prints character by character (SNES cadence) — which is
## also the latency mask's best friend: a buffer line is still printing
## while the mind composes, so generation hides inside presentation.
## Interact/click fast-forwards the current line.
##
## The status lamp tells the player up front what kind of wait this is:
##   SCRIPTED    — authored lines only, instant
##   MIND LINKED — a live mind; generated beats may take a few seconds
##   COMPOSING…  — the mind is working right now (pulses)
##   LINK OFFLINE— no daemon; fallback lines only
##
## Narration mode shows a second-person card (used when a scene can't play
## because the player IS its speaker) with a single continue prompt.

class_name DialoguePanel

signal choice_picked(index: int)
signal narration_done

const CPS := 55.0            # characters per second, the SNES cadence
const LINE_GAP := 0.30       # beat between queued lines, seconds
const BARK_SECONDS := 4.5    # transient one-liners auto-hide

const LAMP_STATES := {
	"scripted": {"color": Color(0.55, 0.62, 0.75), "label": "SCRIPTED"},
	"linked": {"color": Color(0.45, 0.85, 0.55), "label": "MIND LINKED"},
	"composing": {"color": Color(1.0, 0.75, 0.35), "label": "COMPOSING…"},
	"offline": {"color": Color(0.5, 0.5, 0.55), "label": "LINK OFFLINE"},
}

var _name_label: Label
var _lamp: _Lamp
var _body: RichTextLabel
var _choice_box: VBoxContainer
var _continue_hint: Label

var _queue: Array = []          # [{speaker, text}]
var _typing := false
var _type_clock := 0.0
var _gap_left := 0.0
var _pending_choices: Array = []
var _choices_up := false
var _link_state := "scripted"
var _thinking := false
var _narrating := false
var _bark_timer := 0.0
var _open := false


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	offset_left = 120
	offset_right = -120
	offset_top = -230
	offset_bottom = -16
	mouse_filter = Control.MOUSE_FILTER_IGNORE

	var frame := PanelContainer.new()
	frame.name = "Frame"
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0.055, 0.065, 0.095, 0.94)
	style.border_color = Color(0.42, 0.52, 0.68, 0.8)
	style.set_border_width_all(2)
	style.set_corner_radius_all(4)
	style.set_content_margin_all(12)
	frame.add_theme_stylebox_override("panel", style)
	frame.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(frame)

	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 6)
	frame.add_child(box)

	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 10)
	box.add_child(header)
	_name_label = Label.new()
	_name_label.add_theme_font_size_override("font_size", 17)
	_name_label.add_theme_color_override("font_color", Color(0.95, 0.9, 0.7))
	_name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(_name_label)
	_lamp = _Lamp.new()
	_lamp.custom_minimum_size = Vector2(150, 18)
	header.add_child(_lamp)

	_body = RichTextLabel.new()
	_body.bbcode_enabled = true
	_body.scroll_following = true
	_body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_body.custom_minimum_size = Vector2(0, 96)
	_body.add_theme_font_size_override("normal_font_size", 16)
	_body.add_theme_font_size_override("italics_font_size", 16)
	_body.add_theme_font_size_override("bold_font_size", 16)
	box.add_child(_body)

	_choice_box = VBoxContainer.new()
	_choice_box.add_theme_constant_override("separation", 2)
	box.add_child(_choice_box)

	_continue_hint = Label.new()
	_continue_hint.text = "R — continue"
	_continue_hint.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_continue_hint.add_theme_font_size_override("font_size", 12)
	_continue_hint.add_theme_color_override("font_color", Color(0.55, 0.6, 0.7))
	_continue_hint.visible = false
	box.add_child(_continue_hint)

	visible = false


## --- host API ---------------------------------------------------------------------


## Begin a conversation. `link_state`: scripted | linked | offline — what kind
## of wait the player should expect, shown before the first line lands.
func open(npc_name: String, link_state: String) -> void:
	_open = true
	_narrating = false
	_bark_timer = 0.0
	_link_state = link_state
	_name_label.text = npc_name
	_body.clear()
	_queue.clear()
	_typing = false
	_clear_choices()
	_refresh_lamp()
	visible = true


func show_line(speaker: String, text: String) -> void:
	if not _open:
		return
	_bark_timer = 0.0
	_queue.append({"speaker": speaker, "text": text})
	if not _typing and _gap_left <= 0.0:
		_next_line()


## Choices arrive from the runner; they render once the current line has
## finished printing (or on fast-forward).
func show_choices(choices: Array) -> void:
	_pending_choices = choices
	if not _typing:
		_render_choices()


func set_thinking(thinking: bool) -> void:
	_thinking = thinking
	_refresh_lamp()


## A transient one-liner outside a conversation (crew callouts, ambient
## barks). Opens the panel briefly and hides itself.
func bark(speaker: String, text: String) -> void:
	if _open and _bark_timer <= 0.0:
		return  # a real conversation owns the panel
	if not visible:
		_name_label.text = speaker
		_body.clear()
		_queue.clear()
		_link_state = "scripted"
		_refresh_lamp()
		visible = true
	_queue.append({"speaker": speaker, "text": text})
	if not _typing:
		_next_line()
	_bark_timer = BARK_SECONDS


## A second-person card standing in for a scene the player IS the speaker of.
func show_narration(title: String, text: String) -> void:
	_open = true
	_narrating = true
	_name_label.text = title
	_link_state = "scripted"
	_refresh_lamp()
	_body.clear()
	_queue.clear()
	_clear_choices()
	_queue.append({"speaker": "", "text": "[i]%s[/i]" % text})
	_next_line()
	visible = true


func close() -> void:
	_open = false
	_narrating = false
	_typing = false
	_queue.clear()
	_clear_choices()
	visible = false


func is_open() -> bool:
	return _open


## The whole current line at once (tests, accessibility).
func fast_forward() -> void:
	if _typing:
		_body.visible_characters = -1
		_typing = false
		_after_line()


## --- typewriter -------------------------------------------------------------------


func _next_line() -> void:
	if _queue.is_empty():
		return
	var line: Dictionary = _queue.pop_front()
	var speaker: String = line.speaker
	if _body.get_parsed_text() != "":
		_body.append_text("\n")
	if speaker != "":
		_body.append_text("[b]%s[/b] — " % speaker)
	_body.append_text(str(line.text))
	# Everything already shown stays shown; only the new tail types out.
	var before := _body.get_total_character_count() - str(line.text).length()
	_body.visible_characters = before
	_typing = true
	_type_clock = float(before)


func _process(delta: float) -> void:
	if _bark_timer > 0.0 and not _narrating:
		_bark_timer -= delta
		if _bark_timer <= 0.0 and _queue.is_empty() and not _typing and not _open:
			visible = false
	if _gap_left > 0.0:
		_gap_left -= delta
		if _gap_left <= 0.0 and not _queue.is_empty():
			_next_line()
	if not _typing:
		return
	_type_clock += delta * CPS
	var total := _body.get_total_character_count()
	if int(_type_clock) >= total:
		_body.visible_characters = -1
		_typing = false
		_after_line()
	else:
		_body.visible_characters = int(_type_clock)


func _after_line() -> void:
	if _narrating:
		_continue_hint.visible = true
		return
	if not _queue.is_empty():
		_gap_left = LINE_GAP
		return
	if not _pending_choices.is_empty():
		_render_choices()


func _input(event: InputEvent) -> void:
	if not visible:
		return
	if event.is_action_pressed("interact") or event.is_action_pressed("ui_accept"):
		if _typing:
			accept_event()
			fast_forward()
		elif _narrating and _continue_hint.visible:
			accept_event()
			_continue_hint.visible = false
			_narrating = false
			narration_done.emit()
			close()
	elif event is InputEventKey and event.pressed and _choices_up:
		var index: int = (event as InputEventKey).keycode - KEY_1
		if index >= 0 and index < _choice_box.get_child_count():
			accept_event()
			_pick(index)


## --- choices ----------------------------------------------------------------------


func _render_choices() -> void:
	_clear_choices()
	for choice: Dictionary in _pending_choices:
		var button := Button.new()
		var number := _choice_box.get_child_count() + 1
		button.text = "%d.  %s" % [number, choice.text]
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.flat = true
		button.add_theme_font_size_override("font_size", 15)
		button.add_theme_color_override("font_color", Color(0.8, 0.87, 1.0))
		var index := int(choice.index)
		button.pressed.connect(func() -> void: _pick_by_runner_index(index))
		_choice_box.add_child(button)
	_choices_up = not _pending_choices.is_empty()
	_pending_choices = []


func _pick(child_position: int) -> void:
	var button := _choice_box.get_child(child_position) as Button
	if button != null:
		button.emit_signal("pressed")


func _pick_by_runner_index(index: int) -> void:
	_clear_choices()
	choice_picked.emit(index)


func _clear_choices() -> void:
	_choices_up = false
	for child in _choice_box.get_children():
		child.queue_free()


## --- the lamp ---------------------------------------------------------------------


func _refresh_lamp() -> void:
	var state := _link_state
	if _thinking:
		state = "composing"
	_lamp.set_state(state, LAMP_STATES.get(state, LAMP_STATES.scripted))


## Current lamp state name (contract-testable).
func lamp_state() -> String:
	return "composing" if _thinking else _link_state


func is_typing() -> bool:
	return _typing


class _Lamp extends Control:
	var _state := "scripted"
	var _color := Color(0.55, 0.62, 0.75)
	var _label := "SCRIPTED"
	var _clock := 0.0

	func set_state(state: String, spec: Dictionary) -> void:
		_state = state
		_color = spec.get("color", _color)
		_label = spec.get("label", _label)
		queue_redraw()

	func _process(delta: float) -> void:
		if _state == "composing":
			_clock += delta
			queue_redraw()

	func _draw() -> void:
		var c := _color
		if _state == "composing":
			c.a = 0.55 + 0.45 * (0.5 + 0.5 * sin(_clock * 6.0))
		draw_circle(Vector2(8, size.y * 0.5), 5.0, c)
		draw_string(ThemeDB.fallback_font, Vector2(20, size.y * 0.5 + 5), _label,
			HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(0.75, 0.78, 0.85))
