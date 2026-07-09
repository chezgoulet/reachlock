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
## minigame (the host falls back to its one-line interaction). `station`
## is the hull's station entry — data like `converts` rides on it.
static func build(station_id: String, station: Dictionary = {}) -> Control:
	match station_id:
		"engineering":
			return _power_grid()
		"scanner":
			return _radar_scope()
		"cargo":
			return _manifest()
		"weapons":
			return _gunnery()
		"processing":
			return _ore_processor(station)
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
	var panel := _frame("Sensor Scope")
	var box := _content(panel)
	var scope := _Scope.new()
	scope.custom_minimum_size = Vector2(480, 270)
	scope.configure(DataRegistry.get_entity("locations", GameState.current_space()))
	box.add_child(scope)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 10)
	box.add_child(row)
	var ping := Button.new()
	ping.text = "Active Ping"
	row.add_child(ping)
	var note := Label.new()
	note.text = "Passive sweep glows what it passes. A ping lights everything — and everything hears a ping."
	note.add_theme_font_size_override("font_size", 12)
	note.add_theme_color_override("font_color", Color(0.55, 0.62, 0.58))
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(note)
	ping.pressed.connect(func() -> void:
		AudioManager.play("computer_noise", 1.1)
		scope.ping())
	return panel


class _Scope extends Control:
	var _location: Dictionary = {}
	var _blips: Array = []  # {pos:Vector2(norm), color, label}
	var _t := 0.0
	var _ping_at := -100.0

	## Active ping: an expanding pulse lights every contact at once.
	func ping() -> void:
		_ping_at = _t

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
		# The ping: an expanding ring that lights the whole board for a beat.
		var ping_age := _t - _ping_at
		if ping_age < 2.5:
			draw_arc(center, minf(radius, ping_age * radius * 0.8), 0, TAU, 64,
				Color(0.4, 1.0, 0.6, clampf(1.0 - ping_age / 2.5, 0.0, 0.8)), 2.0)
		# Blips glow when the sweep has recently passed them (or a ping did)
		for blip: Dictionary in _blips:
			var world := Vector2((blip.pos as Vector2).x * size.x, (blip.pos as Vector2).y * size.y)
			var offset := world - center
			if offset.length() > radius - 6:
				offset = offset.normalized() * (radius - 10)
				world = center + offset
			var blip_angle := fposmod(offset.angle(), TAU)
			var since := fposmod(sweep_angle - blip_angle, TAU)
			var glow := clampf(1.0 - since / 2.4, 0.15, 1.0)
			if ping_age < 3.5:
				glow = maxf(glow, clampf(1.0 - ping_age / 3.5, 0.0, 1.0))
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
	var panel := _frame("Gunnery — Calibration Range")
	var box := _content(panel)

	var hull := DataRegistry.get_entity("ships", GameState.player.ship.hull_id)
	var hardpoints := int(hull.get("hardpoints", {}).get("weapons", 0))
	var damage := 1 + int(GameState.upgrade_effect_sum("damage_bonus"))
	var weapons_power := GameState.power_share("weapons")

	var stats := Label.new()
	stats.text = "Hardpoints: %d    Slug damage: %d    Fire-rate feed: %d%%" % [
		hardpoints, damage, int(round(weapons_power * 100))]
	box.add_child(stats)

	var status := Label.new()
	status.add_theme_font_size_override("font_size", 13)
	box.add_child(status)

	var range_view := _CalibrationRange.new()
	range_view.custom_minimum_size = Vector2(480, 220)
	box.add_child(range_view)

	var refresh := func() -> void:
		if GameState.player.ship.get("weapons_calibrated", false):
			status.text = "CALIBRATED — the next flight's guns cycle 15% faster."
			status.add_theme_color_override("font_color", Color(0.5, 0.95, 0.6))
		else:
			status.text = "Click the drone when the reticle bites — %d/%d hits. A calibrated feed cycles 15%% faster next flight." % [
				range_view.hits(), range_view.HITS_NEEDED]
			status.add_theme_color_override("font_color", Color(0.62, 0.66, 0.74))
	range_view.progressed.connect(refresh)
	refresh.call()
	return panel


