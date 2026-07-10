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
var _calibrated_fire_mult := 1.0   # gunnery calibration payout (consumed)
var _damage_cooldown_mult := 1.0   # interior damage: guns cycle slower
var _damage_vulnerability := 1.0   # interior damage: hits land harder

# The hail window: a patrol that catches you clean would rather have your
# money than your debris. One fee per run; word travels.
var _hail_patrol: PatrolController = null
var _hail_window := 0.0
var _hail_cost := 0
var _bribe_used := false

# The decoy beacon (upgrade effect decoy_charges): one loud lie per flight.
var _decoy_charges := 0
var _decoy_node: Node3D = null

# Cargo spilled from a breached hold: drifting canisters, briefly recoverable.
var _spilled: Array = []  # {node, good, vel:Vector3}

var _alert_rect: ColorRect = null  # red-alert vignette
var _alert_time := 0.0
var _callout_box: VBoxContainer = null
var _radar: _Radar = null
var _beacon_mats: Array = []       # pulsing station running lights
var _said: Dictionary = {}         # one-shot crew callout keys

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
	_build_planet_backdrop()
	_build_combat_overlay()
	_camera.fov = BASE_FOV
	_camera.global_transform = _camera_rest_transform()
	_refresh_status()
	GameState.state_changed.connect(_refresh_status)

	# Arriving inside a hostile picket line is its own kind of morning.
	var hostiles := false
	for entry: Dictionary in _location.get("patrols", []):
		if entry.get("engagement", "") in ["engage", "ambush"]:
			hostiles = true
	if hostiles:
		_start_red_alert(3.0)
		_callout_by_role(["droid", "pilot"],
			"Cordon contacts on the board. They aren't curious — they're hunting. Fly small.")
		var stealth := GameState.upgrade_effect_product("detection_mult")
		if stealth < 0.999:
			_callout_by_role(["engineer"],
				"Transponder's lying beautifully — signature at %d%%. Keep it dark and slow." % int(round(stealth * 100)))
	
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
	_update_spilled(delta)
	_update_hail(delta)
	_update_gadget()
	_update_docking()
	_update_self_jump()
	_update_combat_theater(delta)
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
	# Owned upgrades tune the hull (upgrade contract: speed_mult, turn_mult),
	# the engineering power grid decides how much of the drive reaches the
	# engines this flight (even split ≈ 1.0), the pilot's hands matter
	# (piloting stat), and unrepaired interior damage bleeds it all.
	var penalty := GameState.flight_damage_penalty()
	var pilot_touch := 1.0 + 0.04 * float(GameState.player_stat("piloting") - 2)
	_stats.top_speed = float(_stats.top_speed) * GameState.upgrade_effect_product("speed_mult") \
		* (0.7 + GameState.power_share("engines") * 0.9) \
		* pilot_touch * float(penalty.speed_mult)
	_stats.turn_rate = float(_stats.turn_rate) * GameState.upgrade_effect_product("turn_mult") \
		* (1.0 + 0.05 * float(GameState.player_stat("piloting") - 2))
	# The gunnery calibration run pays out here — and is spent.
	_calibrated_fire_mult = 1.15 if GameState.consume_weapons_calibration() else 1.0
	_damage_cooldown_mult = float(penalty.cooldown_mult)
	_damage_vulnerability = float(penalty.vulnerability)
	# The decoy beacon rebuilds its charge at dock: one lie per flight.
	_decoy_charges = int(GameState.upgrade_effect_sum("decoy_charges"))
	if not GameState.ship_damage().is_empty():
		# Deferred: stats load before the combat overlay (and its callout
		# box) exists; the line lands once the HUD is up.
		_callout_by_role.call_deferred(["engineer", "droid"],
			"We're flying wounded — %d open damage reports below decks. She'll be slow and she'll be sore." %
			GameState.ship_damage().size(), "flying_wounded")


func _advance_tick(delta: float) -> void:
	# SP tick driver v0: one universe tick per real second in space.
	_tick_accumulator += delta
	while _tick_accumulator >= 1.0:
		_tick_accumulator -= 1.0
		GameState.universe.tick += 1


## --- the slice: mining -------------------------------------------------------


## Rocks are individuals: each carries a prospector's name (location
## `mining.rock_names` — nine days on a claim, you name things), a seam
## tier that changes how fast it cuts and how it glows, and a finite hold
## of ore. A spent rock cracks apart in front of you; the next one is
## somewhere out there, and the rich ones are worth the flying.
const SEAM_TIERS := [
	{"label": "lean seam", "speed": 0.65, "glow": 0.25, "ore": 2},
	{"label": "fair seam", "speed": 1.0, "glow": 0.55, "ore": 3},
	{"label": "rich seam", "speed": 1.6, "glow": 1.1, "ore": 4},
]


