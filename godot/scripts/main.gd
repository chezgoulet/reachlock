extends Node
## The boot spine. Interactive runs open on the title screen (New Game /
## Continue / Join a Ship / Settings / Quit — nobody deletes JSON by hand
## again); headless and CI runs keep their exact pre-hatch behavior: a
## save resumes, REACHLOCK_FORCE_MODE/REACHLOCK_CHARACTER pick the seat,
## boot time untouched. Multiplayer never blocks single-player — nothing
## network exists on this path until a player asks for it.

var _active_scene: Node = null
var _pause: PauseMenu = null
var _title_layer: CanvasLayer = null


func _ready() -> void:
	GameManager.mode_change_requested.connect(_on_mode_change_requested)
	_pause = PauseMenu.new()
	_pause.quit_to_title.connect(_quit_to_title)
	add_child(_pause)
	if _is_headless_run():
		# The CI/testing contract, unchanged: a save always resumes; no
		# menus, no waiting, same boot time.
		if not GameState.load_game():
			var forced := OS.get_environment("REACHLOCK_CHARACTER")
			if forced != "":
				GameState.set_player_character(forced)
		_start_playing()
	else:
		_show_title()
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


func _is_headless_run() -> bool:
	return DisplayServer.get_name() == "headless" \
		or not OS.get_environment("REACHLOCK_FORCE_MODE").is_empty()


## --- the title screen --------------------------------------------------------


func _show_title() -> void:
	_unload_active_scene()
	_title_layer = CanvasLayer.new()
	_title_layer.layer = 90
	add_child(_title_layer)
	var title := TitleScreen.new()
	_title_layer.add_child(title)
	title.new_game.connect(_on_title_new_game)
	title.continue_game.connect(_on_title_continue)
	title.join_ship.connect(_join_ship)


func _on_title_new_game() -> void:
	GameState.reset_for_new_game()
	_close_title()
	_character_select(_start_playing)


func _on_title_continue() -> void:
	GameState.load_game()
	_close_title()
	_start_playing()


func _close_title() -> void:
	if _title_layer != null:
		_title_layer.queue_free()
		_title_layer = null


func _quit_to_title() -> void:
	GameState.save_game()
	ShipShare.stop()
	_show_title()


## --- joining a friend's boat (SHIP-SHARE.md) -----------------------------------


func _join_ship(address: String) -> void:
	if not ShipShare.join(address):
		return
	ShipShare.share_joined.connect(_on_share_joined, CONNECT_ONE_SHOT)
	ShipShare.share_refused.connect(_on_share_refused, CONNECT_ONE_SHOT)


func _on_share_joined() -> void:
	if ShipShare.share_refused.is_connected(_on_share_refused):
		ShipShare.share_refused.disconnect(_on_share_refused)
	_close_title()
	# Seat-claiming IS character select with company: pick a crew member,
	# the host arbitrates, the boat is the mode.
	_character_select(func() -> void:
		ShipShare.send_intent({"kind": "claim_seat",
			"body": {"npc_id": GameState.player_character()}})
		_load_mode(GameManager.Mode.ON_BOARD))


func _on_share_refused(reason: String, host_version: int, yours: int) -> void:
	if ShipShare.share_joined.is_connected(_on_share_joined):
		ShipShare.share_joined.disconnect(_on_share_joined)
	push_warning("share: %s (host runs v%d, you run v%d)" % [reason, host_version, yours])


## --- starting play ---------------------------------------------------------------


func _character_select(then: Callable) -> void:
	var layer := CanvasLayer.new()
	layer.layer = 90
	add_child(layer)
	var select := CharacterSelect.new()
	layer.add_child(select)
	select.finished.connect(func() -> void:
		layer.queue_free()
		then.call())


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


func _unload_active_scene() -> void:
	if _active_scene:
		remove_child(_active_scene)
		_active_scene.queue_free()
		_active_scene = null


func _load_mode(mode: int) -> void:
	_unload_active_scene()
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
