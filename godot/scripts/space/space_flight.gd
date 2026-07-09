extends Node3D
## Space Flight mode — arcade flight plus the Sprint 01 slice: mine the drift,
## survive the ambush, dock at the station (GAME-DESIGN.md §2).
##
## Engine code: every name here comes from content. The station is the start
## location; what the drift yields is the location's `mining` block; the
## pirate's hull is whatever ship the ambush NPC flies. Feel numbers come from
## the player hull's `flight` block, with engine defaults.

const DEFAULT_FLIGHT := {
	"top_speed": 46.0, "acceleration": 30.0, "turn_rate": 1.7,
	"boost_multiplier": 2.3, "drift": 0.35, "bank_angle_deg": 50.0,
}

const BASE_FOV := 75.0
const BOOST_FOV := 93.0
const TURN_RESPONSE := 8.0
const CAMERA_RESPONSE := 5.5
const CAMERA_OFFSET := Vector3(0.0, 2.4, 10.0)

const DOCK_RANGE := 32.0
const MINE_RANGE := 20.0
const MINE_SECONDS_PER_UNIT := 1.4
const FIRE_COOLDOWN := 0.25
const FIRE_RANGE := 90.0
const FIRE_CONE_DEG := 5.0
const PIRATE_FIRE_INTERVAL := 1.6
const PIRATE_FIRE_RANGE := 70.0
const PIRATE_HIT_CHANCE := 0.5
const PIRATE_HIT_DAMAGE := 0.06

var _stats: Dictionary = DEFAULT_FLIGHT.duplicate()
var _velocity := Vector3.ZERO
var _angular_velocity := Vector3.ZERO
var _boosting := false

var _ship: Node3D
var _hull: Node3D
var _starfield: MultiMeshInstance3D
var _station: Node3D
var _location: Dictionary = {}

var _minable: Array[Dictionary] = []  # {node, ore}
var _mine_progress := 0.0
var _ore_mined_total := 0

var _pirate: Node3D = null
var _pirate_soul: Dictionary = {}
var _pirate_hull: Dictionary = {}
var _pirate_hp := 0
var _pirate_fleeing := false
var _pirate_fire_clock := 0.0
var _fire_clock := 0.0
var _ambush_done := false
var _tick_accumulator := 0.0
var _rng := RandomNumberGenerator.new()

var _jump_route: Dictionary = {}   # this space's self_jump block, if any
var _transit: CryoTransit = null   # live cryo transit sequence, if any

@onready var _camera: Camera3D = $Camera3D
@onready var _speed_label: Label = $ModeOverlay/SpeedLabel
@onready var _ship_label: Label = $ModeOverlay/ShipLabel
@onready var _hint_label: Label = $ModeOverlay/HintLabel
@onready var _status_label: Label = $ModeOverlay/StatusLabel


func _ready() -> void:
	_rng.seed = 0x4C6F7570
	_location = DataRegistry.get_entity("locations", GameState.current_space())
	_load_flight_stats()
	_build_environment()
	_ship = _build_ship(Color(0.42, 0.44, 0.48))
	add_child(_ship)
	_starfield = _build_starfield()
	add_child(_starfield)
	add_child(_build_asteroid_field())
	_station = _build_station()
	add_child(_station)
	_build_minable_rocks()
	_build_patrols()
	_build_jump_gate()
	_jump_route = _location.get("self_jump", {})
	# Configure gravity from location
	var loc_grav: Dictionary = _location.get("gravity", {"type": "energy_plate", "strength": 1.0, "safe": true})
	GravitySystem.configure_location(loc_grav)
	_camera.fov = BASE_FOV
	_camera.global_transform = _camera_rest_transform()
	_refresh_status()
	GameState.state_changed.connect(_refresh_status)
	
	# Start engine audio loop as a named child
	# arch-allow: audio is mod content loaded at runtime by path
	var eng := AudioStreamPlayer2D.new()
	eng.name = "EngineLoop"
	var eng_path := "res://mods/reachlock/assets/audio/kenney_sci-fi-sounds/Audio/spaceEngineLarge_001.ogg"  # arch-allow: content path
	if ResourceLoader.exists(eng_path):
		eng.stream = ResourceLoader.load(eng_path)
	eng.volume_db = -20
	eng.autoplay = true
	add_child(eng)