func _build_minable_rocks() -> void:
	if not _location.has("mining"):
		return
	var names: Array = _location.get("mining", {}).get("rock_names", [])
	var count := maxi(6, names.size())
	for i in count:
		var tier: Dictionary = SEAM_TIERS[_rng.randi() % SEAM_TIERS.size()]
		var material := StandardMaterial3D.new()
		material.albedo_color = Color(0.5, 0.42, 0.3)
		material.roughness = 0.9
		material.emission_enabled = true
		material.emission = Color(0.5, 0.3, 0.08)
		material.emission_energy_multiplier = float(tier.glow)
		var rock := MeshInstance3D.new()
		var mesh := SphereMesh.new()
		mesh.radius = 3.0
		mesh.height = 6.0
		mesh.radial_segments = 10
		mesh.rings = 6
		rock.mesh = mesh
		rock.material_override = material
		var angle := TAU * i / float(count)
		rock.position = Vector3(cos(angle) * _rng.randf_range(60.0, 95.0),
			_rng.randf_range(-14.0, 14.0), sin(angle) * _rng.randf_range(60.0, 95.0) - 40.0)
		# Lumpy, not spherical: every rock has a shape of its own.
		rock.scale = Vector3(_rng.randf_range(0.7, 1.7), _rng.randf_range(0.6, 1.3),
			_rng.randf_range(0.7, 1.7))
		add_child(rock)
		_minable.append({
			"node": rock,
			"ore": int(tier.ore),
			"name": str(names[i]) if i < names.size() else "Rock %d" % (i + 1),
			"tier": tier,
		})


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
	var richness: float = _location.get("mining", {}).get("richness", 1.0) \
		* float(target.get("tier", {}).get("speed", 1.0))
	_mine_progress += delta * richness
	if _mine_progress >= MINE_SECONDS_PER_UNIT:
		_mine_progress = 0.0
		target.ore -= 1
		_ore_mined_total += 1
		GameState.add_cargo(_location.mining.good, 1)
		MissionManager.report_event("ore_mined", {"good": _location.mining.good})
		# The chunk breaks off and comes home: the cut you can feel.
		_fly_chunk_to_ship(target.node as Node3D)
		AudioManager.play("ui_click", 0.9)
		(target.node as Node3D).scale *= 0.84
		if _ore_mined_total == 1:
			_callout_by_role(["smuggler", "pilot"],
				"Clean cut. Ride the seam till the rock's bones show — every unit in the hold is credit at Sorrow.", "first_cut")
		if target.ore <= 0:
			_crack_rock(target)
		# The skiff has been watching the claim: it jumps you the moment the
		# hold is worth taking — right when the drift was getting generous.
		if _ore_mined_total == 3 and not _ambush_done:
			_spawn_pirate()


## A fist-sized chunk splits off the rock face and arcs into the hold.
func _fly_chunk_to_ship(rock: Node3D) -> void:
	var chunk := MeshInstance3D.new()
	var mesh := SphereMesh.new()
	mesh.radius = 0.6
	mesh.height = 1.2
	mesh.radial_segments = 6
	mesh.rings = 4
	chunk.mesh = mesh
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(0.62, 0.5, 0.32)
	mat.emission_enabled = true
	mat.emission = Color(0.7, 0.45, 0.15)
	chunk.material_override = mat
	add_child(chunk)
	chunk.global_position = rock.global_position \
		+ (_ship.global_position - rock.global_position).normalized() * rock.scale.length() * 2.0
	var tween := create_tween()
	tween.tween_property(chunk, "global_position",
		_ship.global_position, 0.55).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
	tween.parallel().tween_property(chunk, "scale", Vector3.ONE * 0.4, 0.55)
	tween.tween_callback(chunk.queue_free)