## Click the drifting practice drone while the tracking reticle overlaps it.
## Land the streak and the gun feed is calibrated: a real fire-rate edge,
## consumed by the next flight.
class _CalibrationRange extends Control:
	signal progressed

	const HITS_NEEDED := 3

	var _t := 0.0
	var _hits := 0
	var _flash := 0.0

	func hits() -> int:
		return _hits

	func _target_pos() -> Vector2:
		var center := size * 0.5
		return center + Vector2(sin(_t * 0.9) * size.x * 0.3, cos(_t * 1.3) * size.y * 0.25)

	func _reticle_pos() -> Vector2:
		var center := size * 0.5
		return center + (_target_pos() - center) * 0.85

	func _gui_input(event: InputEvent) -> void:
		if GameState.player.ship.get("weapons_calibrated", false):
			return
		if event is InputEventMouseButton and event.pressed \
				and event.button_index == MOUSE_BUTTON_LEFT:
			var on_target := (event.position as Vector2).distance_to(_target_pos()) < 16.0
			var reticle_bites := _reticle_pos().distance_to(_target_pos()) < 14.0
			if on_target and reticle_bites:
				_hits += 1
				_flash = 0.25
				AudioManager.play("ui_click", 1.2)
				if _hits >= HITS_NEEDED:
					GameState.set_weapons_calibrated(true)
					AudioManager.play("ui_switch")
			else:
				_hits = maxi(0, _hits - 1)  # a spooked drone resets your streak
				AudioManager.play("ui_click", 0.6)
			progressed.emit()

	func _process(delta: float) -> void:
		_t += delta
		_flash = maxf(0.0, _flash - delta)
		queue_redraw()

	func _draw() -> void:
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.03, 0.05, 0.08))
		var calibrated: bool = GameState.player.ship.get("weapons_calibrated", false)
		var target := _target_pos()
		var target_color := Color(0.4, 0.9, 0.55) if calibrated else Color(0.85, 0.4, 0.3)
		draw_circle(target, 7, target_color)
		draw_circle(target, 7, target_color.lightened(0.3), false, 1.5)
		if _flash > 0.0:
			draw_arc(target, 22, 0, TAU, 24, Color(1, 1, 0.8, _flash * 3.0), 2.0)
		var lag := _reticle_pos()
		var pulse := 0.6 + 0.4 * sin(_t * 8.0)
		var reticle_color := Color(1.0, 0.85, 0.4, pulse)
		draw_arc(lag, 14, 0, TAU, 24, reticle_color, 1.5)
		draw_line(lag - Vector2(20, 0), lag - Vector2(8, 0), reticle_color)
		draw_line(lag + Vector2(8, 0), lag + Vector2(20, 0), reticle_color)
		draw_line(lag - Vector2(0, 20), lag - Vector2(0, 8), reticle_color)
		draw_line(lag + Vector2(0, 8), lag + Vector2(0, 20), reticle_color)
		for i in _hits:
			draw_circle(Vector2(14 + i * 16, 14), 5,
				Color(0.5, 0.95, 0.6) if calibrated or i < _hits else Color(0.3, 0.4, 0.35))


## --- ore processing: the crusher press ---------------------------------------------


## Time the press: the marker sweeps, the seam band is the sweet spot.
## Hit it and the charge refines clean; jam it and you grind good rock to
## dust. `converts` on the station entry names the goods and base ratio —
## the engine only ever sees "from", "to", and numbers.
static func _ore_processor(station: Dictionary) -> Control:
	var converts: Dictionary = station.get("converts", {})
	var from_id: String = converts.get("from", "")
	var to_id: String = converts.get("to", "")
	var ratio := maxi(1, int(converts.get("ratio", 2)))
	var panel := _frame("Ore Processor — Crusher Press")
	var box := _content(panel)

	if from_id == "" or to_id == "":
		var idle := Label.new()
		idle.text = "The processor idles. Nothing routed to the hopper."
		box.add_child(idle)
		return panel

	var from_name: String = DataRegistry.get_entity("goods", from_id).get("name", from_id)
	var to_name: String = DataRegistry.get_entity("goods", to_id).get("name", to_id)

	var counts := Label.new()
	box.add_child(counts)
	var status := Label.new()
	status.add_theme_font_size_override("font_size", 13)
	status.add_theme_color_override("font_color", Color(0.62, 0.66, 0.74))
	box.add_child(status)

	var press := _CrusherPress.new()
	press.custom_minimum_size = Vector2(480, 90)
	press.zone_width = clampf(0.10 + 0.03 * float(GameState.player_stat("engineering") - 2),
		0.06, 0.24)
	box.add_child(press)

	var button := Button.new()
	button.text = "CRUSH  (Space)"
	box.add_child(button)
	button.grab_focus.call_deferred()

	var refresh := func() -> void:
		counts.text = "%s in the hopper: %d      %s out: %d" % [
			from_name, GameState.cargo_count(from_id), to_name, GameState.cargo_count(to_id)]
		button.disabled = GameState.cargo_count(from_id) < 1

	button.pressed.connect(func() -> void:
		var have := GameState.cargo_count(from_id)
		if have < 1:
			return
		if press.strike():
			# Clean cut: a full charge refines even short-fed.
			var used := mini(ratio, have)
			GameState.add_cargo(from_id, -used)
			GameState.add_cargo(to_id, 1)
			AudioManager.play("ui_switch")
			status.text = "Clean cut — %d %s pressed into 1 %s." % [used, from_name, to_name]
		else:
			GameState.add_cargo(from_id, -1)
			AudioManager.play("ui_click", 0.5)
			status.text = "Off the seam. The crusher grinds 1 %s to dust." % from_name
		refresh.call())
	refresh.call()
	status.text = "Feed the press on the seam band. %d %s to a clean %s; a good engineer reads a wider seam." % [
		ratio, from_name, to_name]
	return panel


