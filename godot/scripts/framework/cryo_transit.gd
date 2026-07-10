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
var _awake := false         # the player is synthetic: no pod, open eyes
var _player_flies := false  # the player IS the jump pilot
var _tunnel_lines: Array = []   # the pilot's thoughts, spaced through the fold
var _tunnel_line_clock := 0.0

var _dim: ColorRect
var _title: Label
var _log: RichTextLabel
var _tunnel: _Tunnel
var _pod_row: _PodRow
var _shake := 0.0


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

	_pod_row = _PodRow.new()
	_pod_row.set_anchors_preset(Control.PRESET_CENTER_TOP)
	_pod_row.offset_top = 110
	_pod_row.offset_left = -320
	_pod_row.offset_right = 320
	_pod_row.custom_minimum_size = Vector2(640, 96)
	add_child(_pod_row)

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
## If the player IS synthetic, they stay awake — and see the crossing the
## crew never does.
func begin(route: Dictionary) -> void:
	_route = route
	_title.text = ("JUMP — %s" % route.get("name", "self-generated")).to_upper()

	var character := GameState.player_character()
	var player_synthetic: bool = character != "" \
		and DataRegistry.get_entity("npcs", character).get("synthetic", false)

	var organics: Array[String] = []
	var pilot_id := ""
	for crew_id: String in CrewRoster.aboard():
		if crew_id == character:
			continue  # the player's own body is the "you" pod (or the seat)
		var npc := DataRegistry.get_entity("npcs", crew_id)
		if not npc.get("synthetic", false):
			organics.append(crew_id)
		elif npc.get("jump_pilot", false):
			pilot_id = crew_id
		elif pilot_id == "":
			pilot_id = crew_id  # first synthetic stands in if nobody is rated
	_player_flies = player_synthetic and character != "" \
		and DataRegistry.get_entity("npcs", character).get("jump_pilot", false)
	_awake = player_synthetic
	if _player_flies:
		pilot_id = character
		_pilot_name = "You"
	elif pilot_id != "":
		_pilot_name = DataRegistry.get_entity("npcs", pilot_id).get("name", pilot_id)

	# The pods, visible: one per sleeping organic (plus the player's, if the
	# player sleeps), sealing as the script reads them off.
	_pod_row.reset()
	for crew_id: String in organics:
		var npc := DataRegistry.get_entity("npcs", crew_id)
		_pod_row.add_pod(npc.get("name", crew_id), StandIn.character_color(npc, crew_id))
	if not _awake:
		_pod_row.add_pod("You", Color(0.74, 0.82, 0.92))

	_lines = []
	_lines.append({"text": "[i]Jump drive spinning up. Conscious transit is not survivable. Cryo protocol begins.[/i]", "hold_seconds": 1.6})
	var pod := 1
	for crew_id: String in organics:
		var display_name: String = DataRegistry.get_entity("npcs", crew_id).get("name", crew_id)
		_lines.append({"text": "Pod %d sealed — %s. Vitals green." % [pod, display_name], "hold_seconds": POD_LINE_SECONDS})
		pod += 1
	var pilot_line: String = _route.get("pilot_line", "")
	if _awake:
		# You are the thing that stays awake.
		if _player_flies:
			_lines.append({"text": "[i]The deck goes quiet. %d slow pulses on the board, all green, all yours to carry. You run the pre-fold checks alone, the way you always do.[/i]" % organics.size(), "hold_seconds": 3.2})
		else:
			_lines.append({"text": "[i]You and %s are the only things awake on the boat. She crosses to the pilot's seat without ceremony and sits down next to the thing that unmakes minds. You hold the rail, and watch.[/i]" % _pilot_name, "hold_seconds": 3.6})
			if pilot_line != "":
				_lines.append({"text": "[b]%s:[/b] %s" % [_pilot_name, pilot_line], "hold_seconds": 2.2})
	else:
		_lines.append({"text": "Pod %d sealed — you. The lid hisses shut. Cold crawls up your arms." % pod, "hold_seconds": 1.6})
		if pilot_line != "":
			_lines.append({"text": "[b]%s:[/b] %s" % [_pilot_name, pilot_line], "hold_seconds": 2.2})
		# The held beat: the last thing you see through the frost.
		_lines.append({"text": "[i]The last thing you see through the frost is %s, alone at the board — the only mind left awake on the boat, settling in beside the thing that unmakes minds. A small, precise wave. Then the cold takes the window.[/i]" % _pilot_name, "hold_seconds": 4.5})

	# The awake crossing runs long, and the pilot's own thoughts surface
	# through it (npc `barks.transit_alone` — authored, hers).
	_tunnel_lines = []
	if _awake and pilot_id != "":
		var barks: Array = DataRegistry.get_entity("npcs", pilot_id) \
			.get("barks", {}).get("transit_alone", [])
		for bark: String in barks:
			_tunnel_lines.append("[i]%s[/i]" % bark if _player_flies
				else "[b]%s:[/b] %s" % [_pilot_name, bark])

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
	# The hull complains at the threshold: vibration as the window opens
	# and closes, still during the crossing itself.
	if _shake > 0.0:
		_shake = maxf(0.0, _shake - delta * 2.0)
		offset = Vector2(randf_range(-1.0, 1.0), randf_range(-1.0, 1.0)) * _shake * 4.0
	else:
		offset = Vector2.ZERO
	match _phase:
		"boarding":
			if _clock <= 0.0:
				_advance_line()
		"tunnel":
			# The awake crossing thinks out loud, one long-held line at a time.
			if not _tunnel_lines.is_empty():
				_tunnel_line_clock -= delta
				if _tunnel_line_clock <= 0.0:
					_log.append_text(str(_tunnel_lines.pop_front()) + "\n")
					AudioManager.play("ui_click", 0.5)
					_tunnel_line_clock = 4.2
			if _clock <= 0.0:
				_start_wake()
		"wake":
			if _clock <= 0.0:
				_phase = "done"
				offset = Vector2.ZERO
				finished.emit(_route)