## A spent rock doesn't vanish — it cracks into drifting shards, and the
## scope already knows where the next seam is.
func _crack_rock(entry: Dictionary) -> void:
	var rock: Node3D = entry.node
	for i in 3:
		var shard := MeshInstance3D.new()
		var mesh := SphereMesh.new()
		mesh.radius = 0.9
		mesh.height = 1.6
		mesh.radial_segments = 6
		mesh.rings = 4
		shard.mesh = mesh
		shard.material_override = (rock as MeshInstance3D).material_override
		add_child(shard)
		shard.global_position = rock.global_position
		var drift := Vector3(_rng.randf_range(-1, 1), _rng.randf_range(-0.4, 0.4),
			_rng.randf_range(-1, 1)).normalized() * _rng.randf_range(4.0, 9.0)
		var tween := create_tween()
		tween.tween_property(shard, "global_position",
			rock.global_position + drift, 2.2)
		tween.parallel().tween_property(shard, "transparency", 1.0, 2.2)
		tween.tween_callback(shard.queue_free)
	(rock as MeshInstance3D).visible = false
	AudioManager.play("explosion", 0.4)
	var richest := _richest_remaining()
	if not richest.is_empty():
		_hint_label.text = "%s is bones. Scope reads %s two burns out — %s." % [
			entry.get("name", "That rock"), richest.get("name", "another rock"),
			str(richest.get("tier", {}).get("label", "seam"))]


func _richest_remaining() -> Dictionary:
	var best := {}
	var best_speed := 0.0
	for entry: Dictionary in _minable:
		if int(entry.ore) <= 0:
			continue
		var speed := float(entry.get("tier", {}).get("speed", 1.0))
		if speed > best_speed:
			best_speed = speed
			best = entry
	return best


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
	_start_red_alert(4.0)
	_callout_by_role(["droid", "pilot"],
		"Contact! %s closing on an attack vector — weapons are yours, captain." % _pirate_hull.get("name", "unregistered skiff"))
	_callout_by_role(["engineer"], "Drive's hot, shields are what they are. Make it quick.")


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
	# Weapons power feeds the fire rate (even split ≈ stock cooldown); the
	# calibration run speeds the cycle, unrepaired damage drags on it.
	_fire_clock = FIRE_COOLDOWN * _damage_cooldown_mult \
		/ ((0.6 + GameState.power_share("weapons") * 1.2) * _calibrated_fire_mult)
	ShipOperation.set_effect("weapons_firing", true)
	AudioManager.laser_fire()
	var damage := 1 + int(GameState.upgrade_effect_sum("damage_bonus"))
	var hit_something := false
	if _pirate != null and _in_fire_cone(_pirate.global_position):
		hit_something = true
		_fire_tracer(_ship.global_position - _ship.basis.z * 3.0,
			_pirate.global_position, Color(1.0, 0.8, 0.4))
		_pirate_hp -= damage
		_pirate.scale = Vector3.ONE * 1.15  # hit flash, decays in _update_pirate
		if _pirate_hp <= 1 and not _pirate_fleeing:
			_pirate_fleeing = true  # a coward's math: one more hit isn't worth it
			_hint_label.text = "%s is running for it" % _pirate_soul.get("name", "The pirate")
		if _pirate_hp <= 0:
			AudioManager.explosion()
			_end_ambush(true)
	for pc: PatrolController in _patrols:
		if not pc.is_alive():
			continue
		if not _in_fire_cone(pc.global_position):
			continue
		hit_something = true
		_fire_tracer(_ship.global_position - _ship.basis.z * 3.0,
			pc.global_position, Color(1.0, 0.8, 0.4))
		if pc.hit(damage):
			AudioManager.explosion()
			MissionManager.report_event("patrol_destroyed", {"faction": pc.faction_id()})
			# Shooting down a patrol is loud: the faction remembers.
			if pc.faction_id() != "":
				GameState.adjust_faction_standing(pc.faction_id(), "notoriety", 10)
				GameState.adjust_faction_standing(pc.faction_id(), "trust", -10)
	if not hit_something:
		# Slugs that find nothing still go somewhere: the miss is visible.
		var muzzle := _ship.global_position - _ship.basis.z * 3.0
		_fire_tracer(muzzle, muzzle - _ship.basis.z * FIRE_RANGE * 0.55,
			Color(1.0, 0.8, 0.4, 0.5))


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
	# Shield power soaks part of the slug (even split ≈ stock damage), grit
	# rides the impact out, and a ship already burning takes it worse.
	var soak := (1.15 - GameState.power_share("shields") * 0.45) \
		* (1.0 - 0.03 * float(GameState.player_stat("grit") - 2)) \
		* _damage_vulnerability
	GameState.player.ship.hull_integrity = maxf(
		0.0, GameState.player.ship.hull_integrity - PIRATE_HIT_DAMAGE * soak)
	GameState.set_flag("took_the_hit")
	_camera.fov += 3.0  # a flinch the lerp settles
	_maybe_interior_damage()
	_maybe_spill_cargo()
	var hull := int(GameState.player.ship.hull_integrity * 100.0)
	if hull <= 25:
		_callout_by_role(["medic"],
			"Hull at %d%%. If anyone's planning to bleed, do it near the med bay." % hull, "hull_25")
	elif hull <= 50:
		_callout_by_role(["engineer"],
			"Hull at %d%% and I felt that one from engineering. Fewer of those." % hull, "hull_50")
	elif hull <= 75:
		_callout_by_role(["engineer"], "Shields drank most of it. Hull at %d%%." % hull, "hull_75")
	if GameState.player.ship.hull_integrity <= 0.0:
		_player_downed()