class _CrusherPress extends Control:
	var zone_width := 0.14   # normalized sweet-spot width
	var _t := 0.0
	var _result_flash := 0.0
	var _result_good := false

	## The press comes down NOW: true if the marker sits on the seam.
	func strike() -> bool:
		_result_good = absf(_marker() - 0.5) < zone_width * 0.5
		_result_flash = 0.4
		return _result_good

	func _marker() -> float:
		return 0.5 + 0.5 * sin(_t * 2.6)

	func _process(delta: float) -> void:
		_t += delta
		_result_flash = maxf(0.0, _result_flash - delta)
		queue_redraw()

	func _draw() -> void:
		var bar := Rect2(10, size.y * 0.35, size.x - 20, 22)
		draw_rect(bar, Color(0.05, 0.08, 0.07))
		draw_rect(bar, Color(0.3, 0.4, 0.35), false, 1.0)
		var zone := Rect2(bar.position.x + bar.size.x * (0.5 - zone_width * 0.5),
			bar.position.y, bar.size.x * zone_width, bar.size.y)
		draw_rect(zone, Color(0.95, 0.75, 0.3, 0.55))
		var x := bar.position.x + bar.size.x * _marker()
		draw_rect(Rect2(x - 2, bar.position.y - 6, 4, bar.size.y + 12), Color(0.9, 0.95, 1.0))
		if _result_flash > 0.0:
			var color := Color(0.5, 0.95, 0.6, _result_flash * 2.0) if _result_good \
				else Color(0.95, 0.4, 0.3, _result_flash * 2.0)
			draw_rect(Rect2(Vector2.ZERO, size), color)


## --- damage control: the repair rig ------------------------------------------------


## The repair minigame for one interior damage entry. Weld when the marker
## crosses the seam; land the strikes and the damage clears. Engineering
## widens the seam; a plasma welder needs fewer strikes.
static func repair_rig(entry: Dictionary) -> Control:
	var kind: String = entry.get("kind", "fire")
	var titles := {"fire": "Damage Control — Fire",
		"conduit": "Damage Control — Arcing Conduit",
		"breach": "Damage Control — Hull Breach"}
	var panel := _frame(titles.get(kind, "Damage Control"))
	var box := _content(panel)

	var speed_mult := maxf(1.0, GameState.upgrade_effect_product("repair_speed_mult"))
	var strikes_needed := maxi(1, int(ceil(3.0 / speed_mult)))

	var status := Label.new()
	status.add_theme_font_size_override("font_size", 13)
	status.add_theme_color_override("font_color", Color(0.62, 0.66, 0.74))
	box.add_child(status)

	var press := _CrusherPress.new()
	press.custom_minimum_size = Vector2(480, 90)
	press.zone_width = clampf((0.10 + 0.03 * float(GameState.player_stat("engineering") - 2))
		* sqrt(speed_mult), 0.06, 0.30)
	box.add_child(press)

	var button := Button.new()
	button.text = {"fire": "SMOTHER  (Space)", "conduit": "RE-SEAT  (Space)",
		"breach": "WELD  (Space)"}.get(kind, "WELD  (Space)")
	box.add_child(button)
	button.grab_focus.call_deferred()

	var strikes := {"done": 0}
	var refresh := func() -> void:
		status.text = "Catch the seam — %d/%d. Slips feed the %s." % [
			strikes.done, strikes_needed, kind]
	button.pressed.connect(func() -> void:
		if press.strike():
			strikes.done += 1
			AudioManager.play("ui_click", 1.1)
			if strikes.done >= strikes_needed:
				GameState.repair_ship_damage(int(entry.get("id", -1)))
				AudioManager.play("ui_switch")
				(panel as _ClosablePanel).close()
				return
		else:
			entry["severity"] = minf(1.0, float(entry.get("severity", 1.0)) + 0.05)
			AudioManager.play("ui_click", 0.5)
		refresh.call())
	refresh.call()
	return panel
