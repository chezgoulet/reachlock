extends RefCounted
## Ring 0 — StationMinigames: the interactive consoles behind the ship's
## stations. Walking to a station and pressing R opens its panel (Stardew
## register, not a text menu):
##
##   engineering — the power routing grid: slide the drive's output between
##                 weapons, shields, and engines. The allocation persists
##                 (GameState.player.ship.power) and the next flight flies
##                 it: engines are speed, weapons are fire rate, shields
##                 soak hits.
##   scanner     — a sweeping radar scope reading the CURRENT space's data:
##                 minable rocks, patrol contacts by faction, the dock, a
##                 charted self-jump route.
##   cargo       — the manifest: hold contents with sprites, hull state,
##                 owned upgrades.
##   weapons     — the gunnery check: hardpoints, fire-rate/damage numbers
##                 the next fight will actually use.
##
## All panels are built from live data; none name a content id. The host
## closes them via the returned panel's `closed` signal (Esc or button).

class_name StationMinigames

const PANEL_BG := Color(0.07, 0.08, 0.11, 0.96)
const PANEL_BORDER := Color(0.45, 0.55, 0.70, 0.7)


## Build the panel for a station id, or null when the station has no
## minigame (the host falls back to its one-line interaction).
static func build(station_id: String) -> Control:
	match station_id:
		"engineering":
			return _power_grid()
		"scanner":
			return _radar_scope()
		"cargo":
			return _manifest()
		"weapons":
			return _gunnery()
	return null


static func _frame(title: String) -> PanelContainer:
	var panel := _ClosablePanel.new()
	var style := StyleBoxFlat.new()
	style.bg_color = PANEL_BG
	style.border_color = PANEL_BORDER
	style.set_border_width_all(2)
	style.set_content_margin_all(14)
	panel.add_theme_stylebox_override("panel", style)
	panel.set_anchors_preset(Control.PRESET_CENTER)
	panel.custom_minimum_size = Vector2(520, 360)

	var box := VBoxContainer.new()
	box.name = "Box"
	box.add_theme_constant_override("separation", 10)
	panel.add_child(box)

	var header := HBoxContainer.new()
	box.add_child(header)
	var label := Label.new()
	label.text = title
	label.add_theme_font_size_override("font_size", 20)
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(label)
	var close := Button.new()
	close.text = "Close  (Esc)"
	close.pressed.connect(func() -> void: panel.close())
	header.add_child(close)
	return panel


static func _content(panel: PanelContainer) -> VBoxContainer:
	return panel.get_node("Box") as VBoxContainer


## A panel that closes itself on ui_cancel and announces it.
class _ClosablePanel extends PanelContainer:
	signal closed

	func _input(event: InputEvent) -> void:
		if event.is_action_pressed("ui_cancel"):
			accept_event()
			close()

	func close() -> void:
		closed.emit()
		queue_free()


## --- engineering: the power routing grid ----------------------------------------


static func _power_grid() -> Control:
	var panel := _frame("Power Routing — Drive Output")
	var box := _content(panel)

	var blurb := Label.new()
	blurb.text = "One drive, three hungers. What you feed here is what the ship is out there."
	blurb.add_theme_font_size_override("font_size", 13)
	blurb.add_theme_color_override("font_color", Color(0.62, 0.66, 0.74))
	blurb.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	box.add_child(blurb)

	var channels := [
		["weapons", "Weapons", Color(0.92, 0.35, 0.30), "fire rate"],
		["shields", "Shields", Color(0.35, 0.75, 0.92), "damage soak"],
		["engines", "Engines", Color(1.0, 0.73, 0.33), "top speed"],
	]
	var bars := {}
	var refresh: Callable
	for entry: Array in channels:
		var channel: String = entry[0]
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 8)
		box.add_child(row)
		var name_label := Label.new()
		name_label.text = entry[1]
		name_label.custom_minimum_size = Vector2(90, 0)
		row.add_child(name_label)
		var minus := Button.new()
		minus.text = "–"
		minus.custom_minimum_size = Vector2(36, 0)
		row.add_child(minus)
		var bar := ProgressBar.new()
		bar.min_value = 0.0
		bar.max_value = 0.9
		bar.show_percentage = false
		bar.custom_minimum_size = Vector2(220, 22)
		bar.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var fill := StyleBoxFlat.new()
		fill.bg_color = entry[2]
		bar.add_theme_stylebox_override("fill", fill)
		row.add_child(bar)
		var pct := Label.new()
		pct.custom_minimum_size = Vector2(48, 0)
		row.add_child(pct)
		var plus := Button.new()
		plus.text = "+"
		plus.custom_minimum_size = Vector2(36, 0)
		row.add_child(plus)
		var effect := Label.new()
		effect.text = entry[3]
		effect.add_theme_font_size_override("font_size", 12)
		effect.add_theme_color_override("font_color", Color(0.55, 0.58, 0.66))
		row.add_child(effect)
		bars[channel] = {"bar": bar, "pct": pct}
		minus.pressed.connect(func() -> void:
			GameState.set_power_share(channel, GameState.power_share(channel) - 0.1)
			AudioManager.play("ui_click", 0.7)
			refresh.call())
		plus.pressed.connect(func() -> void:
			GameState.set_power_share(channel, GameState.power_share(channel) + 0.1)
			AudioManager.play("ui_click", 0.7)
			refresh.call())

	refresh = func() -> void:
		for channel: String in bars:
			var share := GameState.power_share(channel)
			(bars[channel].bar as ProgressBar).value = share
			(bars[channel].pct as Label).text = "%d%%" % int(round(share * 100))
	refresh.call()
	return panel