func _physics_process(delta: float) -> void:
	# Mode switches free this scene at end of frame; physics can tick once
	# more against nodes already out of the tree (global transforms error).
	if not is_inside_tree() or _ship == null or not _ship.is_inside_tree():
		return
	# During a cryo transit the ship flies itself (the crew is asleep).
	if _transit != null:
		return

	# Read pilot controls: player occupancy uses keyboard + sync to ShipOperation,
	# crew occupancy reads from ShipOperation, unoccupied = keyboard fallback.
	var pilot_controls: Dictionary = {}
	var weapons_controls: Dictionary = {}
	
	if ShipOperation.is_active() and ShipOperation.is_occupied("pilot"):
		var occupant: String = ShipOperation.occupied_by("pilot")
		if occupant == "player":
			# Player at the console — read keyboard, sync to ShipOperation for effects
			pilot_controls = _direct_pilot_input()
			for axis: String in pilot_controls:
				ShipOperation.set_control("pilot", axis, pilot_controls[axis])
		else:
			# Crew AI piloting — read from ShipOperation
			pilot_controls = ShipOperation.controls.get("pilot", {})
	else:
		# Fallback: direct keyboard input when no one at pilot station
		pilot_controls = _direct_pilot_input()
	
	if ShipOperation.is_active() and ShipOperation.is_occupied("weapons"):
		weapons_controls = ShipOperation.controls.get("weapons", {})
	else:
		weapons_controls = _direct_weapons_input()
	
	_apply_rotation(delta, pilot_controls)
	_apply_thrust(delta, pilot_controls)
	_ship.global_position += _velocity * delta
	_apply_banking(delta)
	_update_camera(delta)
	
	# Update ShipOperation effects for exterior visualization
	ShipOperation.set_effect("engine_glow", absf(pilot_controls.get("throttle", 0.0)))
	ShipOperation.set_effect("engine_trail", absf(_velocity.length() / _stats.top_speed))
	
	# Engine audio follows throttle
	if has_node("EngineLoop"):
		var eng: AudioStreamPlayer2D = $EngineLoop
		var throttle_val: float = absf(pilot_controls.get("throttle", 0.0))
		eng.volume_db = -20 + throttle_val * 15
		eng.pitch_scale = 0.8 + throttle_val * 0.5
	
	_starfield.global_position = _ship.global_position
	_advance_tick(delta)
	_update_mining(delta)
	_update_combat(delta)
	_update_pirate(delta)
	_update_patrols(delta)
	_update_docking()
	_update_self_jump()
	_update_hud()


## --- flight (ShipOperation-aware) --------------------------------------------

## Fallback pilot controls from direct keyboard input when no one occupies
## the pilot station. Used for backward compat and testing.
func _direct_pilot_input() -> Dictionary:
	return {
		"throttle": Input.get_axis("thrust_back", "thrust_forward"),
		"yaw": Input.get_axis("yaw_right", "yaw_left"),
		"pitch": Input.get_axis("pitch_down", "pitch_up"),
		"roll": Input.get_axis("roll_right", "roll_left"),
		"boost": Input.is_action_pressed("boost"),
		"brake": Input.is_action_pressed("brake"),
	}


## Fallback weapons controls from direct keyboard input.
func _direct_weapons_input() -> Dictionary:
	return {
		"fire": Input.is_action_pressed("fire"),
		"target_id": "",
		"target_position": Vector3.ZERO,
		"weapon_index": 0,
	}


func _apply_rotation(delta: float, controls: Dictionary) -> void:
	var turn_rate: float = _stats.turn_rate
	var target := Vector3(
		controls.get("pitch", 0.0) * turn_rate,
		controls.get("yaw", 0.0) * turn_rate,
		controls.get("roll", 0.0) * turn_rate * 1.6,
	)
	_angular_velocity = _angular_velocity.lerp(target, 1.0 - exp(-TURN_RESPONSE * delta))
	var b := _ship.basis
	b = b.rotated(b.x, _angular_velocity.x * delta)
	b = b.rotated(b.y, _angular_velocity.y * delta)
	b = b.rotated(b.z, _angular_velocity.z * delta)
	_ship.basis = b.orthonormalized()


