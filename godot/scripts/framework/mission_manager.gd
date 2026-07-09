extends Node
## Ring 0 — MissionManager: tracks active missions, advances stages,
## dispatches rewards (Sprint 03, P12).
##
## Missions are data-driven JSON files in godot/mods/<mod>/missions/.
## Each mission has ordered stages with type, objective, trigger,
## completion criteria, failure conditions, and rewards. A mission may
## declare `next` (the id started automatically when it completes — an
## authored campaign is a linked list of missions) and `epilogue` cards
## the MissionHud shows when it ends.
##
## The engine never names a mission id — the mission_id comes from
## the data file (or the manifest's start.mission for a fresh game).
##
## Persistence: progress mirrors into GameState.mission on every change,
## so it rides the ordinary save. On load (GameState.universe_loaded) the
## active mission and stage are restored from that block.

signal mission_started(mission_id: String, name: String)
signal stage_advanced(mission_id: String, stage_id: String, objective: String)
signal mission_completed(mission_id: String)
signal mission_failed(mission_id: String, reason: String)
signal epilogue_ready(mission_id: String, text: String, success: bool)

var _active: Dictionary = {}        # mission_id -> {stage_index, stages, rewards_final}
var _stage_timers: Dictionary = {}  # mission_id -> remaining seconds
var _event_counts: Dictionary = {}  # event name -> occurrences within current stage


func _ready() -> void:
	GameState.universe_loaded.connect(_on_universe_loaded)


## Start a mission by its data id. If a mission is already active,
## it is replaced (only one active mission at a time).
func start_mission(mission_id: String) -> bool:
	var data := DataRegistry.get_entity("missions", mission_id)
	if data.is_empty():
		push_warning("mission: '%s' not found in loaded mods" % mission_id)
		return false

	_active = {
		"id": mission_id,
		"name": data.get("name", mission_id),
		"stage_index": 0,
		"stages": data.get("stages", []),
		"rewards_final": data.get("rewards_final", {}),
		"failure_global": data.get("failure_global", {}),
		"next": data.get("next", ""),
		"epilogue": data.get("epilogue", {}),
	}
	_event_counts = {}

	mission_started.emit(mission_id, _active.name)
	_activate_stage(0)
	return true


## Start the manifest's start.mission if no mission is active or restored.
## Called by the boot sequence after the save (if any) has been loaded.
func autostart_if_idle() -> void:
	if not _active.is_empty():
		return
	if GameState.has_flag("campaign_over"):
		return
	var mission_id: String = DataRegistry.start_config().get("mission", "")
	if mission_id != "":
		start_mission(mission_id)


## Advance to the next stage. Called by completion triggers or manually.
func advance() -> void:
	if _active.is_empty():
		return
	var next_index: int = _active.stage_index + 1
	var stages: Array = _active.stages
	if next_index >= stages.size():
		_complete_mission()
		return
	_activate_stage(next_index)


func _activate_stage(index: int) -> void:
	var stages: Array = _active.stages
	if index < 0 or index >= stages.size():
		return
	var stage: Dictionary = stages[index]
	_active.stage_index = index
	_active["stage_start_time"] = Time.get_ticks_msec()
	_event_counts = {}

	# Set timer if stage has a failure timer. Owned upgrades can buy time
	# (upgrade contract: timer_bonus_seconds).
	var failure: Dictionary = stage.get("failure", {})
	if failure.has("timer_seconds"):
		var bonus: float = GameState.upgrade_effect_sum("timer_bonus_seconds")
		_stage_timers[_active.id] = float(failure.timer_seconds) + bonus
	else:
		_stage_timers.erase(_active.id)

	stage_advanced.emit(_active.id, stage.get("id", ""), stage.get("objective", ""))
	_persist()


## Called every frame by GameManager or a mission watcher node.
func tick(delta: float) -> void:
	if _active.is_empty():
		return

	# Tick stage timer
	if _stage_timers.has(_active.id):
		var remaining: float = _stage_timers[_active.id] - delta
		_stage_timers[_active.id] = remaining
		GameState.mission["timer_remaining"] = remaining
		if remaining <= 0.0:
			_fail_mission("time_expired")
			return


## Report a game event that might advance a mission stage.
func report_event(event_name: String, context: Dictionary = {}) -> void:
	if _active.is_empty():
		return

	# Global / stage failure conditions first.
	if event_name == "ship_destroyed" and _fails_on("on_ship_destroyed"):
		_fail_mission("ship_destroyed")
		return
	if event_name == "player_died" and _fails_on("on_death"):
		_fail_mission("player_died")
		return

	var stage: Dictionary = current_stage()
	var completion: Dictionary = stage.get("completion", {})
	var completion_type: String = completion.get("type", "")

	match completion_type:
		"arrive":
			if event_name == "docked" and context.get("location_id", "") == completion.get("target_id", ""):
				_finish_stage(stage)
		"dialogue_end":
			if event_name == "dialogue_end" and context.get("npc_id", "") == completion.get("target_id", ""):
				_finish_stage(stage)
		"dock":
			if event_name == "docked":
				_finish_stage(stage)
		"undock":
			if event_name == "undocked":
				_finish_stage(stage)
		"jump":
			if event_name == "jump_completed" or event_name == "self_jump_completed":
				_finish_stage(stage)
		"event":
			if event_name == completion.get("target_id", ""):
				var needed := int(completion.get("count", 1))
				_event_counts[event_name] = int(_event_counts.get(event_name, 0)) + 1
				if _event_counts[event_name] >= needed:
					_finish_stage(stage)
				else:
					_persist()
		"manual":
			# Stage advances via explicit call, e.g. dialogue choice
			pass