func _advance_line() -> void:
	_line_index += 1
	if _line_index >= _lines.size():
		_start_tunnel()
		return
	var line: Dictionary = _lines[_line_index]
	_log.append_text(str(line.text) + "\n")
	if str(line.text).begins_with("Pod "):
		_pod_row.seal_next()
		AudioManager.play("force_field", 0.9)
	else:
		AudioManager.play("ui_click", 0.8)
	_clock = float(line.hold_seconds)


func _start_tunnel() -> void:
	_phase = "tunnel"
	var seconds := float(_route.get("transit_seconds", DEFAULT_TRANSIT_SECONDS))
	if _awake:
		# Awake, the crossing is not a cut — it is a duration. Sit with it.
		seconds = maxf(seconds * 1.8, seconds + _tunnel_lines.size() * 4.5)
	_clock = seconds
	_tunnel_line_clock = 2.5
	_log.append_text("\n[i]The world folds.[/i]\n")
	_tunnel.visible = true
	_pod_row.visible = false
	_shake = 1.6
	AudioManager.play("phase_jump", 0.9)


func _start_wake() -> void:
	_phase = "wake"
	_clock = WAKE_SECONDS
	_tunnel.visible = false
	_pod_row.visible = true
	_pod_row.open_all()
	_shake = 1.0
	if _awake:
		_log.append_text("\n[i]The window remembers how to be a window. You initiate the revival cycle; the pods hiss open and the crew comes up gasping, the way organics do — as if surfacing. None of them will ask what it looked like. Both facts are load-bearing.[/i]\n")
	else:
		_log.append_text("\n[i]Revival cycle. Light. The smell of recycled antiseptic — the sensory signature of arrival.[/i]\n")
	var arrival: String = _route.get("arrival_line", "")
	if arrival != "" and not _player_flies:
		_log.append_text("[b]%s:[/b] %s\n" % [_pilot_name, arrival])
	AudioManager.play("power_up", 0.9)


## The pod bank, visible: one silhouette per sleeper, lids sealing in turn
## with a cryo-blue frost, opening again on the far side.
class _PodRow extends Control:
	var _pods: Array = []  # {name, color, state: open|sealed}
	var _t := 0.0

	func reset() -> void:
		_pods.clear()

	func add_pod(pod_name: String, color: Color) -> void:
		_pods.append({"name": pod_name, "color": color, "state": "open"})

	func seal_next() -> void:
		for pod: Dictionary in _pods:
			if pod.state == "open":
				pod.state = "sealed"
				return

	func open_all() -> void:
		for pod: Dictionary in _pods:
			pod.state = "open"

	func _process(delta: float) -> void:
		_t += delta
		queue_redraw()

	func _draw() -> void:
		if _pods.is_empty():
			return
		var pod_w := 52.0
		var gap := 18.0
		var total := _pods.size() * pod_w + (_pods.size() - 1) * gap
		var x := (size.x - total) * 0.5
		for pod: Dictionary in _pods:
			var rect := Rect2(x, 0, pod_w, 78)
			draw_rect(rect, Color(0.16, 0.20, 0.24))
			draw_rect(rect, Color(0.45, 0.55, 0.62), false, 2.0)
			# The sleeper
			var body: Color = pod.color
			draw_circle(Vector2(x + pod_w * 0.5, 22), 8.0, body)
			draw_rect(Rect2(x + pod_w * 0.5 - 9, 32, 18, 34), body.darkened(0.25))
			if pod.state == "sealed":
				var frost := Color(0.55, 0.85, 0.95, 0.55 + 0.1 * sin(_t * 2.0))
				draw_rect(rect.grow(-3), frost)
				draw_rect(rect.grow(-3), Color(0.75, 0.95, 1.0, 0.9), false, 1.5)
			draw_string(ThemeDB.fallback_font, Vector2(x - 4, 94), str(pod.name),
				HORIZONTAL_ALIGNMENT_CENTER, pod_w + 8, 11, Color(0.85, 0.9, 0.95))
			x += pod_w + gap


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
