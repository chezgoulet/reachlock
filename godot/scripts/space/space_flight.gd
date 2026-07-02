extends Node3D
## Space Flight mode — arcade flight, cinematic feel over simulation
## (GAME-DESIGN.md §2: Star Fox 64, not Elite Dangerous).
##
## Every feel number comes from the player ship's `flight` block (framework
## ship schema) via DataRegistry, with engine defaults so the mode flies even
## with zero content loaded. The world here is a placeholder flight range —
## starfield and asteroids — so handling can be tuned against real parallax.

const DEFAULT_FLIGHT := {
	"top_speed": 46.0,        # units/s
	"acceleration": 30.0,     # units/s^2
	"turn_rate": 1.7,         # rad/s
	"boost_multiplier": 2.3,
	"drift": 0.35,            # 0 = velocity snaps to nose, 1 = pure drift
	"bank_angle_deg": 50.0,
}

const BASE_FOV := 75.0
const BOOST_FOV := 93.0
const TURN_RESPONSE := 8.0    # how quickly angular velocity follows input
const CAMERA_RESPONSE := 5.5  # chase-camera lag
const CAMERA_OFFSET := Vector3(0.0, 2.4, 10.0)

var _stats: Dictionary = DEFAULT_FLIGHT.duplicate()
var _velocity := Vector3.ZERO
var _angular_velocity := Vector3.ZERO  # rad/s around ship-local x (pitch), y (yaw), z (roll)
var _boosting := false

var _ship: Node3D
var _hull: Node3D  # visual child of _ship; carries the banking lean
var _starfield: MultiMeshInstance3D

@onready var _camera: Camera3D = $Camera3D
@onready var _speed_label: Label = $ModeOverlay/SpeedLabel
@onready var _ship_label: Label = $ModeOverlay/ShipLabel


func _ready() -> void:
	_load_flight_stats()
	_build_environment()
	_ship = _build_ship()
	add_child(_ship)
	_starfield = _build_starfield()
	add_child(_starfield)
	add_child(_build_asteroid_field())
	_camera.fov = BASE_FOV
	_camera.global_transform = _camera_rest_transform()


func _physics_process(delta: float) -> void:
	_apply_rotation(delta)
	_apply_thrust(delta)
	_ship.global_position += _velocity * delta
	_apply_banking(delta)
	_update_camera(delta)
	_starfield.global_position = _ship.global_position
	_update_hud()


## --- flight -------------------------------------------------------------


func _apply_rotation(delta: float) -> void:
	var turn_rate: float = _stats.turn_rate
	var target := Vector3(
		Input.get_axis("pitch_down", "pitch_up") * turn_rate,
		Input.get_axis("yaw_right", "yaw_left") * turn_rate,
		Input.get_axis("roll_right", "roll_left") * turn_rate * 1.6,
	)
	_angular_velocity = _angular_velocity.lerp(target, 1.0 - exp(-TURN_RESPONSE * delta))
	var b := _ship.basis
	b = b.rotated(b.x, _angular_velocity.x * delta)
	b = b.rotated(b.y, _angular_velocity.y * delta)
	b = b.rotated(b.z, _angular_velocity.z * delta)
	_ship.basis = b.orthonormalized()


func _apply_thrust(delta: float) -> void:
	_boosting = Input.is_action_pressed("boost")
	var boost_factor: float = _stats.boost_multiplier if _boosting else 1.0
	var speed: float = _stats.top_speed * boost_factor
	var accel: float = _stats.acceleration * boost_factor

	var throttle := Input.get_axis("thrust_back", "thrust_forward")
	var strafe := Input.get_axis("strafe_left", "strafe_right")
	var desired := (-_ship.basis.z * throttle + _ship.basis.x * strafe * 0.6) * speed

	if Input.is_action_pressed("brake"):
		desired = Vector3.ZERO
		accel *= 1.5
	elif desired.length() > 0.1 and _velocity.length() > 0.1:
		# Drift: a heavy hull keeps sliding along its old vector through a turn.
		var drifted := _velocity.normalized() * desired.length()
		desired = desired.lerp(drifted, _stats.drift)

	_velocity = _velocity.move_toward(desired, accel * delta)


