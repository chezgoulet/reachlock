extends CanvasLayer
## Ring 0 — MissionHud: the always-on mission surface. Shows the active
## mission's name, current objective, counted-event progress, and stage
## timer across every mode (it is a CanvasLayer autoload, so it survives
## mode switches). Also the presenter for mission `epilogue` cards —
## success and failure endings are content, this just puts them on screen.
##
## It is the frame-driver for MissionManager.tick (stage timers need a
## heartbeat that no mode scene should own).

const TIMER_WARNING_SECONDS := 30.0

var _panel: PanelContainer
var _mission_label: Label
var _objective_label: Label
var _timer_label: Label
var _timer_ring: _TimerRing

var _overlay: Control = null


## The countdown as a shape: a draining ring that cools from green through
## amber to red. Peripheral vision reads it long before the digits do.
class _TimerRing extends Control:
	var fraction := 1.0

	func _process(_delta: float) -> void:
		queue_redraw()

	func _draw() -> void:
		var center := size * 0.5
		var radius := minf(size.x, size.y) * 0.42
		draw_arc(center, radius, 0, TAU, 32, Color(0.3, 0.32, 0.38, 0.6), 3.0)
		if fraction < 0.0:
			return
		var color := Color(0.35, 0.85, 0.45)
		if fraction < 0.5:
			color = Color(1.0, 0.73, 0.33)
		if fraction < 0.2:
			var pulse := 0.6 + 0.4 * sin(Time.get_ticks_msec() * 0.012)
			color = Color(0.95, 0.30, 0.25, pulse)
		draw_arc(center, radius, -PI / 2.0, -PI / 2.0 + TAU * fraction, 32, color, 4.0)


func _ready() -> void:
	layer = 90
	_build_banner()
	MissionManager.mission_started.connect(func(_id: String, _name: String) -> void: _refresh())
	MissionManager.stage_advanced.connect(func(_id: String, _sid: String, _obj: String) -> void: _refresh())
	MissionManager.mission_completed.connect(func(_id: String) -> void: _refresh())
	MissionManager.mission_failed.connect(func(id: String, reason: String) -> void:
		_on_mission_failed(id, reason)
		_refresh())
	MissionManager.epilogue_ready.connect(_show_epilogue)
	_refresh()


func _process(delta: float) -> void:
	MissionManager.tick(delta)
	_update_timer()


## --- banner --------------------------------------------------------------------


func _build_banner() -> void:
	_panel = PanelContainer.new()
	_panel.set_anchors_preset(Control.PRESET_TOP_RIGHT)
	_panel.offset_left = -420
	_panel.offset_right = -12
	_panel.offset_top = 10
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0.06, 0.07, 0.10, 0.82)
	style.border_color = Color(0.45, 0.55, 0.70, 0.6)
	style.set_border_width_all(1)
	style.set_content_margin_all(10)
	_panel.add_theme_stylebox_override("panel", style)
	add_child(_panel)

	var box := VBoxContainer.new()
	_panel.add_child(box)

	_mission_label = Label.new()
	_mission_label.add_theme_font_size_override("font_size", 14)
	_mission_label.add_theme_color_override("font_color", Color(0.65, 0.75, 0.9))
	box.add_child(_mission_label)

	_objective_label = Label.new()
	_objective_label.add_theme_font_size_override("font_size", 17)
	_objective_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	box.add_child(_objective_label)

	var timer_row := HBoxContainer.new()
	timer_row.add_theme_constant_override("separation", 10)
	box.add_child(timer_row)
	_timer_ring = _TimerRing.new()
	_timer_ring.custom_minimum_size = Vector2(34, 34)
	timer_row.add_child(_timer_ring)
	_timer_label = Label.new()
	_timer_label.add_theme_font_size_override("font_size", 20)
	timer_row.add_child(_timer_label)


func _refresh() -> void:
	if not MissionManager.is_active():
		_panel.visible = false
		return
	_panel.visible = true
	_mission_label.text = MissionManager.mission_name().to_upper()
	var objective := MissionManager.current_objective()
	var remaining := MissionManager.event_remaining()
	if remaining > 0:
		objective += "  (%d to go)" % remaining
	_objective_label.text = objective
	_update_timer()


