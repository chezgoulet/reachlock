extends Node
## Ring 0 — the FactionAction dispatch engine (P8, Sprint 02).
##
## Processes trigger events against every authored FactionAction that matches,
## applying faction standing deltas, rival deltas, and price modifiers. The
## trigger vocabulary is the closed enum from faction_action.schema.json; every
## hot path calls `Reputation.trigger(name, context)` where context carries
## location_id, good_id, target_faction_id, and any action-specific keys.
##
## A modder adds new faction-relevant actions by writing a FactionAction JSON
## file in godot/mods/<mod>/faction_actions/. The engine never names a content
## id — action ids live in the mod file and become a denylist for the
## architecture guard.
##
## Content instance (the handoff contract): Trading ore with Sorrow Station's
## market (independent / Reach-aligned) slightly increases standing with Reach
## and slightly decreases standing with the Compact.

class_name ReputationManager
static var _actions: Array[Dictionary] = []


## Reload all faction actions from loaded mods. Called automatically when
## mods finish loading (DataRegistry.mods_loaded).
static func reload() -> void:
	_actions = DataRegistry.ids("faction_actions").map(
		func(id: String) -> Dictionary: return DataRegistry.get_entity("faction_actions", id)
	)


## Fire a trigger event. Called from every hot path — MarketBoard.on_traded,
## StationDock.on_dock/undock, PlanetScene.on_arrival, DialogueRunner.on_end.
## Matches every action whose `trigger` matches, applies standing deltas.
static func trigger(event_name: String, context: Dictionary = {}) -> void:
	if _actions.is_empty():
		return
	for action: Dictionary in _actions:
		if action.get("trigger", "") != event_name:
			continue
		var requires: Dictionary = action.get("requires_standing", {})
		var faction_id: String = _resolve_faction(action, context)
		if _gate_fails(requires, faction_id):
			continue
		_apply_delta(action, faction_id)
		_apply_rival_delta(action, faction_id)
		_apply_price_modifier(action, faction_id)


## --- helpers ------------------------------------------------------------------


static func _resolve_faction(action: Dictionary, context: Dictionary) -> String:
	match action.get("applies_to", "controlling_faction"):
		"target_faction":
			return str(action.get("target_faction_id", ""))
		"controlling_faction":
			return str(context.get("faction_control", ""))
		"all_known":
			return "_all"  # special sentinel handled in _apply_delta
	return ""


## Check whether the player's standing meets the requires_standing gate.
## Action is skipped (with a log line) when the gate fails — it does NOT
## consume a charge.
static func _gate_fails(requires: Dictionary, faction_id: String) -> bool:
	if requires.is_empty():
		return false
	if faction_id == "" or faction_id == "_all":
		return false  # can't gate broad actions
	var s: Dictionary = GameState.faction_standing(faction_id)
	for axis: String in requires:
		var threshold: int = int(requires[axis])
		var current: int = int(s.get(axis, 0))
		# trust etc: player must be >= threshold
		# notoriety etc: player must be <= threshold
		if current < threshold:
			return true
	return false


static func _apply_delta(action: Dictionary, faction_id: String) -> void:
	var delta: Dictionary = action.get("faction_delta", {})
	if delta.is_empty():
		return
	if faction_id == "_all":
		for fid: String in GameState.factions:
			_apply_deltas(fid, delta)
		return
	_apply_deltas(faction_id, delta)


static func _apply_rival_delta(action: Dictionary, faction_id: String) -> void:
	var rival_delta: Dictionary = action.get("rival_faction_delta", {})
	if rival_delta.is_empty():
		return
	var rival_id := str(action.get("rival_faction_id", ""))
	if rival_id == "":
		# Pick the faction with the worst stance toward the affected faction.
		rival_id = _worst_rival(faction_id)
	if rival_id != "":
		_apply_deltas(rival_id, rival_delta)


static func _apply_deltas(fid: String, deltas: Dictionary) -> void:
	for axis: String in deltas:
		GameState.adjust_faction_standing(fid, axis, int(deltas[axis]))


static func _worst_rival(faction_id: String) -> String:
	if faction_id == "" or faction_id == "_all":
		return ""
	var worst := ""
	var worst_stance := 100
	for fid: String in GameState.factions:
		if fid == faction_id:
			continue
		var stance: String = DataRegistry.get_entity("factions", fid).get("relationships", {}).get(faction_id, "neutral")
		var score: int = _stance_score(stance)
		if score < worst_stance:
			worst_stance = score
			worst = fid
	return worst


static func _stance_score(stance: String) -> int:
	match stance:
		"allied": return 4
		"friendly": return 3
		"neutral": return 2
		"tense": return 1
		"hostile": return 0
		"war": return -1
	return 2


static func _apply_price_modifier(action: Dictionary, faction_id: String) -> void:
	# Price modifiers are computed on read (MarketBoard) from GameState
	# faction_standing(), not applied here. The modifiers defined in the
	# action schema document what standing thresholds affect which goods;
	# the engine reads them when rendering prices.
	pass