func _apply_thrust(delta: float, controls: Dictionary) -> void:
	_boosting = controls.get("boost", false)
	var boost_factor: float = _stats.boost_multiplier if _boosting else 1.0
	var speed: float = _stats.top_speed * boost_factor
	var accel: float = _stats.acceleration * boost_factor
	var throttle: float = controls.get("throttle", 0.0)
	var strafe: float = controls.get("strafe", 0.0)
	# Legacy strafe support via keyboard fallback
	if strafe == 0.0:
		strafe = Input.get_axis("strafe_left", "strafe_right") * 0.6
	var desired := (-_ship.basis.z * throttle + _ship.basis.x * strafe) * speed
	if Input.is_action_pressed("brake"):
		desired = Vector3.ZERO
		accel *= 1.5
	elif desired.length() > 0.1 and _velocity.length() > 0.1:
		var drifted := _velocity.normalized() * desired.length()
		desired = desired.lerp(drifted, _stats.drift)
	_velocity = _velocity.move_toward(desired, accel * delta)


func _apply_banking(delta: float) -> void:
	var yaw_input := Input.get_axis("yaw_right", "yaw_left")
	_hull.rotation.z = lerp_angle(
		_hull.rotation.z, -yaw_input * deg_to_rad(_stats.bank_angle_deg),
		1.0 - exp(-6.0 * delta))
	var pitch_input := Input.get_axis("pitch_down", "pitch_up")
	_hull.rotation.x = lerp_angle(_hull.rotation.x, pitch_input * 0.12, 1.0 - exp(-6.0 * delta))


func _update_camera(delta: float) -> void:
	_camera.global_transform = _camera.global_transform.interpolate_with(
		_camera_rest_transform(), 1.0 - exp(-CAMERA_RESPONSE * delta))
	var target_fov := BOOST_FOV if _boosting else BASE_FOV
	_camera.fov = lerpf(_camera.fov, target_fov, 1.0 - exp(-4.0 * delta))


func _camera_rest_transform() -> Transform3D:
	return _ship.global_transform.translated_local(CAMERA_OFFSET)


func _load_flight_stats() -> void:
	var ship_id: String = GameState.player.ship.hull_id
	if ship_id == "":
		ship_id = DataRegistry.start_config().get("player_ship", "")
	if ship_id == "":
		_ship_label.text = "test hull (no content loaded)"
		return
	var hull := DataRegistry.get_entity("ships", ship_id)
	_ship_label.text = str(hull.get("name", ship_id))
	var flight: Dictionary = hull.get("flight", {})
	for key: String in _stats:
		if flight.has(key):
			_stats[key] = flight[key]
	# Owned upgrades tune the hull (upgrade contract: speed_mult).
	_stats.top_speed = float(_stats.top_speed) * GameState.upgrade_effect_product("speed_mult")


func _advance_tick(delta: float) -> void:
	# SP tick driver v0: one universe tick per real second in space.
	_tick_accumulator += delta
	while _tick_accumulator >= 1.0:
		_tick_accumulator -= 1.0
		GameState.universe.tick += 1


## --- the slice: mining -------------------------------------------------------


func _build_minable_rocks() -> void:
	if not _location.has("mining"):
		return
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.5, 0.42, 0.3)
	material.roughness = 0.9
	material.emission_enabled = true
	material.emission = Color(0.35, 0.2, 0.05)
	material.emission_energy_multiplier = 0.5
	for i in 6:
		var rock := MeshInstance3D.new()
		var mesh := SphereMesh.new()
		mesh.radius = 3.0
		mesh.height = 6.0
		mesh.radial_segments = 10
		mesh.rings = 6
		rock.mesh = mesh
		rock.material_override = material
		var angle := TAU * i / 6.0
		rock.position = Vector3(cos(angle) * 70.0, _rng.randf_range(-12.0, 12.0), sin(angle) * 70.0 - 40.0)
		rock.scale = Vector3.ONE * _rng.randf_range(0.8, 1.6)
		add_child(rock)
		_minable.append({"node": rock, "ore": 3})


func _nearest_minable() -> Dictionary:
	var best := {}
	var best_dist := MINE_RANGE
	for entry in _minable:
		if entry.ore <= 0:
			continue
		var dist: float = _ship.global_position.distance_to((entry.node as Node3D).global_position)
		if dist < best_dist:
			best_dist = dist
			best = entry
	return best