## A hit that gets through can start something INSIDE the ship: a fire, an
## arcing conduit, a small breach — placed in a real room from the hull data,
## visible and fixable when you next walk the deck. A good medic runs triage
## that keeps the casualty list short.
func _maybe_interior_damage() -> void:
	# A ship only has so many rooms: past six open reports, new hits deepen
	# the existing fires instead of papering the deck with markers.
	if GameState.ship_damage().size() >= 6:
		for entry: Dictionary in GameState.ship_damage():
			entry["severity"] = minf(1.0, float(entry.get("severity", 1.0)) + 0.1)
		return
	var chance := 0.45 * (1.0 - 0.07 * float(GameState.player_stat("medicine") - 2))
	if GameState.player.ship.hull_integrity <= 0.25:
		chance = maxf(chance, 0.8)  # a shredded hull sheds sparks
	if _rng.randf() > clampf(chance, 0.1, 0.9):
		return
	var hull := DataRegistry.get_entity("ships", GameState.player.ship.hull_id)
	var rooms: Array = hull.get("rooms", [])
	if rooms.is_empty():
		return
	var room: Dictionary = rooms[_rng.randi() % rooms.size()]
	var kind: String = ["fire", "conduit", "breach"][_rng.randi() % 3]
	# Somewhere inside the room, clear of the walls.
	var x := float(room.get("x", 0.0)) + float(room.get("w", 100.0)) * _rng.randf_range(0.25, 0.75)
	var y := float(room.get("y", 0.0)) + float(room.get("h", 100.0)) * _rng.randf_range(0.3, 0.7)
	GameState.add_ship_damage(room.get("id", ""), kind, [x, y],
		_rng.randf_range(0.6, 1.0))
	var room_name: String = room.get("name", str(room.get("id", "?")).capitalize())
	match kind:
		"fire":
			_callout_by_role(["droid", "engineer"],
				"Fire in the %s. Logged, alarmed, and spreading at a professional pace." % room_name)
		"conduit":
			_callout_by_role(["engineer", "droid"],
				"Conduit's arcing in the %s — if the plates stutter, that's why." % room_name)
		"breach":
			_callout_by_role(["medic", "engineer"],
				"Breach in the %s! Small one. Everyone keep breathing out of habit." % room_name)


## A hit past the shields can breach the hold: loose cargo (good schema
## `loose_cargo`) vents into space as a drifting canister. Fly close and
## you get it back; jump or dock and it's gone — and nobody tells you the
## count. The manifest does, later, quietly.
func _maybe_spill_cargo() -> void:
	if _rng.randf() > 0.5:
		return
	var cargo: Dictionary = GameState.player.ship.cargo
	for good_id: String in cargo:
		if int(cargo[good_id]) < 1:
			continue
		if not bool(DataRegistry.get_entity("goods", good_id).get("loose_cargo", false)):
			continue
		GameState.add_cargo(good_id, -1)
		var canister := MeshInstance3D.new()
		var mesh := CapsuleMesh.new()
		mesh.radius = 0.5
		mesh.height = 1.6
		canister.mesh = mesh
		var mat := StandardMaterial3D.new()
		mat.albedo_color = Color(0.75, 0.6, 0.3)
		mat.emission_enabled = true
		mat.emission = Color(0.9, 0.6, 0.2)
		mat.emission_energy_multiplier = 0.8
		canister.material_override = mat
		add_child(canister)
		canister.global_position = _ship.global_position + Vector3(0, -1.5, 0)
		_spilled.append({"node": canister, "good": good_id, "age": 0.0,
			"vel": Vector3(_rng.randf_range(-1, 1), _rng.randf_range(-0.5, 0.5),
				_rng.randf_range(-1, 1)).normalized() * _rng.randf_range(4.0, 8.0)})
		_callout_by_role(["smuggler", "engineer"],
			"We're venting the hold! Cargo's going over the side!", "hold_breached")
		return