## How many more of the current stage's counted event are needed (HUD sugar).
## Returns -1 when the current stage is not an event-counting stage.
func event_remaining() -> int:
	var completion: Dictionary = current_stage().get("completion", {})
	if completion.get("type", "") != "event":
		return -1
	var needed := int(completion.get("count", 1))
	return needed - int(_event_counts.get(completion.get("target_id", ""), 0))


## Return the current active stage dict, or {} if no mission active.
func current_stage() -> Dictionary:
	if _active.is_empty():
		return {}
	var stages: Array = _active.stages
	var idx: int = _active.get("stage_index", 0)
	if idx < stages.size():
		return stages[idx]
	return {}


## Current objective text for the HUD.
func current_objective() -> String:
	var stage: Dictionary = current_stage()
	return stage.get("objective", "")


func mission_name() -> String:
	return _active.get("name", "")


## Current time remaining for timer stages, or -1.
func timer_remaining() -> float:
	if _active.is_empty() or not _stage_timers.has(_active.get("id", "")):
		return -1.0
	return _stage_timers[_active.id]


func is_active() -> bool:
	return not _active.is_empty()


## Immediately end the current mission as failed.
func fail(reason: String) -> void:
	_fail_mission(reason)


## --- internal ----------------------------------------------------------------


func _fails_on(key: String) -> bool:
	var stage_failure: Dictionary = current_stage().get("failure", {})
	if stage_failure.has(key):
		return bool(stage_failure[key])
	return bool(_active.get("failure_global", {}).get(key, false))


func _finish_stage(stage: Dictionary) -> void:
	_apply_stage_rewards(stage)
	advance()


func _apply_stage_rewards(stage: Dictionary) -> void:
	var rewards: Dictionary = stage.get("rewards", {})
	_apply_rewards_dict(rewards)


func _apply_rewards_dict(rewards: Dictionary) -> void:
	var credits: int = rewards.get("credits", 0)
	if credits > 0:
		GameState.adjust_credits(credits)

	var cargo: Dictionary = rewards.get("cargo", {})
	for good_id: String in cargo:
		GameState.add_cargo(good_id, int(cargo[good_id]))

	var deltas: Dictionary = rewards.get("faction_deltas", {})
	for faction_id: String in deltas:
		var axes: Dictionary = deltas[faction_id]
		for axis: String in axes:
			GameState.adjust_faction_standing(faction_id, axis, int(axes[axis]))

	var mutations: Array = rewards.get("soul_mutations", [])
	for mutation: Dictionary in mutations:
		GameState.apply_soul_mutation(mutation.get("npc", ""), mutation)


func _complete_mission() -> void:
	var mid: String = _active.get("id", "")
	var next_id: String = _active.get("next", "")
	var epilogue: Dictionary = _active.get("epilogue", {})
	_apply_rewards_dict(_active.get("rewards_final", {}))
	mission_completed.emit(mid)
	if epilogue.get("success", "") != "":
		epilogue_ready.emit(mid, epilogue.success, true)
	_active = {}
	_stage_timers.erase(mid)
	_persist()
	if next_id != "":
		start_mission(next_id)
	else:
		# The chain is over — a fresh boot should not restart the campaign.
		GameState.set_flag("campaign_over")


func _fail_mission(reason: String) -> void:
	var mid: String = _active.get("id", "")
	var epilogue: Dictionary = _active.get("epilogue", {})
	mission_failed.emit(mid, reason)
	var text: String = epilogue.get("failure_reasons", {}).get(reason, epilogue.get("failure", ""))
	if text != "":
		epilogue_ready.emit(mid, text, false)
	_active = {}
	_stage_timers.erase(mid)
	_persist()


## --- persistence ---------------------------------------------------------------


## Mirror progress into GameState.mission so any save captures it.
func _persist() -> void:
	if _active.is_empty():
		GameState.mission = {}
		return
	var block := {
		"id": _active.get("id", ""),
		"stage_index": int(_active.get("stage_index", 0)),
		"event_counts": _event_counts.duplicate(),
	}
	if _stage_timers.has(_active.id):
		block["timer_remaining"] = _stage_timers[_active.id]
	GameState.mission = block


## Restore the active mission from a loaded save.
func _on_universe_loaded() -> void:
	var block: Dictionary = GameState.mission
	if block.is_empty() or block.get("id", "") == "":
		_active = {}
		_stage_timers.clear()
		return
	var mission_id: String = block.id
	if not start_mission(mission_id):
		return
	var index := int(block.get("stage_index", 0))
	if index > 0:
		_activate_stage(index)
	_event_counts = block.get("event_counts", {}).duplicate()
	if block.has("timer_remaining"):
		_stage_timers[mission_id] = float(block.timer_remaining)
	_persist()
