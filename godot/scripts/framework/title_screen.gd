extends Control
## Ring 0 — the title screen: the first thing a stranger meets. New Game /
## Continue / Join a Ship / Settings / Quit — nobody deletes JSON by hand
## again. Continue exists only when a save does; Join is the Ship-Share
## client door (SHIP-SHARE.md); Settings persist outside the save.

class_name TitleScreen

signal new_game
signal continue_game
signal join_ship(address: String)

var _menu: VBoxContainer
var _settings_box: Control = null
var _join_box: Control = null


func _ready() -> void:
	set_anchors_preset(Control.PRESET_FULL_RECT)
	var backdrop := ColorRect.new()
	backdrop.color = Color(0.04, 0.05, 0.08)
	backdrop.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(backdrop)
	_star_field(backdrop)

	var title := Label.new()
	title.text = "REACHLOCK"
	title.add_theme_font_size_override("font_size", 64)
	title.add_theme_color_override("font_color", Color(0.92, 0.88, 0.72))
	title.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	title.offset_top = 120
	title.offset_left = -300
	title.offset_right = 300
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	add_child(title)

	var tagline := Label.new()
	tagline.text = "The universe doesn't wait for you."
	tagline.add_theme_font_size_override("font_size", 16)
	tagline.add_theme_color_override("font_color", Color(0.55, 0.6, 0.7))
	tagline.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	tagline.offset_top = 200
	tagline.offset_left = -300
	tagline.offset_right = 300
	tagline.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	add_child(tagline)

	_menu = VBoxContainer.new()
	_menu.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	_menu.offset_top = -40
	_menu.offset_left = -140
	_menu.offset_right = 140
	_menu.add_theme_constant_override("separation", 10)
	add_child(_menu)

	if GameState.has_save():
		_button("Continue", func() -> void: continue_game.emit())
	_button("New Game", _on_new_game)
	_button("Join a Ship", _toggle_join)
	_button("Settings", _toggle_settings)
	_button("Quit", func() -> void: get_tree().quit())

	var version := Label.new()
	version.text = "v" + str(ProjectSettings.get_setting("application/config/version", "?"))
	version.add_theme_font_size_override("font_size", 12)
	version.add_theme_color_override("font_color", Color(0.4, 0.44, 0.52))
	version.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT)
	version.offset_left = -120
	version.offset_top = -30
	add_child(version)


func _button(text: String, on_press: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.add_theme_font_size_override("font_size", 20)
	button.pressed.connect(on_press)
	_menu.add_child(button)
	return button


func _on_new_game() -> void:
	if not GameState.has_save():
		new_game.emit()
		return
	# There's a story on this deck already — starting over erases it, and
	# that deserves a sentence, not a silent wipe.
	var confirm := ConfirmationDialog.new()
	confirm.dialog_text = "Start a new run? The current save — its crew, its debts,\nits standing — will be written over."
	confirm.ok_button_text = "Write over it"
	confirm.cancel_button_text = "Keep it"
	add_child(confirm)
	confirm.confirmed.connect(func() -> void: new_game.emit())
	confirm.canceled.connect(func() -> void: confirm.queue_free())
	confirm.popup_centered()


func _toggle_join() -> void:
	if _join_box != null:
		_join_box.queue_free()
		_join_box = null
		return
	_close_panels()
	var box := PanelContainer.new()
	var column := VBoxContainer.new()
	box.add_child(column)
	var hint := Label.new()
	hint.text = "Host's address (LAN, or Tailscale/ZeroTier for remote):"
	hint.add_theme_font_size_override("font_size", 13)
	column.add_child(hint)
	var row := HBoxContainer.new()
	column.add_child(row)
	var address := LineEdit.new()
	address.placeholder_text = "192.168.1.20"
	address.custom_minimum_size = Vector2(220, 0)
	row.add_child(address)
	var go := Button.new()
	go.text = "Board"
	go.pressed.connect(func() -> void:
		if address.text.strip_edges() != "":
			join_ship.emit(address.text.strip_edges()))
	row.add_child(go)
	address.text_submitted.connect(func(text: String) -> void:
		if text.strip_edges() != "":
			join_ship.emit(text.strip_edges()))
	_menu.add_child(box)
	_join_box = box
	address.grab_focus()


func _toggle_settings() -> void:
	if _settings_box != null:
		_settings_box.queue_free()
		_settings_box = null
		return
	_close_panels()
	_settings_box = SettingsPanel.build()
	_menu.add_child(_settings_box)


func _close_panels() -> void:
	for box in [_settings_box, _join_box]:
		if box != null:
			box.queue_free()
	_settings_box = null
	_join_box = null


func _star_field(parent: Control) -> void:
	var stars := Control.new()
	stars.set_anchors_preset(Control.PRESET_FULL_RECT)
	stars.draw.connect(func() -> void:
		var rng := RandomNumberGenerator.new()
		rng.seed = 8471
		for i in 140:
			var pos := Vector2(rng.randf() * 1920.0, rng.randf() * 1080.0)
			var brightness := rng.randf_range(0.25, 0.9)
			stars.draw_circle(pos, rng.randf_range(0.6, 1.6),
				Color(brightness, brightness, brightness * 1.05)))
	parent.add_child(stars)


## The shared settings UI: title screen and pause menu mount the same box.
class SettingsPanel:
	static func build() -> Control:
		var box := PanelContainer.new()
		var column := VBoxContainer.new()
		column.add_theme_constant_override("separation", 8)
		box.add_child(column)

		var volume_row := HBoxContainer.new()
		column.add_child(volume_row)
		var volume_label := Label.new()
		volume_label.text = "Volume"
		volume_label.custom_minimum_size = Vector2(140, 0)
		volume_row.add_child(volume_label)
		var volume := HSlider.new()
		volume.min_value = 0.0
		volume.max_value = 1.0
		volume.step = 0.05
		volume.value = float(Settings.get_value("master_volume"))
		volume.custom_minimum_size = Vector2(180, 0)
		volume.value_changed.connect(func(value: float) -> void:
			Settings.set_value("master_volume", value))
		volume_row.add_child(volume)

		var fullscreen := CheckBox.new()
		fullscreen.text = "Fullscreen"
		fullscreen.button_pressed = bool(Settings.get_value("fullscreen"))
		fullscreen.toggled.connect(func(on: bool) -> void:
			Settings.set_value("fullscreen", on))
		column.add_child(fullscreen)

		var cps_row := HBoxContainer.new()
		column.add_child(cps_row)
		var cps_label := Label.new()
		cps_label.text = "Text speed"
		cps_label.custom_minimum_size = Vector2(140, 0)
		cps_row.add_child(cps_label)
		var cps := HSlider.new()
		cps.min_value = 20.0
		cps.max_value = 120.0
		cps.step = 5.0
		cps.value = float(Settings.get_value("typewriter_cps"))
		cps.custom_minimum_size = Vector2(180, 0)
		cps.value_changed.connect(func(value: float) -> void:
			Settings.set_value("typewriter_cps", value))
		cps_row.add_child(cps)

		# The keybind card: display-only this sprint, one place to look.
		var keys := Label.new()
		keys.add_theme_font_size_override("font_size", 12)
		keys.add_theme_color_override("font_color", Color(0.6, 0.65, 0.75))
		keys.text = "\n".join([
			"WASD — move · R — interact · F — mine",
			"Space — brake · Shift — boost · Ctrl — fire",
			"G — decoy · H — hail · J — jump · B — board",
			"V — hold to speak (needs the voice daemon)",
			"Esc — pause",
		])
		column.add_child(keys)
		return box
