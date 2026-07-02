extends Node

var _active_scene: Node = null

func _ready() -> void:
	GameManager.mode_change_requested.connect(_on_mode_change_requested)
	_load_mode(_initial_mode())


## Content decides where a new game starts (manifest `start.mode`); the
## engine only supplies the fallback.
func _initial_mode() -> int:
	match DataRegistry.start_config().get("mode", ""):
		"on_board": return GameManager.Mode.ON_BOARD
		"landed":   return GameManager.Mode.LANDED
		_:          return GameManager.Mode.SPACE_FLIGHT

func _on_mode_change_requested(mode: int) -> void:
	_load_mode(mode)

func _load_mode(mode: int) -> void:
	if _active_scene:
		remove_child(_active_scene)
		_active_scene.queue_free()
		_active_scene = null
	var packed: PackedScene = load(_scene_path(mode))
	_active_scene = packed.instantiate()
	add_child(_active_scene)

func _scene_path(mode: int) -> String:
	match mode:
		GameManager.Mode.SPACE_FLIGHT: return "res://scenes/space/space_flight.tscn"
		GameManager.Mode.ON_BOARD:     return "res://scenes/on_board/on_board.tscn"
		GameManager.Mode.LANDED:       return "res://scenes/landed/landed.tscn"
	push_error("Unknown mode: %d" % mode)
	return ""