func _update_spilled(delta: float) -> void:
	for i in range(_spilled.size() - 1, -1, -1):
		var entry: Dictionary = _spilled[i]
		var node: Node3D = entry.node
		entry["age"] = float(entry.get("age", 0.0)) + delta
		node.global_position += (entry.vel as Vector3) * delta
		node.rotate_y(delta * 2.0)
		# The grace keeps a fresh spill from teleporting straight back into
		# the breach it just left: you have to TURN AROUND for it.
		if float(entry.age) > 2.5 \
				and _ship.global_position.distance_to(node.global_position) < 7.0:
			GameState.add_cargo(entry.good, 1)
			AudioManager.play("ui_switch", 0.8)
			_hint_label.text = "Canister recovered — back in the hold."
			node.queue_free()
			_spilled.remove_at(i)


## Spilled cargo does not survive a departure: what's still drifting when
## you jump or dock is the drift's now.
func _clear_spilled() -> void:
	for entry: Dictionary in _spilled:
		(entry.node as Node3D).queue_free()
	_spilled.clear()


func _end_ambush(destroyed: bool) -> void:
	_ambush_done = true
	GameState.set_flag("survived_ambush")
	MissionManager.report_event("survived_ambush")
	if _pirate != null:
		_pirate.queue_free()
		_pirate = null
	_hint_label.text = "Skiff destroyed. The drift is quiet again." if destroyed \
		else "The skiff jumps trace and runs. The drift is quiet again."
	if not _jump_route.is_empty():
		_callout_by_role(["droid", "pilot"],
			"Drive can spool whenever you call it (J). The pods are prepped — you'll sleep this one, captain.", "spool_ready")


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
			pc.set_meta("bribe", int(entry.get("bribe", 0)))
			pc.engagement_started.connect(_on_patrol_alert.bind(pc))
			pc.contact_detected.connect(_on_patrol_contact.bind(pc))
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


## A hostile patrol just latched on. Some of them would rather have your
## money than your debris (location patrol entry `bribe` > 0) — a short
## hail window opens: pay the "inspection fee" and it breaks off. Once.
## The second patrol has heard about the first one's good afternoon.
func _on_patrol_contact(_ship_id: String, _faction: String, threat: int,
		pc: PatrolController) -> void:
	if threat < 2 or _bribe_used or _hail_patrol != null:
		return
	var cost := int(pc.get_meta("bribe", 0))
	if cost <= 0 or GameState.player.credits < cost:
		return
	_hail_patrol = pc
	_hail_cost = cost
	_hail_window = 6.0
	AudioManager.play("computer_noise", 1.2)
	_callout_by_role(["droid", "pilot"],
		"They're hailing, not firing. That's a price, captain — pickets this far out run their own economy.", "hail_open")


func _update_hail(delta: float) -> void:
	if _hail_patrol == null:
		return
	if not _hail_patrol.is_alive() or not _hail_patrol.is_engaged():
		_hail_patrol = null
		return
	_hail_window -= delta
	if _hail_window <= 0.0:
		_hint_label.text = "The hail window closes. They've committed."
		_hail_patrol = null
		return
	if Input.is_action_just_pressed("hail"):
		GameState.adjust_credits(-_hail_cost)
		_bribe_used = true
		_hail_patrol.stand_down()
		if _hail_patrol.faction_id() != "":
			GameState.adjust_faction_standing(_hail_patrol.faction_id(), "notoriety", 3)
		_hint_label.text = "'Inspection complete. Mind the traffic.' The picket peels off."
		_callout_by_role(["smuggler", "engineer"],
			"That fee only works once — the next boat over heard the transaction. Fly like it.", "hail_paid")
		AudioManager.play("ui_switch")
		_hail_patrol = null