func _update_mining(delta: float) -> void:
	var target := _nearest_minable()
	if target.is_empty() or not Input.is_action_pressed("mine"):
		_mine_progress = 0.0
		return
	var richness: float = _location.get("mining", {}).get("richness", 1.0)
	_mine_progress += delta * richness
	if _mine_progress >= MINE_SECONDS_PER_UNIT:
		_mine_progress = 0.0
		target.ore -= 1
		_ore_mined_total += 1
		GameState.add_cargo(_location.mining.good, 1)
		MissionManager.report_event("ore_mined", {"good": _location.mining.good})
		(target.node as Node3D).scale *= 0.82
		if target.ore <= 0:
			(target.node as MeshInstance3D).visible = false
		if _ore_mined_total == 1 and not _ambush_done:
			_spawn_pirate()


## --- the slice: the ambush ----------------------------------------------------


func _spawn_pirate() -> void:
	# The ambusher: first NPC present in this system's drift with a ship that
	# isn't the player's — content decides who jumps you. v0: the location's
	# extra.ambusher, else any npc whose role is 'pirate'.
	_pirate_soul = _find_ambusher()
	if _pirate_soul.is_empty():
		return
	_pirate_hull = DataRegistry.get_entity("ships", _pirate_soul.get("ship", ""))
	_pirate = _build_ship(Color(0.55, 0.2, 0.16))
	_pirate.position = _ship.global_position + Vector3(0, 8, -120)
	add_child(_pirate)
	_pirate_hp = 3 + int(_pirate_hull.get("stats", {}).get("armor", 1)) * 2
	_pirate_fleeing = false
	_hint_label.text = "CONTACT — %s (%s) closing fast" % [
		_pirate_soul.get("name", "?"), _pirate_hull.get("name", "unknown skiff")]


func _find_ambusher() -> Dictionary:
	for npc_id in DataRegistry.ids("npcs"):
		var npc := DataRegistry.get_entity("npcs", npc_id)
		if npc.get("role", "") == "pirate" and npc.get("ship", "") != GameState.player.ship.hull_id:
			return npc
	return {}


func _update_combat(delta: float) -> void:
	_fire_clock = maxf(0.0, _fire_clock - delta)
	var fire_pressed := false
	if ShipOperation.is_active() and ShipOperation.is_occupied("weapons"):
		var wc: Dictionary = ShipOperation.controls.get("weapons", {})
		fire_pressed = wc.get("fire", false)
	else:
		fire_pressed = Input.is_action_pressed("fire")
	
	if not fire_pressed or _fire_clock > 0.0:
		ShipOperation.set_effect("weapons_firing", false)
		return
	_fire_clock = FIRE_COOLDOWN
	ShipOperation.set_effect("weapons_firing", true)
	AudioManager.laser_fire()
	var damage := 1 + int(GameState.upgrade_effect_sum("damage_bonus"))
	if _pirate != null and _in_fire_cone(_pirate.global_position):
		_pirate_hp -= damage
		_pirate.scale = Vector3.ONE * 1.15  # hit flash, decays in _update_pirate
		if _pirate_hp <= 1 and not _pirate_fleeing:
			_pirate_fleeing = true  # a coward's math: one more hit isn't worth it
			_hint_label.text = "%s is running for it" % _pirate_soul.get("name", "The pirate")
		if _pirate_hp <= 0:
			_end_ambush(true)
	for pc: PatrolController in _patrols:
		if not pc.is_alive():
			continue
		if not _in_fire_cone(pc.global_position):
			continue
		if pc.hit(damage):
			MissionManager.report_event("patrol_destroyed", {"faction": pc.faction_id()})
			# Shooting down a patrol is loud: the faction remembers.
			if pc.faction_id() != "":
				GameState.adjust_faction_standing(pc.faction_id(), "notoriety", 10)
				GameState.adjust_faction_standing(pc.faction_id(), "trust", -10)


func _in_fire_cone(target: Vector3) -> bool:
	var to_target := target - _ship.global_position
	if to_target.length() > FIRE_RANGE:
		return false
	return (-_ship.basis.z).angle_to(to_target.normalized()) <= deg_to_rad(FIRE_CONE_DEG)


