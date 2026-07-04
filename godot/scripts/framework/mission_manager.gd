extends Node
## Ring 0 — MissionManager: tracks active missions, advances stages,
## dispatches rewards (Sprint 03, P12).
##
## Missions are data-driven JSON files in godot/mods/<mod>/missions/.
## Each mission has ordered stages with type, objective, trigger,
## completion criteria, failure conditions, and rewards.
##
## The engine never names a mission id — the mission_id comes from
## the data file.

signal mission_started(mission_id: String, name: String)
signal stage_advanced(mission_id: String, stage_id: String, objective: String)
signal mission_completed(mission_id: String)
signal mission_failed(mission_id: String, reason: String)

var _active: Dictionary = {}        # mission_id -> {stage_index, stages, rewards_final}
var _stage_timers: Dictionary = {}  # mission_id -> remaining seconds


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
	}
	
	mission_started.emit(mission_id, _active.name)
	_activate_stage(0)
	return true


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
	
	# Set timer if stage has a failure timer
	var failure: Dictionary = stage.get("failure", {})
	if failure.has("timer_seconds"):
		_stage_timers[_active.id] = failure.timer_seconds
	else:
		_stage_timers.erase(_active.id)
	
	stage_advanced.emit(_active.id, stage.get("id", ""), stage.get("objective", ""))


## Called every frame by GameManager or a mission watcher node.
func tick(delta: float) -> void:
	if _active.is_empty():
		return
	var stage: Dictionary = current_stage()
	
	# Tick stage timer
	if _stage_timers.has(_active.id):
		var remaining: float = _stage_timers[_active.id] - delta
		_stage_timers[_active.id] = remaining
		if remaining <= 0.0:
			_fail_mission("Time expired")
			return


## Report a game event that might advance a mission stage.
func report_event(event_name: String, context: Dictionary = {}) -> void:
	if _active.is_empty():
		return
	var stage: Dictionary = current_stage()
	var completion: Dictionary = stage.get("completion", {})
	var completion_type: String = completion.get("type", "")
	
	match completion_type:
		"arrive":
			if event_name == "docked" and context.get("location_id", "") == completion.get("target_id", ""):
				_apply_stage_rewards(stage)
				advance()
		"dialogue_end":
			if event_name == "dialogue_end" and context.get("npc_id", "") == completion.get("target_id", ""):
				_apply_stage_rewards(stage)
				advance()
		"dock":
			if event_name == "docked":
				_apply_stage_rewards(stage)
				advance()
		"undock":
			if event_name == "undocked":
				_apply_stage_rewards(stage)
				advance()
		"jump":
			if event_name == "jump_completed":
				_apply_stage_rewards(stage)
				advance()
		"manual":
			# Stage advances via explicit call, e.g. dialogue choice
			pass


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


func _apply_stage_rewards(stage: Dictionary) -> void:
	var rewards: Dictionary = stage.get("rewards", {})
	_apply_rewards_dict(rewards)


func _apply_rewards_dict(rewards: Dictionary) -> void:
	var credits: int = rewards.get("credits", 0)
	if credits > 0:
		GameState.adjust_credits(credits)
	
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
	_apply_rewards_dict(_active.get("rewards_final", {}))
	mission_completed.emit(mid)
	_active = {}
	_stage_timers.erase(mid)


func _fail_mission(reason: String) -> void:
	var mid: String = _active.get("id", "")
	mission_failed.emit(mid, reason)
	_active = {}
	_stage_timers.erase(mid)