## The decoy beacon (G): one loud lie per flight. Every patrol that can
## hear it goes to look at it instead of you.
func _update_gadget() -> void:
	if _decoy_charges <= 0 or not Input.is_action_just_pressed("gadget"):
		return
	_decoy_charges -= 1
	_decoy_node = MeshInstance3D.new()
	var mesh := SphereMesh.new()
	mesh.radius = 1.2
	mesh.height = 2.4
	_decoy_node.mesh = mesh
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(1.0, 0.8, 0.3)
	mat.emission_enabled = true
	mat.emission = Color(1.0, 0.7, 0.2)
	mat.emission_energy_multiplier = 3.0
	_decoy_node.material_override = mat
	add_child(_decoy_node)
	_decoy_node.global_position = _ship.global_position
	var distracted := 0
	for pc: PatrolController in _patrols:
		if not pc.is_alive():
			continue
		if pc.global_position.distance_to(_ship.global_position) < 220.0:
			pc.distract(_decoy_node.global_position, 18.0)
			distracted += 1
	if _hail_patrol != null:
		_hail_patrol = null  # the lie outbids the fee
	AudioManager.play("phase_jump", 0.5)
	_hint_label.text = "Decoy away — it's louder than you are. %d contact(s) bite. Run." % distracted
	_callout_by_role(["droid", "engineer"],
		"Beacon's screaming our transponder in the wrong place. It buys minutes, not forgiveness.", "decoy_away")
	var tween := create_tween()
	tween.tween_interval(18.0)
	tween.tween_callback(func() -> void:
		if _decoy_node != null:
			_decoy_node.queue_free()
			_decoy_node = null)


func _on_patrol_alert(_ship_id: String, _source: PatrolController = null) -> void:
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
	# Tracer from whichever patrol is closest (the shot's author, near enough).
	var nearest: PatrolController = null
	var best := INF
	for pc: PatrolController in _patrols:
		if not pc.is_alive():
			continue
		var d := pc.global_position.distance_to(_ship.global_position)
		if d < best:
			best = d
			nearest = pc
	if nearest != null:
		_fire_tracer(nearest.global_position, _ship.global_position, Color(0.95, 0.35, 0.3))
	if _rng.randf() < PIRATE_HIT_CHANCE:
		AudioManager.impact()
		_player_hit()


## ___self_jump________________________________________________________________
##
## A location with a `self_jump` route lets a jump-capable hull leave without
## a gate — the Duskway's whole trick. The organic crew must be in cryo for
## the crossing (GAME-DESIGN.md §6.3): the CryoTransit sequence is mandatory,
## not decorative.


func _update_self_jump() -> void:
	# B — step back from the stick and walk the ship (adjust power, talk
	# to the crew), any time you're not mid-transit.
	if _transit == null and Input.is_action_just_pressed("board"):
		on_board_ship()
		return
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
	_clear_spilled()  # what's still drifting stays behind
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
	if _hail_patrol != null:
		_hint_label.text = "INCOMING HAIL — H: transmit 'inspection fee' (%d cr) — %ds" % [
			_hail_cost, int(ceil(_hail_window))]
		return
	if _pirate != null and not _pirate_fleeing:
		var dist := _ship.global_position.distance_to(_pirate.global_position)
		_hint_label.text = "%s — hull %d — %.0f u  (CTRL to fire)" % [
			_pirate_soul.get("name", "hostile"), _pirate_hp, dist]
	elif _station != null and _ship.global_position.distance_to(_station.global_position) <= DOCK_RANGE:
		_hint_label.text = "R — dock at %s" % _location.get("name", "station")
	elif not _nearest_minable().is_empty():
		var rock := _nearest_minable()
		_hint_label.text = "hold F — cut into %s (%s)" % [rock.get("name", "the rock"),
			str(rock.get("tier", {}).get("label", "seam"))]
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


## --- combat theater: red alert, callouts, tracers, radar -----------------------


func _build_combat_overlay() -> void:
	var overlay: CanvasLayer = $ModeOverlay
	_alert_rect = ColorRect.new()
	_alert_rect.color = Color(0.8, 0.1, 0.08, 0.0)
	_alert_rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	_alert_rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.add_child(_alert_rect)

	_callout_box = VBoxContainer.new()
	_callout_box.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT)
	_callout_box.offset_left = 16
	_callout_box.offset_top = -260
	_callout_box.offset_right = 620
	_callout_box.offset_bottom = -60
	_callout_box.alignment = BoxContainer.ALIGNMENT_END
	overlay.add_child(_callout_box)

	_radar = _Radar.new()
	_radar.custom_minimum_size = Vector2(170, 170)
	_radar.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT)
	_radar.offset_left = -186
	_radar.offset_top = -186
	_radar.offset_right = -16
	_radar.offset_bottom = -16
	overlay.add_child(_radar)


func _start_red_alert(seconds: float) -> void:
	_alert_time = seconds
	AudioManager.alert()


