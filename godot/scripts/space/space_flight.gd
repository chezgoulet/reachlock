extends Node3D

@export var thrust_speed: float = 20.0
@export var boost_multiplier: float = 3.0

var _velocity: Vector3 = Vector3.ZERO

func _ready() -> void:
	pass

func _process(delta: float) -> void:
	var input_dir := Vector3.ZERO
	input_dir.z -= Input.get_action_strength("thrust_forward")
	input_dir.z += Input.get_action_strength("thrust_back")
	input_dir.x += Input.get_action_strength("strafe_right")
	input_dir.x -= Input.get_action_strength("strafe_left")

	var speed := thrust_speed
	if Input.is_action_pressed("boost"):
		speed *= boost_multiplier

	_velocity = _velocity.lerp(input_dir * speed, delta * 4.0)

func on_dock_initiated(_station_id: String) -> void:
	GameManager.request_mode(GameManager.Mode.LANDED)

func on_board_ship() -> void:
	GameManager.request_mode(GameManager.Mode.ON_BOARD)
