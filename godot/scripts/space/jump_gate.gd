extends Node3D
## Ring 0 — Jump Gate transit system (Sprint 03, P15).

class_name JumpGate
##
## Jump gates connect systems. Flying through a gate triggers transit
## to the connected system. Emergency jump drives bypass gates with risk.
##
## Data-driven: gate network lives in JSON. The engine never names a
## system id.

signal gate_entered(gate_id: String, from_system: String, to_system: String)
signal transit_completed(from_system: String, to_system: String)
signal transit_failed(reason: String)

const GATE_APERTURE_RADIUS := 15.0
const GATE_ACTIVATION_RANGE := 25.0
const TRANSIT_DURATION := 3.0  # seconds of hyperspace tunnel

var _gate_id: String = ""
var _from_system: String = ""
var _to_system: String = ""
var _requires_clearance: bool = false
var _faction_control: String = ""
var _player: Node3D = null
var _transit_progress: float = -1.0  # -1 = not transiting
var _tunnel_nodes: Array = []

var _rng := RandomNumberGenerator.new()


func _ready() -> void:
	_rng.seed = randi() % 100000


## Initialize from gate data.
func configure(gate_data: Dictionary, player_node: Node3D) -> void:
	_gate_id = gate_data.get("id", "")
	_from_system = gate_data.get("system", "")
	_to_system = gate_data.get("connects_to", "")
	_requires_clearance = gate_data.get("requires_clearance", false)
	_faction_control = gate_data.get("faction_control", "")
	_player = player_node
	
	_build_gate_mesh()


func _build_gate_mesh() -> void:
	# Simple ring mesh for the gate aperture
	var ring := MeshInstance3D.new()
	var torus := TorusMesh.new()
	torus.inner_radius = GATE_APERTURE_RADIUS - 1
	torus.outer_radius = GATE_APERTURE_RADIUS + 1
	ring.mesh = torus
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(0.2, 0.5, 0.9)
	mat.emission_enabled = true
	mat.emission = Color(0.3, 0.7, 1.0)
	mat.emission_energy_multiplier = 1.5
	ring.material_override = mat
	add_child(ring)
	
	# Inner glow plane
	var glow := MeshInstance3D.new()
	var plane := QuadMesh.new()
	plane.size = Vector2(GATE_APERTURE_RADIUS * 2, GATE_APERTURE_RADIUS * 2)
	glow.mesh = plane
	var glow_mat := StandardMaterial3D.new()
	glow_mat.albedo_color = Color(0.1, 0.3, 0.8, 0.3)
	glow_mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	glow_mat.emission_enabled = true
	glow_mat.emission = Color(0.2, 0.5, 1.0)
	glow_mat.emission_energy_multiplier = 0.8
	glow.material_override = glow_mat
	add_child(glow)


func _physics_process(delta: float) -> void:
	if _transit_progress >= 0.0:
		_update_transit(delta)
		return
	
	if _player == null:
		return
	
	var dist := _player.global_position.distance_to(global_position)
	
	# Show activation prompt when close
	if dist < GATE_ACTIVATION_RANGE:
		if Input.is_action_just_pressed("interact"):
			_start_transit()


func _start_transit() -> void:
	if _requires_clearance and not _has_clearance():
		transit_failed.emit("Clearance required — Compact controls this gate")
		return
	
	gate_entered.emit(_gate_id, _from_system, _to_system)
	_transit_progress = 0.0
	_show_tunnel_effect()


func _update_transit(delta: float) -> void:
	_transit_progress += delta / TRANSIT_DURATION
	
	if _transit_progress >= 1.0:
		_complete_transit()


func _complete_transit() -> void:
	_transit_progress = -1.0
	_hide_tunnel_effect()
	transit_completed.emit(_from_system, _to_system)


func _show_tunnel_effect() -> void:
	# Simple hyperspace effect: colored particles/stars streaming past
	_tunnel_nodes = []
	for i in 60:
		var star := MeshInstance3D.new()
		star.mesh = SphereMesh.new()
		star.mesh.radius = _rng.randf_range(0.1, 0.3)
		star.mesh.height = star.mesh.radius * 2
		var mat := StandardMaterial3D.new()
		mat.albedo_color = Color(_rng.randf(), _rng.randf(), _rng.randf(), 0.7)
		mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		mat.emission_enabled = true
		mat.emission = Color(1, 1, 1, 0.5)
		mat.emission_energy_multiplier = 2.0
		star.material_override = mat
		
		var theta := _rng.randf() * TAU
		var phi := _rng.randf() * PI
		star.position = Vector3(
			sin(theta) * cos(phi) * _rng.randf_range(5, 30),
			sin(theta) * sin(phi) * _rng.randf_range(5, 30),
			-_rng.randf_range(10, 200)
		)
		add_child(star)
		_tunnel_nodes.append(star)


func _hide_tunnel_effect() -> void:
	for node in _tunnel_nodes:
		if is_instance_valid(node):
			node.queue_free()
	_tunnel_nodes.clear()


func _has_clearance() -> bool:
	# Check player's standing with the controlling faction
	if _faction_control.is_empty():
		return true
	var standing := GameState.faction_standing(_faction_control)
	return int(standing.get("trust", 0)) >= 0


## Initiate an emergency jump (bypasses gate, risk of malfunction).
func emergency_jump(target_system: String) -> bool:
	if _transit_progress >= 0.0:
		return false
	
	# Risk roll
	var risk := _rng.randf()
	if risk < 0.2:
		# Malfunction
		transit_failed.emit("Jump drive malfunction — wrong coordinates")
		return false
	
	gate_entered.emit("emergency_jump", _from_system, target_system)
	_transit_progress = 0.0
	_show_tunnel_effect()
	return true
