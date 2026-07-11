extends CanvasLayer
## Ring 0 — the pause menu. Esc pauses the tree; the menu itself keeps
## processing. Esc-vs-panel ordering (the sprint's noted trap): while a
## conversation is open the DialoguePanel owns the moment — Esc does
## nothing here until the panel closes.
##
## "Open the hatch" is Ship-Share hosting as one button: press it, the
## boat listens on the LAN, friends board with the title screen's Join.

class_name PauseMenu

signal quit_to_title

var _panel: Control = null
var _settings_box: Control = null
var _host_label: Label = null


func _ready() -> void:
	layer = 95
	process_mode = Node.PROCESS_MODE_ALWAYS


func _unhandled_input(event: InputEvent) -> void:
	if not (event is InputEventKey and event.pressed and not event.echo):
		return
	if (event as InputEventKey).keycode != KEY_ESCAPE:
		return
	# The panel owns the moment: a conversation on screen swallows Esc.
	for panel in get_tree().get_nodes_in_group("dialogue_surface"):
		if panel is DialoguePanel and (panel as DialoguePanel).is_open():
			return
	toggle()


func is_paused() -> bool:
	return _panel != null


func toggle() -> void:
	if is_paused():
		_close()
	else:
		_open()


func _open() -> void:
	get_tree().paused = true
	_panel = Control.new()
	_panel.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	var dim := ColorRect.new()
	dim.color = Color(0.02, 0.03, 0.05, 0.72)
	dim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_panel.add_child(dim)

	var column := VBoxContainer.new()
	column.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	column.offset_left = -140
	column.offset_right = 140
	column.offset_top = -120
	column.add_theme_constant_override("separation", 8)
	_panel.add_child(column)

	var heading := Label.new()
	heading.text = "PAUSED"
	heading.add_theme_font_size_override("font_size", 28)
	heading.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	column.add_child(heading)

	_add_button(column, "Resume", _close)
	_add_button(column, "Save", func() -> void:
		GameState.save_game())
	if ShipShare.mode == ShipShare.Mode.SOLO:
		_add_button(column, "Open the Hatch (host LAN)", func() -> void:
			if ShipShare.host():
				_host_label.text = "Hatch open — friends board via your address, port %d" % ShipShare.DEFAULT_PORT
			else:
				_host_label.text = "Could not open the hatch (port busy?)")
	_host_label = Label.new()
	_host_label.add_theme_font_size_override("font_size", 12)
	_host_label.add_theme_color_override("font_color", Color(0.55, 0.75, 0.6))
	_host_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	if ShipShare.mode == ShipShare.Mode.HOSTING:
		_host_label.text = "Hatch open — %d aboard" % ShipShare.players.size()
	elif ShipShare.mode == ShipShare.Mode.JOINED:
		_host_label.text = "Aboard a friend's boat"
	column.add_child(_host_label)
	_add_button(column, "Settings", func() -> void: _toggle_settings(column))
	_add_button(column, "Quit to Title", func() -> void:
		_close()
		quit_to_title.emit())
	_add_button(column, "Quit to Desktop", func() -> void:
		GameState.save_game()
		get_tree().quit())

	add_child(_panel)


func _toggle_settings(column: VBoxContainer) -> void:
	if _settings_box != null:
		_settings_box.queue_free()
		_settings_box = null
		return
	_settings_box = TitleScreen.SettingsPanel.build()
	column.add_child(_settings_box)


func _add_button(column: VBoxContainer, text: String, on_press: Callable) -> void:
	var button := Button.new()
	button.text = text
	button.add_theme_font_size_override("font_size", 18)
	button.pressed.connect(on_press)
	column.add_child(button)


func _close() -> void:
	get_tree().paused = false
	if _panel != null:
		_panel.queue_free()
		_panel = null
	_settings_box = null