## --- scanner: the sweeping scope -------------------------------------------------


static func _radar_scope() -> Control:
	var panel := _frame("Sensor Scope — Passive Sweep")
	var box := _content(panel)
	var scope := _Scope.new()
	scope.custom_minimum_size = Vector2(480, 300)
	scope.configure(DataRegistry.get_entity("locations", GameState.current_space()))
	box.add_child(scope)
	return panel


class _Scope extends Control:
	var _location: Dictionary = {}
	var _blips: Array = []  # {pos:Vector2(norm), color, label}
	var _t := 0.0

	func configure(location: Dictionary) -> void:
		_location = location
		var rng := RandomNumberGenerator.new()
		rng.seed = hash(location.get("id", "scope"))
		if location.has("mining"):
			for i in 6:
				_blips.append({"pos": _ring_pos(rng, 0.35, 0.8), "color": Color(0.75, 0.65, 0.4),
					"label": "rock" if i == 0 else ""})
		for patrol: Dictionary in location.get("patrols", []):
			var faction: Dictionary = DataRegistry.get_entity("factions", patrol.get("faction", ""))
			var color := Color.from_string(str(patrol.get("color", "")), Color(0.8, 0.3, 0.3))
			for i in int(patrol.get("count", 1)):
				_blips.append({"pos": _ring_pos(rng, 0.5, 0.95), "color": color,
					"label": faction.get("short_name", faction.get("name", "contact")) if i == 0 else ""})
		if "dock" in location.get("services", []):
			_blips.append({"pos": Vector2(0.72, 0.35), "color": Color(0.4, 0.9, 0.55),
				"label": str(location.get("name", "dock"))})
		if location.has("self_jump"):
			_blips.append({"pos": Vector2(0.2, 0.75), "color": Color(0.55, 0.75, 1.0),
				"label": str(location.get("self_jump", {}).get("name", "jump route"))})

	func _ring_pos(rng: RandomNumberGenerator, r0: float, r1: float) -> Vector2:
		var theta := rng.randf() * TAU
		var r := rng.randf_range(r0, r1) * 0.5
		return Vector2(0.5 + cos(theta) * r, 0.5 + sin(theta) * r)

	func _process(delta: float) -> void:
		_t += delta
		queue_redraw()

	func _draw() -> void:
		var center := size * 0.5
		var radius := minf(size.x, size.y) * 0.48
		draw_circle(center, radius, Color(0.04, 0.10, 0.07))
		for i in range(1, 4):
			draw_arc(center, radius * i / 3.0, 0, TAU, 48, Color(0.2, 0.5, 0.3, 0.5), 1.0)
		draw_line(center - Vector2(radius, 0), center + Vector2(radius, 0), Color(0.2, 0.5, 0.3, 0.3))
		draw_line(center - Vector2(0, radius), center + Vector2(0, radius), Color(0.2, 0.5, 0.3, 0.3))
		# The sweep
		var sweep_angle := fmod(_t * 1.2, TAU)
		for trail in 24:
			var a := sweep_angle - trail * 0.03
			draw_line(center, center + Vector2.RIGHT.rotated(a) * radius,
				Color(0.3, 0.9, 0.5, 0.5 * (1.0 - trail / 24.0)), 2.0)
		# Blips glow when the sweep has recently passed them
		for blip: Dictionary in _blips:
			var world := Vector2((blip.pos as Vector2).x * size.x, (blip.pos as Vector2).y * size.y)
			var offset := world - center
			if offset.length() > radius - 6:
				offset = offset.normalized() * (radius - 10)
				world = center + offset
			var blip_angle := fposmod(offset.angle(), TAU)
			var since := fposmod(sweep_angle - blip_angle, TAU)
			var glow := clampf(1.0 - since / 2.4, 0.15, 1.0)
			var color: Color = blip.color
			color.a = glow
			draw_circle(world, 4.0, color)
			if blip.label != "" and glow > 0.4:
				draw_string(ThemeDB.fallback_font, world + Vector2(7, 4), str(blip.label),
					HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(0.7, 0.95, 0.75, glow))


