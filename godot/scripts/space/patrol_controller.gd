extends Node3D
## Ring 0 — Compact Blockade Patrol AI (Sprint 03, P16).

class_name PatrolController
##
## Patrol ships that scan, detect, and engage based on player actions
## and reputation. Behavior driven by a state machine with configurable
## parameters from data files.
##
## Data-driven: patrol routes and ship types come from content data.
## The engine never names a faction id.

signal contact_detected(ship_id: String, faction: String, threat_level: int)
signal engagement_started(ship_id: String)
signal destroyed(ship_id: String)

enum State {
	PATROL = 0,
	INVESTIGATE = 1,
	ENGAGE = 2,
	ALERT = 3,
	PURSUE = 4,
	DESTROYED = 5,
}

const STATE_NAMES := {
	State.PATROL: "patrol",
	State.INVESTIGATE: "investigate",
	State.ENGAGE: "engage",
	State.ALERT: "alert",
	State.PURSUE: "pursue",
	State.DESTROYED: "destroyed",
}

## Detection ranges (in units). Affected by scanner station power.
const DETECT_RANGE := 80.0
const INVESTIGATE_RANGE := 120.0
const ENGAGE_RANGE := 60.0
const PURSUE_SPEED_MULT := 1.1
const ALERT_REINFORCEMENT_DELAY := 5.0

var _state: int = State.PATROL
var _ship: Node3D = null
var _player: Node3D = null
var _target: Vector3 = Vector3.ZERO
var _patrol_route: Array = []
var _route_index: int = 0
var _alert_timer: float = 0.0
var _fire_clock: float = 0.0
var _hp: int = 3
var _max_hp: int = 3
var _flight_stats: Dictionary = {}

var _soul_id: String = ""
var _faction_id: String = ""
var _rng: RandomNumberGenerator = RandomNumberGenerator.new()


func _ready() -> void:
	_rng.seed = randi() % 100000


## Initialize from a hull + soul definition.
func configure(soul: Dictionary, hull: Dictionary, player_node: Node3D) -> void:
	_soul_id = soul.get("id", "")
	_faction_id = soul.get("faction", "")
	_flight_stats = hull.get("flight", {})
	_max_hp = 2 + int(hull.get("stats", {}).get("armor", 1)) * 2
	_hp = _max_hp
	_player = player_node
	
	# Build the 3D ship node
	_ship = _build_ship_mesh(Color.from_string(str(soul.get("color", "")), Color(0.55, 0.2, 0.16)))
	add_child(_ship)


func _build_ship_mesh(base_color: Color) -> Node3D:
	var ship := Node3D.new()
	var mesh := MeshInstance3D.new()
	mesh.mesh = BoxMesh.new()
	mesh.mesh.size = Vector3(4, 2, 6)  # Simple patrol boat shape
	mesh.set_surface_override_material(0, _make_material(base_color))
	ship.add_child(mesh)
	return ship


func _make_material(color: Color) -> Material:
	var mat := StandardMaterial3D.new()
	mat.albedo_color = color
	return mat


## Set patrol waypoints.
func set_patrol_route(waypoints: Array) -> void:
	_patrol_route = waypoints.duplicate()
	_route_index = 0


## ___steering_behaviors_______________________________________________________

const _UP := Vector3(0, 1, 0)
var _current_vel := Vector3.ZERO  # updated per physics tick


## Seek: steer toward a target position at max acceleration.
func _seek(target_pos: Vector3) -> Vector3:
	var desired: Vector3 = (target_pos - global_position).normalized() * _flight_stats.get("top_speed", 35.0)
	desired = desired.slide(_UP)
	return (desired - _current_vel).normalized() * _flight_stats.get("acceleration", 15.0)


## Pursue: predict where target will be, seek to that predicted position.
func _pursue(target_pos: Vector3, target_vel: Vector3) -> Vector3:
	var dist: float = global_position.distance_to(target_pos)
	var speed: float = maxf(_current_vel.length(), 1.0)
	var predict_time: float = minf(dist / speed, 1.5)
	return _seek(target_pos + target_vel * predict_time)


## Flee: steer away from a danger position.
func _flee(danger_pos: Vector3) -> Vector3:
	var desired: Vector3 = (global_position - danger_pos).normalized() * _flight_stats.get("top_speed", 35.0)
	desired = desired.slide(_UP)
	return (desired - _current_vel).normalized() * _flight_stats.get("acceleration", 15.0)


## Follow path: steer toward the next waypoint; advance when close.
func _follow_path() -> Vector3:
	if _patrol_route.is_empty():
		return Vector3.ZERO
	var wp: Vector3 = _patrol_route[_route_index]
	if global_position.distance_to(wp) < 15.0:
		_route_index = (_route_index + 1) % _patrol_route.size()
	return _seek(wp)