func _apply_banking(delta: float) -> void:
	var yaw_input := Input.get_axis("yaw_right", "yaw_left")
	var target_bank := -yaw_input * deg_to_rad(_stats.bank_angle_deg)
	_hull.rotation.z = lerp_angle(_hull.rotation.z, target_bank, 1.0 - exp(-6.0 * delta))
	var pitch_input := Input.get_axis("pitch_down", "pitch_up")
	_hull.rotation.x = lerp_angle(_hull.rotation.x, pitch_input * 0.12, 1.0 - exp(-6.0 * delta))


func _update_camera(delta: float) -> void:
	var response := 1.0 - exp(-CAMERA_RESPONSE * delta)
	_camera.global_transform = _camera.global_transform.interpolate_with(
		_camera_rest_transform(), response
	)
	var target_fov := BOOST_FOV if _boosting else BASE_FOV
	_camera.fov = lerpf(_camera.fov, target_fov, 1.0 - exp(-4.0 * delta))


func _camera_rest_transform() -> Transform3D:
	return _ship.global_transform.translated_local(CAMERA_OFFSET)


func _update_hud() -> void:
	var status := "  [BOOST]" if _boosting else ""
	_speed_label.text = "%3.0f u/s%s" % [_velocity.length(), status]


## --- content ---------------------------------------------------------------


func _load_flight_stats() -> void:
	var ship_id: String = DataRegistry.start_config().get("player_ship", "")
	if ship_id == "":
		_ship_label.text = "test hull (no content loaded)"
		return
	var hull := DataRegistry.get_entity("ships", ship_id)
	_ship_label.text = str(hull.get("name", ship_id))
	var flight: Dictionary = hull.get("flight", {})
	for key: String in _stats:
		if flight.has(key):
			_stats[key] = flight[key]


## --- placeholder world ------------------------------------------------------
## Everything below is a generic flight range for tuning feel. Real content
## (stations, hulls, encounters) replaces it via mods; none of this knows any
## content id.


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


func _build_ship() -> Node3D:
	var root := Node3D.new()
	root.name = "PlayerShip"
	_hull = Node3D.new()
	_hull.name = "Hull"
	root.add_child(_hull)

	var body_material := StandardMaterial3D.new()
	body_material.albedo_color = Color(0.42, 0.44, 0.48)
	body_material.metallic = 0.55
	body_material.roughness = 0.45

	var body := MeshInstance3D.new()
	var prism := PrismMesh.new()
	prism.size = Vector3(1.7, 3.4, 0.7)
	body.mesh = prism
	body.rotation_degrees.x = -90.0  # prism apex points forward (-Z)
	body.material_override = body_material
	_hull.add_child(body)

	var wings := MeshInstance3D.new()
	var wing_mesh := BoxMesh.new()
	wing_mesh.size = Vector3(3.4, 0.14, 1.3)
	wings.mesh = wing_mesh
	wings.position = Vector3(0.0, -0.05, 0.7)
	wings.material_override = body_material
	_hull.add_child(wings)

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
		_hull.add_child(engine)
	return root


func _build_starfield() -> MultiMeshInstance3D:
	var rng := RandomNumberGenerator.new()
	rng.seed = 0x5747A5  # deterministic placeholder sky
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
		var star_position := direction * rng.randf_range(750.0, 950.0)
		var star_scale := Vector3.ONE * rng.randf_range(0.5, 1.6)
		multimesh.set_instance_transform(
			i, Transform3D(Basis.from_scale(star_scale), star_position)
		)
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
		var rock_position := direction * rng.randf_range(60.0, 520.0)
		var rock_basis := Basis.from_euler(Vector3(
			rng.randf_range(0.0, TAU), rng.randf_range(0.0, TAU), rng.randf_range(0.0, TAU)
		)).scaled(Vector3(
			rng.randf_range(2.0, 14.0), rng.randf_range(2.0, 10.0), rng.randf_range(2.0, 14.0)
		))
		multimesh.set_instance_transform(i, Transform3D(rock_basis, rock_position))
	var instance := MultiMeshInstance3D.new()
	instance.name = "AsteroidField"
	instance.multimesh = multimesh
	return instance


## --- mode transitions ------------------------------------------------------


func on_dock_initiated(_station_id: String) -> void:
	GameManager.request_mode(GameManager.Mode.LANDED)


func on_board_ship() -> void:
	GameManager.request_mode(GameManager.Mode.ON_BOARD)