## --- cargo: the manifest ---------------------------------------------------------


static func _manifest() -> Control:
	var panel := _frame("Cargo Manifest")
	var box := _content(panel)

	var hull := Label.new()
	hull.text = "Hull integrity %d%%    ·    Credits %d" % [
		int(GameState.player.ship.hull_integrity * 100.0), int(GameState.player.credits)]
	box.add_child(hull)

	var cargo: Dictionary = GameState.player.ship.cargo
	if cargo.is_empty():
		var empty := Label.new()
		empty.text = "The hold is empty. The hold hates being empty."
		empty.add_theme_color_override("font_color", Color(0.6, 0.62, 0.68))
		box.add_child(empty)
	for good_id: String in cargo:
		var good := DataRegistry.get_entity("goods", good_id)
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 10)
		box.add_child(row)
		var icon := TextureRect.new()
		icon.texture = AssetLibrary.texture("props", "crate")
		icon.custom_minimum_size = Vector2(28, 24)
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		row.add_child(icon)
		var line := Label.new()
		line.text = "%s × %d" % [good.get("name", good_id), int(cargo[good_id])]
		row.add_child(line)

	var owned: Array = GameState.player.get("upgrades", [])
	if not owned.is_empty():
		var fitted := Label.new()
		fitted.text = "Fitted: " + ", ".join(owned.map(func(u: String) -> String:
			return DataRegistry.get_entity("upgrades", u).get("name", u)))
		fitted.add_theme_font_size_override("font_size", 13)
		fitted.add_theme_color_override("font_color", Color(0.65, 0.72, 0.6))
		fitted.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		box.add_child(fitted)
	return panel


## --- weapons: the gunnery check ---------------------------------------------------


static func _gunnery() -> Control:
	var panel := _frame("Gunnery — Systems Check")
	var box := _content(panel)

	var hull := DataRegistry.get_entity("ships", GameState.player.ship.hull_id)
	var hardpoints := int(hull.get("hardpoints", {}).get("weapons", 0))
	var damage := 1 + int(GameState.upgrade_effect_sum("damage_bonus"))
	var weapons_power := GameState.power_share("weapons")

	var stats := Label.new()
	stats.text = "Hardpoints: %d    Slug damage: %d    Fire-rate feed: %d%%" % [
		hardpoints, damage, int(round(weapons_power * 100))]
	box.add_child(stats)

	var note := Label.new()
	note.text = "Dry-fire only in the hold — Tove's rule after the incident.\nCTRL fires in flight; the reticle leads for you at close range."
	note.add_theme_font_size_override("font_size", 13)
	note.add_theme_color_override("font_color", Color(0.62, 0.66, 0.74))
	box.add_child(note)

	var reticle := _Reticle.new()
	reticle.custom_minimum_size = Vector2(480, 220)
	box.add_child(reticle)
	return panel


class _Reticle extends Control:
	var _t := 0.0

	func _process(delta: float) -> void:
		_t += delta
		queue_redraw()

	func _draw() -> void:
		var center := size * 0.5
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.03, 0.05, 0.08))
		# A drifting practice target and the tracking reticle chasing it.
		var target := center + Vector2(sin(_t * 0.9) * size.x * 0.3, cos(_t * 1.3) * size.y * 0.25)
		draw_circle(target, 7, Color(0.85, 0.4, 0.3))
		draw_circle(target, 7, Color(0.95, 0.6, 0.4), false, 1.5)
		var lag := center + (target - center) * 0.85
		var pulse := 0.6 + 0.4 * sin(_t * 8.0)
		draw_arc(lag, 14, 0, TAU, 24, Color(1.0, 0.85, 0.4, pulse), 1.5)
		draw_line(lag - Vector2(20, 0), lag - Vector2(8, 0), Color(1.0, 0.85, 0.4, pulse))
		draw_line(lag + Vector2(8, 0), lag + Vector2(20, 0), Color(1.0, 0.85, 0.4, pulse))
		draw_line(lag - Vector2(0, 20), lag - Vector2(0, 8), Color(1.0, 0.85, 0.4, pulse))
		draw_line(lag + Vector2(0, 8), lag + Vector2(0, 20), Color(1.0, 0.85, 0.4, pulse))