func _update_pirate(delta: float) -> void:
	if _pirate == null:
		return
	_pirate.scale = _pirate.scale.lerp(Vector3.ONE, 1.0 - exp(-8.0 * delta))
	var pirate_flight: Dictionary = _pirate_hull.get("flight", {})
	var pirate_speed: float = pirate_flight.get("top_speed", 40.0)
	var to_player: Vector3 = _ship.global_position - _pirate.global_position
	var dist := to_player.length()
	if _pirate_fleeing:
		_pirate.global_position -= to_player.normalized() * pirate_speed * 1.1 * delta
		if dist > 400.0:
			_end_ambush(false)
		return
	# pursue to preferred range, then strafe
	var heading := to_player.normalized()
	if dist > 45.0:
		_pirate.global_position += heading * pirate_speed * delta
	else:
		_pirate.global_position += heading.cross(Vector3.UP).normalized() * pirate_speed * 0.6 * delta
	_pirate.look_at(_ship.global_position, Vector3.UP)
	# return fire
	_pirate_fire_clock += delta
	if _pirate_fire_clock >= PIRATE_FIRE_INTERVAL and dist <= PIRATE_FIRE_RANGE:
		_pirate_fire_clock = 0.0
		if _rng.randf() < PIRATE_HIT_CHANCE:
			_player_hit()


func _player_hit() -> void:
	GameState.player.ship.hull_integrity = maxf(
		0.0, GameState.player.ship.hull_integrity - PIRATE_HIT_DAMAGE)
	GameState.set_flag("took_the_hit")
	_camera.fov += 3.0  # a flinch the lerp settles
	if GameState.player.ship.hull_integrity <= 0.0:
		_player_downed()


func _end_ambush(destroyed: bool) -> void:
	_ambush_done = true
	GameState.set_flag("survived_ambush")
	MissionManager.report_event("survived_ambush")
	if _pirate != null:
		_pirate.queue_free()
		_pirate = null
	_hint_label.text = "Skiff destroyed. The drift is quiet again." if destroyed \
		else "The skiff jumps trace and runs. The drift is quiet again."


func _player_downed() -> void:
	# An active mission that fails on ship loss owns what happens next — the
	# epilogue card presents the ending and rewinds to the last save.
	var was_active := MissionManager.is_active()
	MissionManager.report_event("ship_destroyed")
	if was_active and not MissionManager.is_active():
		GameState.player.ship.hull_integrity = 0.35
		return
	# Design doc §2: emergency — you wake up in the med bay. The station takes
	# you in; half the hold pays for the tow.
	for good_id in GameState.player.ship.cargo.keys():
		GameState.player.ship.cargo[good_id] = int(GameState.player.ship.cargo[good_id]) / 2
	GameState.player.ship.hull_integrity = 0.35
	GameState.player.location = _location.get("id", "")
	GameManager.request_mode(GameManager.Mode.LANDED)


## --- the slice: docking --------------------------------------------------------


func _update_docking() -> void:
	if _station == null:
		return
	var dist := _ship.global_position.distance_to(_station.global_position)
	if dist <= DOCK_RANGE and Input.is_action_just_pressed("interact"):
		GameState.player.location = _location.get("id", "")
		GameState.save_game()  # docking is the natural checkpoint
		on_dock_initiated(_location.get("id", ""))


## ___patrols__________________________________________________________________
var _patrols: Array = []  # Active PatrolController instances


func _build_patrols() -> void:
	var patrol_data: Array = _location.get("patrols", [])
	for entry: Dictionary in patrol_data:
		var count: int = entry.get("count", 1)
		var faction: String = entry.get("faction", "")
		var hull_id: String = entry.get("ship", "")
		var color_str: String = entry.get("color", "#8c3030")
		var color: Color = Color.from_string(color_str, Color(0.5, 0.2, 0.15))
		var engagement: String = entry.get("engagement", "passive")
		
		for i in count:
			var pc := PatrolController.new()
			var mock_soul: Dictionary = {
				"id": entry.get("id", "patrol_%d" % _rng.randi()),
				"faction": faction,
				"color": entry.get("color", "#8c3030"),
			}
			var hull: Dictionary = DataRegistry.get_entity("ships", hull_id)
			if hull.is_empty():
				continue
			pc.configure(mock_soul, hull, _ship)
			pc.set_hostile(engagement in ["engage", "ambush"])
			pc.set_detection_multiplier(GameState.upgrade_effect_product("detection_mult"))
			pc.set_patrol_route(_random_route())
			pc.engagement_started.connect(_on_patrol_alert.bind(pc))
			pc.destroyed.connect(_on_patrol_destroyed.bind(pc))
			pc.fired.connect(_on_patrol_fired)

			# Position patrol at a random offset from the player
			var theta := _rng.randf() * TAU
			var dist := _rng.randf_range(60.0, 100.0)
			pc.global_position = _ship.global_position + Vector3(cos(theta) * dist, 0, sin(theta) * dist)
			add_child(pc)
			_patrols.append(pc)


