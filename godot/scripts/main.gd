extends Node

var _active_scene: Node = null

func _ready() -> void:
	GameManager.mode_change_requested.connect(_on_mode_change_requested)
	# v0 continue-behavior: a save always resumes. "New game" = delete the
	# save; a proper title menu replaces this later.
	var resumed := GameState.load_game()
	if resumed:
		_start_playing()
	else:
		_new_game_opening()
	if OS.is_debug_build():
		# Test/CI hook (same family as REACHLOCK_FORCE_MODE): run headless
		# for N seconds, save, quit. Lets integration runs exercise the
		# full save path — including the universe snapshot — untouched.
		var autosave_after := OS.get_environment("REACHLOCK_AUTOSAVE_AFTER")
		if autosave_after.is_valid_float():
			get_tree().create_timer(autosave_after.to_float()).timeout.connect(
				func() -> void:
					GameState.save_game()
					get_tree().quit())


## A fresh playthrough opens on character select (any npc with a `playable`
## block); the scrawl rolls, then the game proper starts. Headless/CI runs
## skip the screen — REACHLOCK_CHARACTER=<npc id> still picks a seat.
func _new_game_opening() -> void:
	if DisplayServer.get_name() == "headless" or not OS.get_environment("REACHLOCK_FORCE_MODE").is_empty():
		var forced := OS.get_environment("REACHLOCK_CHARACTER")
		if forced != "":
			GameState.set_player_character(forced)
		_start_playing()
		return
	var layer := CanvasLayer.new()
	layer.layer = 90
	add_child(layer)
	var select := CharacterSelect.new()
	layer.add_child(select)
	select.finished.connect(func() -> void:
		layer.queue_free()
		_start_playing())


func _start_playing() -> void:
	# Fresh playthroughs open on the content's start.mission (a loaded save
	# restores its own mission via GameState.universe_loaded instead).
	MissionManager.autostart_if_idle()
	_load_mode(_initial_mode())


## A loaded save resumes where it was; otherwise content decides where a new
## game starts (manifest `start.mode`); the engine only supplies the fallback.
func _initial_mode() -> int:
	if OS.is_debug_build():
		# Test/CI hook: force a mode headlessly (integration tests, M1).
		match OS.get_environment("REACHLOCK_FORCE_MODE"):
			"landed":       return GameManager.Mode.LANDED
			"on_board":     return GameManager.Mode.ON_BOARD
			"space_flight": return GameManager.Mode.SPACE_FLIGHT
	if GameState.is_docked():
		return GameManager.Mode.LANDED
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
