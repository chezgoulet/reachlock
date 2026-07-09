extends CanvasLayer
## Ring 0 — CryoTransit: the self-generated jump sequence (GAME-DESIGN.md
## §6.3). Conscious exposure to hyperspace destroys an organic mind, so a
## self-jump is a ritual: the organic crew pod up, a synthetic crew member
## takes the ship, the tunnel, the wake-up. This scene is that ritual.
##
## Data-driven end to end: the route (a location's `self_jump` block) names
## the destination and carries the pilot's authored lines; the crew roster
## and npc `synthetic`/`jump_pilot` fields decide who sleeps and who flies.
## The scene names no content id.
##
## The host (space flight mode) mounts it, calls begin(route), and acts on
## `finished` — this scene changes no game state itself.

class_name CryoTransit

signal finished(route: Dictionary)

const POD_LINE_SECONDS := 1.1
const DEFAULT_TRANSIT_SECONDS := 8.0
const WAKE_SECONDS := 2.6

var _route: Dictionary = {}
var _pilot_name := "Autopilot"
var _lines: Array[Dictionary] = []  # {text, hold_seconds}
var _line_index := -1
var _clock := 0.0
var _phase := "boarding"  # boarding -> tunnel -> wake -> done

var _dim: ColorRect
var _title: Label
var _log: RichTextLabel
var _tunnel: _Tunnel


func _ready() -> void:
	layer = 80
	_dim = ColorRect.new()
	_dim.color = Color(0.01, 0.015, 0.03, 0.0)
	_dim.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(_dim)

	_tunnel = _Tunnel.new()
	_tunnel.set_anchors_preset(Control.PRESET_FULL_RECT)
	_tunnel.visible = false
	add_child(_tunnel)

	_title = Label.new()
	_title.set_anchors_preset(Control.PRESET_CENTER_TOP)
	_title.offset_top = 60
	_title.add_theme_font_size_override("font_size", 26)
	_title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	add_child(_title)

	_log = RichTextLabel.new()
	_log.bbcode_enabled = true
	_log.scroll_following = true
	_log.set_anchors_preset(Control.PRESET_CENTER)
	_log.offset_left = -380
	_log.offset_right = 380
	_log.offset_top = -140
	_log.offset_bottom = 200
	add_child(_log)


## Kick off the sequence for a `self_jump` route. Builds the pod-up script
## from whoever is actually aboard: organics sleep, the jump pilot flies.
func begin(route: Dictionary) -> void:
	_route = route
	_title.text = ("JUMP — %s" % route.get("name", "self-generated")).to_upper()

	var organics: Array[String] = []
	var pilot_id := ""
	for crew_id: String in CrewRoster.aboard():
		var npc := DataRegistry.get_entity("npcs", crew_id)
		if not npc.get("synthetic", false):
			organics.append(crew_id)
		elif npc.get("jump_pilot", false):
			pilot_id = crew_id
		elif pilot_id == "":
			pilot_id = crew_id  # first synthetic stands in if nobody is rated
	if pilot_id != "":
		_pilot_name = DataRegistry.get_entity("npcs", pilot_id).get("name", pilot_id)

	_lines = []
	_lines.append({"text": "[i]Jump drive spinning up. Conscious transit is not survivable. Cryo protocol begins.[/i]", "hold_seconds": 1.6})
	var pod := 1
	for crew_id: String in organics:
		var display_name: String = DataRegistry.get_entity("npcs", crew_id).get("name", crew_id)
		_lines.append({"text": "Pod %d sealed — %s. Vitals green." % [pod, display_name], "hold_seconds": POD_LINE_SECONDS})
		pod += 1
	_lines.append({"text": "Pod %d sealed — you. The lid hisses shut. Cold crawls up your arms." % pod, "hold_seconds": 1.6})
	var pilot_line: String = _route.get("pilot_line", "")
	if pilot_line != "":
		_lines.append({"text": "[b]%s:[/b] %s" % [_pilot_name, pilot_line], "hold_seconds": 2.2})

	# Let any live minds aboard know the crossing is happening — a soul can
	# form its own memory of another transit spent awake and alone.
	if SoulGateway.is_ready() and pilot_id != "":
		SoulGateway.perceive(pilot_id, {
			"id": "jump_transit_%d" % Time.get_ticks_msec(),
			"kind": "event",
			"objective": "The organic crew is in cryosleep. You are flying the ship through hyperspace, alone and awake, as you always do.",
			"revision": 0,
		}, {"topic": "jump.self_transit", "route": str(route.get("name", ""))})

	_phase = "boarding"
	_line_index = -1
	_clock = 0.0
	_advance_line()


func _process(delta: float) -> void:
	if _phase == "done":
		return
	_clock -= delta
	_dim.color.a = minf(_dim.color.a + delta * 0.8, 0.94)
	match _phase:
		"boarding":
			if _clock <= 0.0:
				_advance_line()
		"tunnel":
			if _clock <= 0.0:
				_start_wake()
		"wake":
			if _clock <= 0.0:
				_phase = "done"
				finished.emit(_route)


func _advance_line() -> void:
	_line_index += 1
	if _line_index >= _lines.size():
		_start_tunnel()
		return
	var line: Dictionary = _lines[_line_index]
	_log.append_text(str(line.text) + "\n")
	_clock = float(line.hold_seconds)
	AudioManager.play("ui_click", 0.8)


func _start_tunnel() -> void:
	_phase = "tunnel"
	_clock = float(_route.get("transit_seconds", DEFAULT_TRANSIT_SECONDS))
	_log.append_text("\n[i]The world folds.[/i]\n")
	_tunnel.visible = true
	AudioManager.play("force_field", 0.6)


func _start_wake() -> void:
	_phase = "wake"
	_clock = WAKE_SECONDS
	_tunnel.visible = false
	_log.append_text("\n[i]Revival cycle. Light. The smell of recycled antiseptic — the sensory signature of arrival.[/i]\n")
	var arrival: String = _route.get("arrival_line", "")
	if arrival != "":
		_log.append_text("[b]%s:[/b] %s\n" % [_pilot_name, arrival])
	AudioManager.play("power_up", 0.9)


## Streaking-star tunnel, drawn flat: hyperspace as the crew never sees it.
class _Tunnel extends Control:
	var _t := 0.0
	var _rng := RandomNumberGenerator.new()
	var _streaks: Array = []

	func _ready() -> void:
		_rng.seed = 0xC7 + 0x40
		for i in 90:
			_streaks.append({
				"angle": _rng.randf() * TAU,
				"dist": _rng.randf_range(0.05, 1.0),
				"speed": _rng.randf_range(0.4, 1.6),
				"hue": _rng.randf(),
			})

	func _process(delta: float) -> void:
		_t += delta
		queue_redraw()

	func _draw() -> void:
		var center := size * 0.5
		var max_r := size.length() * 0.55
		for s: Dictionary in _streaks:
			var d := fmod(s.dist + _t * s.speed * 0.25, 1.0)
			var from := center + Vector2.RIGHT.rotated(s.angle) * d * max_r
			var to := center + Vector2.RIGHT.rotated(s.angle) * minf(d + 0.06 + d * 0.12, 1.0) * max_r
			var color := Color.from_hsv(fmod(s.hue + _t * 0.05, 1.0), 0.55, 1.0, 0.5 + d * 0.5)
			draw_line(from, to, color, 1.0 + d * 3.0)