func _update_patrols(delta: float) -> void:
	for i in range(_patrols.size() - 1, -1, -1):
		var pc: PatrolController = _patrols[i]
		if not pc.is_alive():
			_patrols.remove_at(i)
			continue


func _random_route() -> Array:
	var route: Array = []
	var center := _ship.global_position if is_instance_valid(_ship) else Vector3.ZERO
	for i in 3:
		var theta := _rng.randf() * TAU
		var dist := _rng.randf_range(40.0, 80.0)
		route.append(center + Vector3(cos(theta) * dist, 0, sin(theta) * dist))
	return route


func _on_patrol_alert(_ship_id: String) -> void:
	# A patrol called reinforcements — spawn another patrol nearby
	var theta := _rng.randf() * TAU
	var dist := _rng.randf_range(80.0, 120.0)
	# Reuse the first patrol entry's config
	var patrol_data: Array = _location.get("patrols", [])
	if patrol_data.is_empty():
		return
	var entry: Dictionary = patrol_data[0]
	var hull: Dictionary = DataRegistry.get_entity("ships", entry.get("ship", ""))
	if hull.is_empty():
		return
	var pc := PatrolController.new()
	var mock_soul: Dictionary = {"id": "reinforce_%d" % _rng.randi(), "faction": entry.get("faction", ""), "color": entry.get("color", "#8c3030")}
	pc.configure(mock_soul, hull, _ship)
	pc.global_position = _ship.global_position + Vector3(cos(theta) * dist, 0, sin(theta) * dist)
	pc.set_patrol_route([_ship.global_position])
	pc.destroyed.connect(_on_patrol_destroyed.bind(pc))
	add_child(pc)
	_patrols.append(pc)


func _on_patrol_destroyed(_ship_id: String, _patrol: Node3D) -> void:
	pass


## A patrol fired on us. Kinetic slugs at range: a coin-flip hit, real damage.
func _on_patrol_fired(_ship_id: String) -> void:
	if _rng.randf() < PIRATE_HIT_CHANCE:
		_player_hit()


## ___self_jump________________________________________________________________
##
## A location with a `self_jump` route lets a jump-capable hull leave without
## a gate — the Duskway's whole trick. The organic crew must be in cryo for
## the crossing (GAME-DESIGN.md §6.3): the CryoTransit sequence is mandatory,
## not decorative.


func _update_self_jump() -> void:
	if _jump_route.is_empty() or _transit != null:
		return
	if not Input.is_action_just_pressed("jump"):
		return
	var hull := DataRegistry.get_entity("ships", GameState.player.ship.hull_id)
	if not hull.get("stats", {}).get("jump_drive_capable", false):
		_hint_label.text = "No jump drive on this hull — a gate is the only way out."
		return
	_transit = CryoTransit.new()
	_transit.finished.connect(_on_self_jump_finished)
	add_child(_transit)
	_transit.begin(_jump_route)
	ShipOperation.set_effect("engine_glow", 1.0)


func _on_self_jump_finished(route: Dictionary) -> void:
	var to_id: String = route.get("to", "")
	if not GameState.player.has("travel_log"):
		GameState.player["travel_log"] = []
	GameState.player.travel_log.append({
		"from": _location.get("id", ""), "to": to_id,
		"time": Time.get_unix_time_from_system(), "kind": "self_jump",
	})
	GameState.player.current_space = to_id
	MissionManager.report_event("self_jump_completed", {"to": to_id})
	# Rebuild space flight for the new system.
	GameManager.request_mode(GameManager.Mode.SPACE_FLIGHT)


## ___jump_gate________________________________________________________________


func _build_jump_gate() -> void:
	var gates: Array = _location.get("jump_gates", [])
	for entry: Dictionary in gates:
		var gate := JumpGate.new()
		gate.configure(entry, _ship)
		gate.global_position = _random_gate_position(entry)
		gate.transit_completed.connect(_on_transit_completed)
		gate.transit_failed.connect(func(reason: String) -> void:
			_hint_label.text = "Gate: %s" % reason)
		add_child(gate)