func _physics_process(delta: float) -> void:
	if _state == State.DESTROYED or _ship == null:
		return
	
	var prev_pos := _ship.global_position
	_fire_clock = maxf(0.0, _fire_clock - delta)
	
	match _state:
		State.PATROL:
			_patrol_tick(delta)
		State.INVESTIGATE:
			_investigate_tick(delta)
		State.ENGAGE:
			_engage_tick(delta)
		State.ALERT:
			_alert_tick(delta)
		State.PURSUE:
			_pursue_tick(delta)
	
	# Update current velocity from position delta
	_current_vel = (_ship.global_position - prev_pos) / maxf(delta, 0.001)


func _patrol_tick(delta: float) -> void:
	# Fly toward current waypoint
	if _patrol_route.is_empty():
		# No route — orbit current position
		_target = _ship.global_position + Vector3(sin(Time.get_ticks_msec() * 0.001) * 30, 0, cos(Time.get_ticks_msec() * 0.001) * 30)
	else:
		_target = _patrol_route[_route_index]
		if _ship.global_position.distance_to(_target) < 10.0:
			_route_index = (_route_index + 1) % _patrol_route.size()
			_target = _patrol_route[_route_index]
	
	_move_toward(_target, delta, 0.5)
	
	# Detect player
	if _player != null and _ship.global_position.distance_to(_player.global_position) < DETECT_RANGE:
		_set_state(State.INVESTIGATE)


func _investigate_tick(delta: float) -> void:
	if _player == null:
		_set_state(State.PATROL)
		return
	
	_move_toward(_player.global_position, delta, 0.7)
	
	var dist := _ship.global_position.distance_to(_player.global_position)
	if dist < ENGAGE_RANGE:
		_set_state(State.ENGAGE)
	elif dist > INVESTIGATE_RANGE:
		_set_state(State.PATROL)


func _engage_tick(delta: float) -> void:
	if _player == null:
		_set_state(State.PATROL)
		return
	
	var to_player := _player.global_position - _ship.global_position
	var dist := to_player.length()
	
	# Face and approach
	_ship.look_at(_player.global_position, Vector3.UP)
	
	if dist > ENGAGE_RANGE * 1.5:
		_move_toward(_player.global_position, delta, 0.8)
	elif dist < ENGAGE_RANGE * 0.4:
		# Back off slightly to maintain range
		_move_toward(_ship.global_position - to_player.normalized() * 50, delta, 0.5)
	
	# Fire
	if _fire_clock <= 0.0:
		_fire()
	
	# Call reinforcements if low HP or long engagement
	if _hp <= _max_hp * 0.4:
		_set_state(State.ALERT)


func _alert_tick(delta: float) -> void:
	_alert_timer += delta
	if _alert_timer >= ALERT_REINFORCEMENT_DELAY:
		# Spawn reinforcements (emitted as signal for the host to handle)
		engagement_started.emit(_soul_id)
		_set_state(State.ENGAGE if _player != null else State.PATROL)


func _pursue_tick(delta: float) -> void:
	if _player == null:
		_set_state(State.PATROL)
		return
	
	_move_toward(_player.global_position, delta, PURSUE_SPEED_MULT)
	
	var dist := _ship.global_position.distance_to(_player.global_position)
	if dist < ENGAGE_RANGE:
		_set_state(State.ENGAGE)
	elif dist > DETECT_RANGE * 3:
		_set_state(State.PATROL)


## --- movement ----------------------------------------------------------------


func _move_toward(target: Vector3, delta: float, speed_mult: float) -> void:
	var speed: float = _flight_stats.get("top_speed", 35.0) * speed_mult
	var direction: Vector3 = (target - _ship.global_position).normalized()
	_ship.global_position += direction * speed * delta
	_ship.look_at(target, Vector3.UP)


## --- combat ------------------------------------------------------------------


func _fire() -> void:
	if _player == null:
		return
	_fire_clock = 1.5  # fire interval
	# Damage player (handled by the host scene)
	destroyed.emit(_soul_id + "_fired")


func hit(damage: int = 1) -> bool:
	_hp -= damage
	_ship.scale = Vector3.ONE * 1.2  # hit flash
	if _hp <= 0:
		_set_state(State.DESTROYED)
		destroyed.emit(_soul_id)
		return true
	if _state == State.ENGAGE and _hp <= _max_hp * 0.5:
		_set_state(State.ALERT)
	return false


## --- state machine ----------------------------------------------------------


func _set_state(new_state: int) -> void:
	if _state == new_state:
		return
	_state = new_state
	_alert_timer = 0.0


func current_state_name() -> String:
	return STATE_NAMES.get(_state, "unknown")


func is_alive() -> bool:
	return _state != State.DESTROYED