func _update_combat_theater(delta: float) -> void:
	# Red alert vignette flashes then settles.
	if _alert_time > 0.0:
		_alert_time -= delta
		_alert_rect.color.a = 0.10 + 0.08 * sin(Time.get_ticks_msec() * 0.02)
	elif _alert_rect.color.a > 0.0:
		_alert_rect.color.a = maxf(0.0, _alert_rect.color.a - delta * 0.5)
	# Station running lights breathe.
	for mat: StandardMaterial3D in _beacon_mats:
		mat.emission_energy_multiplier = 2.5 + 2.0 * sin(Time.get_ticks_msec() * 0.004 + mat.get_instance_id() % 7)
	# Radar sees what the scene sees.
	if _radar != null:
		var contacts: Array = []
		for pc: PatrolController in _patrols:
			if not pc.is_alive():
				continue
			var hostile: bool = pc.current_state_name() in ["engage", "pursue", "alert"]
			contacts.append({
				"rel": pc.global_position - _ship.global_position,
				"color": Color(0.95, 0.30, 0.25) if hostile else Color(0.85, 0.55, 0.3),
				"ring": PatrolController.DETECT_RANGE * GameState.upgrade_effect_product("detection_mult"),
			})
		if _pirate != null:
			contacts.append({"rel": _pirate.global_position - _ship.global_position,
				"color": Color(0.9, 0.3, 0.5), "ring": 0.0})
		if _station != null:
			contacts.append({"rel": _station.global_position - _ship.global_position,
				"color": Color(0.4, 0.9, 0.55), "ring": 0.0})
		_radar.update_contacts(contacts, -_ship.basis.z)


## A crew voice on the intercom. `roles` is a preference order; the first
## aboard crew member holding one of them speaks (content decides who that
## is — the engine only knows roles, never names).
func _callout_by_role(roles: Array, text: String, once_key := "") -> void:
	if _callout_box == null:
		return
	if once_key != "" and _said.has(once_key):
		return
	if once_key != "":
		_said[once_key] = true
	# You don't hear your own voice on the intercom: the character the
	# player is embodying never speaks a callout.
	var voices: Array = []
	for crew_id: String in CrewRoster.aboard():
		if crew_id != GameState.player_character():
			voices.append(crew_id)
	var speaker := ""
	for role: String in roles:
		for crew_id: String in voices:
			var npc := DataRegistry.get_entity("npcs", crew_id)
			if npc.get("role", "") == role:
				speaker = npc.get("name", crew_id)
				break
		if speaker != "":
			break
	if speaker == "" and not voices.is_empty():
		var first: String = voices[0]
		speaker = DataRegistry.get_entity("npcs", first).get("name", first)
	if speaker == "":
		return
	var line := Label.new()
	line.text = "%s:  %s" % [speaker, text]
	line.add_theme_font_size_override("font_size", 15)
	line.add_theme_color_override("font_color", Color(0.92, 0.9, 0.8))
	line.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.8))
	line.add_theme_constant_override("outline_size", 3)
	line.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_callout_box.add_child(line)
	var tween := create_tween()
	tween.tween_interval(6.0)
	tween.tween_property(line, "modulate:a", 0.0, 1.5)
	tween.tween_callback(line.queue_free)


## A kinetic slug's light: a thin bright box from muzzle to target, gone in
## a blink. Cheap, and it makes every exchange legible.
func _fire_tracer(from: Vector3, to: Vector3, color: Color) -> void:
	var tracer := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	var length := from.distance_to(to)
	mesh.size = Vector3(0.15, 0.15, maxf(length, 1.0))
	tracer.mesh = mesh
	var mat := StandardMaterial3D.new()
	mat.emission_enabled = true
	mat.emission = color
	mat.emission_energy_multiplier = 4.0
	mat.albedo_color = color
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	tracer.material_override = mat
	add_child(tracer)
	tracer.global_position = (from + to) * 0.5
	if length > 0.01:
		tracer.look_at(to, Vector3.UP)
	get_tree().create_timer(0.09).timeout.connect(tracer.queue_free)