func _random_gate_position(_entry: Dictionary) -> Vector3:
	var theta := _rng.randf() * TAU
	var dist := _rng.randf_range(100.0, 140.0)
	return _ship.global_position + Vector3(cos(theta) * dist, 0, sin(theta) * dist)


func _on_transit_completed(from_system: String, to_system: String) -> void:
	_hint_label.text = "Transit complete — entering %s" % to_system
	MissionManager.report_event("jump_completed", {"to": to_system})
	# Update the save data and reload the flight scene for the new location
	if not GameState.player.has("travel_log"):
		GameState.player["travel_log"] = []
	GameState.player.travel_log.append({"to": to_system, "from": from_system, "time": Time.get_unix_time_from_system()})
	GameState.save_game()
	# Force a location change by triggering the sim to update
	SimGateway.navigate_to(to_system, "jumpgate")


## --- HUD ---------------------------------------------------------------------


func _update_hud() -> void:
	var status := "  [BOOST]" if _boosting else ""
	_speed_label.text = "%3.0f u/s%s" % [_velocity.length(), status]
	if _pirate != null and not _pirate_fleeing:
		var dist := _ship.global_position.distance_to(_pirate.global_position)
		_hint_label.text = "%s — hull %d — %.0f u  (CTRL to fire)" % [
			_pirate_soul.get("name", "hostile"), _pirate_hp, dist]
	elif _station != null and _ship.global_position.distance_to(_station.global_position) <= DOCK_RANGE:
		_hint_label.text = "R — dock at %s" % _location.get("name", "station")
	elif not _nearest_minable().is_empty():
		_hint_label.text = "hold F — mine %s" % str(_location.get("mining", {}).get("good", "ore")).replace("_", " ")
	elif not _jump_route.is_empty() and _transit == null:
		_hint_label.text = "J — jump: %s (crew enters cryo)" % _jump_route.get("name", _jump_route.get("to", "?"))
	elif _ambush_done or _ore_mined_total == 0:
		_hint_label.text = ""


func _refresh_status() -> void:
	var cargo_units := 0
	for qty in GameState.player.ship.cargo.values():
		cargo_units += int(qty)
	_status_label.text = "Hull %d%%   Cargo %d   Credits %d" % [
		int(GameState.player.ship.hull_integrity * 100.0), cargo_units, GameState.player.credits]


## --- placeholder world (unchanged visuals + station) ---------------------------


func _build_environment() -> void:
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.006, 0.008, 0.016)
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.25, 0.28, 0.38)
	env.ambient_light_energy = 0.6
	var world_env := WorldEnvironment.new()
	world_env.environment = env
	add_child(world_env)


func _build_ship(base_color: Color) -> Node3D:
	var root := Node3D.new()
	var hull := Node3D.new()
	hull.name = "Hull"
	root.add_child(hull)
	var body_material := StandardMaterial3D.new()
	body_material.albedo_color = base_color
	body_material.metallic = 0.55
	body_material.roughness = 0.45
	var body := MeshInstance3D.new()
	var prism := PrismMesh.new()
	prism.size = Vector3(1.7, 3.4, 0.7)
	body.mesh = prism
	body.rotation_degrees.x = -90.0
	body.material_override = body_material
	hull.add_child(body)
	var wings := MeshInstance3D.new()
	var wing_mesh := BoxMesh.new()
	wing_mesh.size = Vector3(3.4, 0.14, 1.3)
	wings.mesh = wing_mesh
	wings.position = Vector3(0.0, -0.05, 0.7)
	wings.material_override = body_material
	hull.add_child(wings)
	var glow_material := StandardMaterial3D.new()
	glow_material.emission_enabled = true
	glow_material.emission = Color(1.0, 0.55, 0.15)
	glow_material.emission_energy_multiplier = 3.0
	glow_material.albedo_color = Color(0.2, 0.1, 0.02)
	for x: float in [-0.55, 0.55]:
		var engine := MeshInstance3D.new()
		var engine_mesh := BoxMesh.new()
		engine_mesh.size = Vector3(0.3, 0.3, 0.5)
		engine.mesh = engine_mesh
		engine.position = Vector3(x, 0.0, 1.6)
		engine.material_override = glow_material
		hull.add_child(engine)
	if _hull == null:
		_hull = hull
	return root