func _update_timer() -> void:
	if not MissionManager.is_active():
		return
	# Counted events tick between stage changes; keep the line current.
	var remaining_events := MissionManager.event_remaining()
	if remaining_events >= 0:
		var base := MissionManager.current_objective()
		_objective_label.text = base + ("  (%d to go)" % remaining_events if remaining_events > 0 else "")
	var t := MissionManager.timer_remaining()
	if t < 0.0:
		_timer_label.visible = false
		_timer_ring.visible = false
		return
	_timer_label.visible = true
	_timer_ring.visible = true
	_timer_ring.fraction = MissionManager.timer_fraction()
	_timer_label.text = "%d:%02d" % [int(t) / 60, int(t) % 60]
	_timer_label.add_theme_color_override("font_color",
		Color(0.95, 0.30, 0.25) if t <= TIMER_WARNING_SECONDS else Color(0.9, 0.9, 0.9))


## --- epilogue cards --------------------------------------------------------------


## Failure doesn't cut straight to prose: the moment lands first. A beat of
## red and a verdict line — THE WINDOW CLOSES / HULL LOST — then the card.
const FAIL_BEATS := {
	"time_expired": "THE WINDOW CLOSES",
	"ship_destroyed": "HULL LOST",
}
var _fail_reason := ""


func _on_mission_failed(_mission_id: String, reason: String) -> void:
	_fail_reason = reason


func _show_epilogue(mission_id: String, text: String, success: bool) -> void:
	if not success and FAIL_BEATS.has(_fail_reason):
		_fanfare_then_card(mission_id, text)
		return
	_present_card(text, success)


func _fanfare_then_card(mission_id: String, text: String) -> void:
	var beat := Control.new()
	beat.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(beat)
	var red := ColorRect.new()
	red.color = Color(0.45, 0.04, 0.03, 0.0)
	red.set_anchors_preset(Control.PRESET_FULL_RECT)
	beat.add_child(red)
	var verdict := Label.new()
	verdict.text = FAIL_BEATS.get(_fail_reason, "THE RUN ENDS HERE")
	verdict.add_theme_font_size_override("font_size", 44)
	verdict.add_theme_color_override("font_color", Color(1.0, 0.35, 0.3, 0.0))
	verdict.set_anchors_preset(Control.PRESET_CENTER)
	verdict.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	beat.add_child(verdict)
	AudioManager.play("explosion", 0.5)
	var tween := create_tween()
	tween.tween_property(red, "color:a", 0.55, 0.35)
	tween.parallel().tween_property(verdict, "theme_override_colors/font_color",
		Color(1.0, 0.35, 0.3, 1.0), 0.5)
	tween.tween_interval(1.9)
	tween.tween_callback(func() -> void:
		beat.queue_free()
		_present_card(text, false))


func _present_card(text: String, success: bool) -> void:
	if _overlay != null:
		_overlay.queue_free()
	_overlay = Control.new()
	_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(_overlay)

	var dim := ColorRect.new()
	dim.color = Color(0.02, 0.02, 0.04, 0.92)
	dim.set_anchors_preset(Control.PRESET_FULL_RECT)
	_overlay.add_child(dim)

	var box := VBoxContainer.new()
	box.set_anchors_preset(Control.PRESET_CENTER)
	box.custom_minimum_size = Vector2(760, 0)
	box.position -= Vector2(380, 120)
	box.add_theme_constant_override("separation", 24)
	_overlay.add_child(box)

	var heading := Label.new()
	heading.text = "" if success else "THE RUN ENDS HERE"
	heading.add_theme_font_size_override("font_size", 30)
	heading.add_theme_color_override("font_color",
		Color(0.75, 0.85, 0.95) if success else Color(0.9, 0.35, 0.3))
	heading.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	box.add_child(heading)

	var body := RichTextLabel.new()
	body.bbcode_enabled = true
	body.fit_content = true
	body.custom_minimum_size = Vector2(760, 0)
	body.append_text(text)
	box.add_child(body)

	var button := Button.new()
	button.text = "Continue" if success else "Pick up from your last save"
	button.custom_minimum_size = Vector2(280, 44)
	button.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	button.pressed.connect(_dismiss_epilogue.bind(success))
	box.add_child(button)

	get_tree().paused = true
	_overlay.process_mode = Node.PROCESS_MODE_WHEN_PAUSED


func _dismiss_epilogue(success: bool) -> void:
	get_tree().paused = false
	if _overlay != null:
		_overlay.queue_free()
		_overlay = null
	if success:
		return
	# Failure: rewind to the last checkpoint (docking saves). If none exists,
	# patch the ship up and restart the authored campaign from the top.
	if GameState.load_game():
		GameManager.request_mode(
			GameManager.Mode.LANDED if GameState.is_docked() else GameManager.Mode.SPACE_FLIGHT)
	else:
		GameState.player.ship.hull_integrity = maxf(0.35, float(GameState.player.ship.hull_integrity))
		GameState.clear_flag("campaign_over")
		MissionManager.autostart_if_idle()
		GameManager.request_mode(GameManager.Mode.ON_BOARD)