## The minimap: player centered, contacts as blips, hostile detection rings
## drawn to scale — stealth gear visibly shrinks how far they can see you.
class _Radar extends Control:
	const RANGE := 220.0  # world units across the scope radius
	var _contacts: Array = []
	var _heading := Vector3.FORWARD
	var _t := 0.0

	func update_contacts(contacts: Array, heading: Vector3) -> void:
		_contacts = contacts
		_heading = heading

	func _process(delta: float) -> void:
		_t += delta
		queue_redraw()

	func _draw() -> void:
		var center := size * 0.5
		var radius := minf(size.x, size.y) * 0.5 - 2.0
		draw_circle(center, radius, Color(0.02, 0.05, 0.04, 0.85))
		draw_arc(center, radius, 0, TAU, 48, Color(0.25, 0.55, 0.35, 0.8), 1.5)
		draw_arc(center, radius * 0.5, 0, TAU, 36, Color(0.25, 0.55, 0.35, 0.35), 1.0)
		# Heading tick
		var fwd := Vector2(_heading.x, _heading.z).normalized() if Vector2(_heading.x, _heading.z).length() > 0.01 else Vector2.UP
		draw_line(center, center + fwd * 12.0, Color(0.7, 0.95, 0.75), 1.5)
		draw_circle(center, 3.0, Color(0.85, 0.95, 0.9))
		var world_scale := radius / RANGE
		for contact: Dictionary in _contacts:
			var rel: Vector3 = contact.rel
			var flat := Vector2(rel.x, rel.z) * world_scale
			var clamped := flat.limit_length(radius - 5.0)
			var at_edge := flat.length() > radius - 5.0
			var color: Color = contact.color
			if at_edge:
				color.a = 0.5
			draw_circle(center + clamped, 3.5, color)
			var ring := float(contact.get("ring", 0.0)) * world_scale
			if ring > 1.0 and not at_edge:
				draw_arc(center + clamped, ring, 0, TAU, 32,
					Color(color.r, color.g, color.b, 0.28 + 0.1 * sin(_t * 3.0)), 1.0)


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
	_beacon_mats.append(beacon_material)
	# Running lights around the ring, and a couple of ships at berth —
	# a port reads as a port when somebody else is already home.
	for i in 5:
		var light_mat := StandardMaterial3D.new()
		light_mat.emission_enabled = true
		light_mat.emission = Color(1.0, 0.72, 0.3) if i % 2 == 0 else Color(0.4, 0.7, 1.0)
		light_mat.emission_energy_multiplier = 3.0
		light_mat.albedo_color = Color(0.2, 0.15, 0.05)
		var light := MeshInstance3D.new()
		var light_mesh := SphereMesh.new()
		light_mesh.radius = 0.5
		light_mesh.height = 1.0
		light.mesh = light_mesh
		var angle := TAU * i / 5.0
		light.position = Vector3(cos(angle) * 14.0, sin(angle * 2.0) * 2.0, sin(angle) * 14.0)
		light.material_override = light_mat
		station.add_child(light)
		_beacon_mats.append(light_mat)
	for i in 2:
		var berthed := _build_ship(Color(0.35, 0.38, 0.3) if i == 0 else Color(0.3, 0.32, 0.42))
		berthed.position = Vector3(-10.0 + i * 22.0, -4.0, 18.0)
		berthed.rotation.y = 0.6 + i * 2.1
		berthed.scale = Vector3.ONE * 0.8
		station.add_child(berthed)
	return station


## A landable world fills the sky behind its beacon: the approach IS the
## place growing in the viewport.
func _build_planet_backdrop() -> void:
	if _location.get("kind", "") != "planet":
		return
	var biome: Dictionary = _location.get("biome", {})
	var planet := MeshInstance3D.new()
	var mesh := SphereMesh.new()
	mesh.radius = 240.0
	mesh.height = 480.0
	mesh.radial_segments = 48
	mesh.rings = 24
	planet.mesh = mesh
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color.from_string(str(biome.get("ground", "")), Color(0.24, 0.34, 0.28))
	mat.roughness = 1.0
	mat.emission_enabled = true
	mat.emission = Color.from_string(str(biome.get("sky", "")), Color(0.14, 0.18, 0.28))
	mat.emission_energy_multiplier = 0.25
	planet.material_override = mat
	var anchor := _station.global_position if _station != null else Vector3(30, 6, -150)
	planet.position = anchor + Vector3(40, -60, -420)
	add_child(planet)
	# Atmosphere rim: a slightly larger translucent shell.
	var atmo := MeshInstance3D.new()
	var atmo_mesh := SphereMesh.new()
	atmo_mesh.radius = 252.0
	atmo_mesh.height = 504.0
	atmo.mesh = atmo_mesh
	var atmo_mat := StandardMaterial3D.new()
	atmo_mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	atmo_mat.albedo_color = Color(0.5, 0.7, 1.0, 0.10)
	atmo_mat.emission_enabled = true
	atmo_mat.emission = Color(0.5, 0.7, 1.0)
	atmo_mat.emission_energy_multiplier = 0.3
	atmo.material_override = atmo_mat
	atmo.position = planet.position
	add_child(atmo)


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
	_clear_spilled()
	GameManager.request_mode(GameManager.Mode.LANDED)


func on_board_ship() -> void:
	GameManager.request_mode(GameManager.Mode.ON_BOARD)