func _build_station() -> Node3D:
	# Only dockable places get a dock beacon in their space; a bare drift
	# or blockade picket line has nothing to park at.
	if _location.is_empty() or "dock" not in _location.get("services", []):
		return null
	var station := Node3D.new()
	station.name = "Station"
	station.position = Vector3(30, 6, -150)
	var core_material := StandardMaterial3D.new()
	core_material.albedo_color = Color(0.3, 0.33, 0.4)
	core_material.metallic = 0.4
	core_material.roughness = 0.6
	var core := MeshInstance3D.new()
	var core_mesh := CylinderMesh.new()
	core_mesh.top_radius = 6.0
	core_mesh.bottom_radius = 6.0
	core_mesh.height = 18.0
	core.mesh = core_mesh
	core.material_override = core_material
	station.add_child(core)
	var ring := MeshInstance3D.new()
	var ring_mesh := TorusMesh.new()
	ring_mesh.inner_radius = 12.0
	ring_mesh.outer_radius = 16.0
	ring.mesh = ring_mesh
	ring.material_override = core_material
	station.add_child(ring)
	var beacon_material := StandardMaterial3D.new()
	beacon_material.emission_enabled = true
	beacon_material.emission = Color(0.3, 0.9, 0.5)
	beacon_material.emission_energy_multiplier = 4.0
	var beacon := MeshInstance3D.new()
	var beacon_mesh := BoxMesh.new()
	beacon_mesh.size = Vector3(1.5, 1.5, 1.5)
	beacon.mesh = beacon_mesh
	beacon.position = Vector3(0, 12, 0)
	beacon.material_override = beacon_material
	station.add_child(beacon)
	return station


func _build_starfield() -> MultiMeshInstance3D:
	var rng := RandomNumberGenerator.new()
	rng.seed = 0x5747A5
	var mesh := BoxMesh.new()
	mesh.size = Vector3.ONE * 0.6
	var material := StandardMaterial3D.new()
	material.emission_enabled = true
	material.emission = Color(0.9, 0.92, 1.0)
	material.emission_energy_multiplier = 2.0
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mesh.material = material
	var multimesh := MultiMesh.new()
	multimesh.transform_format = MultiMesh.TRANSFORM_3D
	multimesh.mesh = mesh
	multimesh.instance_count = 1400
	for i in multimesh.instance_count:
		var direction := Vector3(rng.randfn(), rng.randfn(), rng.randfn()).normalized()
		multimesh.set_instance_transform(i, Transform3D(
			Basis.from_scale(Vector3.ONE * rng.randf_range(0.5, 1.6)),
			direction * rng.randf_range(750.0, 950.0)))
	var instance := MultiMeshInstance3D.new()
	instance.name = "Starfield"
	instance.multimesh = multimesh
	return instance


func _build_asteroid_field() -> MultiMeshInstance3D:
	var rng := RandomNumberGenerator.new()
	rng.seed = 0xA57E401D
	var mesh := SphereMesh.new()
	mesh.radius = 1.0
	mesh.height = 2.0
	mesh.radial_segments = 8
	mesh.rings = 5
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.35, 0.32, 0.3)
	material.roughness = 0.95
	mesh.material = material
	var multimesh := MultiMesh.new()
	multimesh.transform_format = MultiMesh.TRANSFORM_3D
	multimesh.mesh = mesh
	multimesh.instance_count = 180
	for i in multimesh.instance_count:
		var direction := Vector3(rng.randfn(), rng.randfn() * 0.35, rng.randfn()).normalized()
		var basis := Basis.from_euler(Vector3(
			rng.randf_range(0.0, TAU), rng.randf_range(0.0, TAU), rng.randf_range(0.0, TAU)
		)).scaled(Vector3(
			rng.randf_range(2.0, 14.0), rng.randf_range(2.0, 10.0), rng.randf_range(2.0, 14.0)))
		multimesh.set_instance_transform(i, Transform3D(basis, direction * rng.randf_range(60.0, 520.0)))
	var instance := MultiMeshInstance3D.new()
	instance.name = "AsteroidField"
	instance.multimesh = multimesh
	return instance


## --- mode transitions ------------------------------------------------------


func on_dock_initiated(_station_id: String) -> void:
	GameManager.request_mode(GameManager.Mode.LANDED)


func on_board_ship() -> void:
	GameManager.request_mode(GameManager.Mode.ON_BOARD)
