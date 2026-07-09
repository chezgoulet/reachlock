extends Node

signal mode_change_requested(mode: Mode)

enum Mode { SPACE_FLIGHT, ON_BOARD, LANDED }

var player_data := {}
var universe_tick: int = 0

func _ready() -> void:
	_register_actions()

func _process(_delta: float) -> void:
	if not OS.is_debug_build():
		return
	if Input.is_action_just_pressed("debug_mode_space"):
		request_mode(Mode.SPACE_FLIGHT)
	elif Input.is_action_just_pressed("debug_mode_board"):
		request_mode(Mode.ON_BOARD)
	elif Input.is_action_just_pressed("debug_mode_landed"):
		request_mode(Mode.LANDED)

func request_mode(mode: Mode) -> void:
	mode_change_requested.emit(mode)

func _register_actions() -> void:
	var bindings := {
		"thrust_forward":   KEY_W,
		"thrust_back":      KEY_S,
		"strafe_left":      KEY_A,
		"strafe_right":     KEY_D,
		"pitch_up":         KEY_UP,
		"pitch_down":       KEY_DOWN,
		"yaw_left":         KEY_LEFT,
		"yaw_right":        KEY_RIGHT,
		"roll_left":        KEY_Q,
		"roll_right":       KEY_E,
		"brake":            KEY_SPACE,
		"boost":            KEY_SHIFT,
		"fire":             KEY_CTRL,
		"mine":             KEY_F,
		"interact":         KEY_R,
		"jump":             KEY_J,
		"board":            KEY_B,
		"debug_mode_space": KEY_F1,
		"debug_mode_board": KEY_F2,
		"debug_mode_landed":KEY_F3,
	}
	for action: String in bindings:
		if InputMap.has_action(action):
			continue
		InputMap.add_action(action)
		var ev := InputEventKey.new()
		ev.keycode = bindings[action]
		InputMap.action_add_event(action, ev)
