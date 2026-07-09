extends Node
## Ring 0 — the runtime-state counterpart of the authored contracts, and the
## implementation of the save schema (godot/framework/schemas/save.schema.json).
##
## Everything that changes during play lives here: player, ship, soul runtime
## state, faction standings, the universe tick. Authored data (DataRegistry)
## is read-only birth-state; this node owns the drift. It is also the single
## source of the trigger-DSL context — conditions evaluate against `context()`.
##
## Save-slot ring (v1, t_f7f06ee):
## Saves rotate through RING_SIZE numbered slots so no single save overwrites
## another. An index file tracks which slot to write next, and each save
## records its originating location for player reference. The single-slot
## save from v0 (slot0.json) is auto-migrated on first load.

signal state_changed
signal soul_memory_pending(soul_id: String, memory: Dictionary)
signal universe_loaded

const SAVE_VERSION := 1
const RING_SIZE := 5
const SAVE_DIR := "user://saves/"
const INDEX_PATH := "user://saves/index.json"

var universe := {"tick": 0, "flags": []}
var player := {
	"location": "",       # location id when landed/docked, "" in space
	"current_space": "",  # location id whose space we fly in; survives undock
	"credits": 200,
	"flags": [],
	"upgrades": [],       # upgrade ids owned (upgrade contract)
	"ship": {
		"hull_id": "",
		"hull_integrity": 1.0,
		"position": [0.0, 0.0, 0.0],
		"cargo": {},        # good id -> qty
	},
}
var souls := {}            # npc id -> {relationships:{to:{axis:val}}, emotions:{}, flags:[], pending_memories:[]}
var factions := {}         # faction id -> {standing:{axis:val}}
var crew := {"seeded": false, "aboard": [], "assignments": {}, "edges": {}, "history": []}
# Active mission progress (save schema `mission` block). MissionManager owns
# the semantics; this var is the persistence slot. Rides every save, restored
# on load via universe_loaded signal.
var mission := {}

var _next_slot := 0


func _ready() -> void:
	DataRegistry.mods_loaded.connect(_on_mods_loaded)


func _on_mods_loaded(_order: Array) -> void:
	if player.ship.hull_id == "":
		player.ship.hull_id = DataRegistry.start_config().get("player_ship", "")
	if player.current_space == "":
		player.current_space = DataRegistry.start_config().get("location", "")


func is_docked() -> bool:
	return player.location != ""


## Return the location id whose space we're flying in. Survives undocking;
## falls back to the mods' start location on a fresh game.
func current_space() -> String:
	if player.get("current_space", "") != "":
		return player.current_space
	return DataRegistry.start_config().get("location", "")


func set_flag(flag: String) -> void:
	if flag not in player.flags:
		player.flags.append(flag)
		state_changed.emit()


func clear_flag(flag: String) -> void:
	player.flags.erase(flag)
	state_changed.emit()


func has_flag(flag: String) -> bool:
	return flag in player.flags


func add_cargo(good_id: String, qty: int) -> void:
	var cargo: Dictionary = player.ship.cargo
	cargo[good_id] = int(cargo.get(good_id, 0)) + qty
	if cargo[good_id] <= 0:
		cargo.erase(good_id)
	state_changed.emit()


func cargo_count(good_id: String) -> int:
	return int(player.ship.cargo.get(good_id, 0))


func adjust_credits(amount: int) -> void:
	player.credits = maxi(0, int(player.credits) + amount)
	state_changed.emit()


## --- upgrades (upgrade contract) ------------------------------------------------


## Record an owned upgrade. Also sets the flag upgrade_<id> so trigger-DSL
## conditions and dialogue choices can see it without a new namespace.
func add_upgrade(upgrade_id: String) -> void:
	if upgrade_id in player.upgrades:
		return
	player.upgrades.append(upgrade_id)
	set_flag("upgrade_" + upgrade_id)


func has_upgrade(upgrade_id: String) -> bool:
	return upgrade_id in player.get("upgrades", [])


## Sum a named numeric effect across every owned upgrade (additive effects:
## hull_bonus, damage_bonus, timer_bonus_seconds, ...).
func upgrade_effect_sum(key: String) -> float:
	var total := 0.0
	for upgrade_id: String in player.get("upgrades", []):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		total += float(effects.get(key, 0.0))
	return total


## Multiply a named numeric effect across every owned upgrade (multiplicative
## effects: detection_mult, speed_mult, ...). 1.0 when none owned.
func upgrade_effect_product(key: String) -> float:
	var product := 1.0
	for upgrade_id: String in player.get("upgrades", []):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		product *= float(effects.get(key, 1.0))
	return product


func faction_standing(faction_id: String) -> Dictionary:
	if not factions.has(faction_id):
		return {"trust": 0, "contribution": 0, "notoriety": 0}
	return factions[faction_id].get("standing", {})


func adjust_faction_standing(faction_id: String, axis: String, amount: int) -> void:
	if not factions.has(faction_id):
		factions[faction_id] = {"standing": {"trust": 0, "contribution": 0, "notoriety": 0}}
	var standing: Dictionary = factions[faction_id].get("standing", {})
	standing[axis] = clampi(int(standing.get(axis, 0)) + amount, -100, 100)
	factions[faction_id]["standing"] = standing
	state_changed.emit()


func price_modifier_for(faction_id: String) -> float:
	var s: Dictionary = faction_standing(faction_id)
	var trust: int = int(s.get("trust", 0))
	var contrib: int = int(s.get("contribution", 0))
	var notoriety: int = int(s.get("notoriety", 0))
	var trust_mod := -float(trust) / 400.0
	var contrib_mod := -float(contrib) / 600.0
	var notoriety_mod := float(notoriety) / 500.0
	return clampf(trust_mod + contrib_mod + notoriety_mod, -0.25, 0.25)


func is_good_unlocked(good_id: String, faction_id: String) -> bool:
	var s: Dictionary = faction_standing(faction_id)
	return int(s.get("trust", 0)) >= 25 and int(s.get("notoriety", 0)) <= 50


func soul_state(soul_id: String) -> Dictionary:
	if not souls.has(soul_id):
		var birth: Dictionary = DataRegistry.get_entity("npcs", soul_id)
		var relationships := {}
		for edge: Dictionary in birth.get("relationships", []):
			relationships[edge.get("to", "")] = {"trust": edge.get("strength", 0)}
		var emotions := {}
		for axis: String in birth.get("emotional_baseline", {}):
			emotions[axis] = birth.emotional_baseline[axis]
		souls[soul_id] = {
			"relationships": relationships,
			"emotions": emotions,
			"flags": [],
			"pending_memories": [],
		}
	return souls[soul_id]


func apply_soul_mutation(soul_id: String, mutation: Dictionary) -> void:
	var state := soul_state(soul_id)
	match mutation.get("op", ""):
		"adjust_relationship":
			var target: String = mutation.get("target", "player")
			var axis: String = mutation.get("axis", "trust")
			var rel: Dictionary = state.relationships.get(target, {})
			rel[axis] = clampi(int(rel.get(axis, 0)) + int(mutation.get("amount", 0)), -100, 100)
			state.relationships[target] = rel
		"add_memory":
			var memory := {
				"text": mutation.get("text", ""),
				"importance": mutation.get("importance", 0.5),
				"tags": mutation.get("tags", []),
				"tick": universe.tick,
			}
			state.pending_memories.append(memory)
			soul_memory_pending.emit(soul_id, memory)
		"set_flag":
			if mutation.get("flag", "") not in state.flags:
				state.flags.append(mutation.get("flag", ""))
		"clear_flag":
			state.flags.erase(mutation.get("flag", ""))
		_:
			push_warning("game_state: unknown mutation op %s" % mutation.get("op", "?"))
	state_changed.emit()


func context() -> Dictionary:
	var soul_ns := {}
	for soul_id: String in souls:
		var s: Dictionary = souls[soul_id]
		var entry: Dictionary = {"flags": s.flags}
		var toward_player: Dictionary = s.relationships.get("player", {})
		for axis: String in toward_player:
			entry[axis] = toward_player[axis]
		for axis: String in s.emotions:
			entry[axis] = s.emotions[axis]
		soul_ns[soul_id] = entry
	var faction_ns := {}
	for faction_id: String in factions:
		faction_ns[faction_id] = factions[faction_id].get("standing", {})
	return {
		"player": {
			"location": player.location,
			"credits": player.credits,
			"flags": player.flags,
			"docked": is_docked(),
		},
		"soul": soul_ns,
		"faction": faction_ns,
		"universe": {"tick": universe.tick},
	}


## save/load — save-slot ring v1


func save_game() -> bool:
	var slots: Array = _read_or_init_index()
	var slot_index: int = _next_slot
	var slot_path: String = _slot_path(slot_index)

	var active_mission := ""
	if MissionManager.is_active():
		active_mission = MissionManager.active_mission_id()

	var snapshot := {
		"save_version": SAVE_VERSION,
		"created_at": Time.get_datetime_string_from_system(true),
		"updated_at": Time.get_datetime_string_from_system(true),
		"universe": universe,
		"player": player,
		"factions": factions,
		"crew": crew,
		"mission": mission,
		"souls": _souls_for_save(),
		"mods": {
			"load_order": DataRegistry.load_order(),
			"framework_version": 0,
		},
	}

	DirAccess.make_dir_recursive_absolute(SAVE_DIR)
	var file := FileAccess.open(slot_path, FileAccess.WRITE)
	if file == null:
		push_error("game_state: cannot open %s for writing" % slot_path)
		return false
	file.store_string(JSON.stringify(snapshot, "  "))
	file.close()

	var location: String = snapshot.player.get("location", "")
	var slot_entry: Dictionary = slots[slot_index] as Dictionary
	slots[slot_index] = {
		"filled": true,
		"tick": universe.tick,
		"created_at": slot_entry.get("created_at", snapshot.created_at),
		"updated_at": snapshot.updated_at,
		"location": location,
		"mission_id": active_mission,
	}
	_next_slot = (slot_index + 1) % RING_SIZE
	_write_index(slots)

	print("game_state: saved to %s (tick %d, slot %d)" % [slot_path, universe.tick, slot_index])
	return true


func load_game() -> bool:
	var slots: Array = _read_or_init_index()
	var best_idx := -1
	var best_tick := -1
	for i in slots.size():
		var entry: Dictionary = slots[i] as Dictionary
		if entry.get("filled", false):
			var tick: int = int(entry.get("tick", 0))
			if tick > best_tick:
				best_tick = tick
				best_idx = i
	if best_idx == -1:
		return false
	return _load_slot_file(best_idx)


func load_slot(slot_index: int) -> bool:
	if slot_index < 0 or slot_index >= RING_SIZE:
		return false
	var slots: Array = _read_or_init_index()
	if slot_index >= slots.size():
		return false
	var entry: Dictionary = slots[slot_index] as Dictionary
	if not entry.get("filled", false):
		return false
	return _load_slot_file(slot_index)


func list_saves() -> Array:
	var slots: Array = _read_or_init_index()
	var result: Array = []
	for i in slots.size():
		var entry: Dictionary = slots[i] as Dictionary
		result.append({
			"index": i,
			"filled": entry.get("filled", false),
			"tick": entry.get("tick", 0),
			"created_at": entry.get("created_at", ""),
			"updated_at": entry.get("updated_at", ""),
			"location": entry.get("location", ""),
			"mission_id": entry.get("mission_id", ""),
		})
	return result


func has_save() -> bool:
	if FileAccess.file_exists(INDEX_PATH):
		var raw: Variant = FileAccess.get_file_as_string(INDEX_PATH)
		if raw is String and raw.length() > 0:
			var parsed: Variant = JSON.parse_string(raw)
			if parsed is Dictionary and parsed.has("slots"):
				for entry in (parsed.slots as Array):
					if (entry as Dictionary).get("filled", false):
						return true
	if FileAccess.file_exists("user://saves/slot0.json"):
		var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string("user://saves/slot0.json"))
		if parsed is Dictionary:
			return true
	return false


## internal


func _slot_path(index: int) -> String:
	return SAVE_DIR + "slot%d.json" % index


func _load_slot_file(slot_index: int) -> bool:
	var path: String = _slot_path(slot_index)
	if not FileAccess.file_exists(path):
		return false
	var raw: Variant = FileAccess.get_file_as_string(path)
	var parsed: Variant = JSON.parse_string(raw)
	if not parsed is Dictionary:
		push_error("game_state: save slot %d is corrupt" % slot_index)
		return false
	return _apply_snapshot(parsed as Dictionary)


func _apply_snapshot(snapshot: Dictionary) -> bool:
	var version: int = int(snapshot.get("save_version", -1))
	if version > SAVE_VERSION:
		push_error("game_state: save version %d > engine version %d" % [version, SAVE_VERSION])
		return false
	if version < 0:
		return false
	universe = snapshot.get("universe", universe)
	player = snapshot.get("player", player)
	if player.ship.get("hull_id", "") == "":
		player.ship.hull_id = DataRegistry.start_config().get("player_ship", "")
	# Fields added after v0 saves existed: default them in place.
	if not player.has("current_space") or player.current_space == "":
		player["current_space"] = DataRegistry.start_config().get("location", "")
	if not player.has("upgrades"):
		player["upgrades"] = []
	factions = snapshot.get("factions", {})
	var saved_crew: Dictionary = snapshot.get("crew", {})
	if not saved_crew.is_empty():
		crew = saved_crew
	mission = snapshot.get("mission", {})
	souls = {}
	var saved_souls: Dictionary = snapshot.get("souls", {})
	for soul_id: String in saved_souls:
		var s: Dictionary = saved_souls[soul_id]
		souls[soul_id] = {
			"relationships": s.get("relationships", {}),
			"emotions": s.get("emotions", {}),
			"flags": s.get("flags", []),
			"pending_memories": s.get("pending_memories", []),
		}
	print("game_state: loaded (tick %d)" % universe.tick)
	universe_loaded.emit()
	state_changed.emit()
	return true


func _souls_for_save() -> Dictionary:
	var out := {}
	for soul_id: String in souls:
		var s: Dictionary = souls[soul_id]
		out[soul_id] = {
			"relationships": s.relationships,
			"emotions": s.emotions,
			"flags": s.flags,
			"pending_memories": s.pending_memories,
		}
	return out


## ring index management


func _read_or_init_index() -> Array:
	if FileAccess.file_exists(INDEX_PATH):
		var raw: Variant = FileAccess.get_file_as_string(INDEX_PATH)
		if raw is String and raw.length() > 0:
			var parsed: Variant = JSON.parse_string(raw)
			if parsed is Dictionary and parsed.has("slots"):
				_next_slot = int(parsed.get("next_slot", 0))
				return parsed.slots as Array

	# No index file (or corrupt) — try migrating a legacy v0 single-slot save.
	var legacy_path := "user://saves/slot0.json"
	if FileAccess.file_exists(legacy_path):
		var raw: Variant = FileAccess.get_file_as_string(legacy_path)
		var parsed: Variant = JSON.parse_string(raw)
		if parsed is Dictionary:
			_migrate_legacy_save(parsed as Dictionary)
			var fresh: Variant = JSON.parse_string(FileAccess.get_file_as_string(INDEX_PATH))
			if fresh is Dictionary and fresh.has("slots"):
				_next_slot = int(fresh.get("next_slot", 0))
				return fresh.slots as Array
	return _init_empty_ring()


func _init_empty_ring() -> Array:
	var slots: Array = []
	for i in RING_SIZE:
		slots.append({"filled": false, "tick": 0})
	_next_slot = 0
	_write_index(slots)
	return slots


func _migrate_legacy_save(legacy: Dictionary) -> void:
	print("game_state: migrating legacy save (slot0.json) to slot ring")
	legacy["save_version"] = 0
	var slot0_path := _slot_path(0)
	DirAccess.make_dir_recursive_absolute(SAVE_DIR)
	var file := FileAccess.open(slot0_path, FileAccess.WRITE)
	if file != null:
		file.store_string(JSON.stringify(legacy, "  "))
		file.close()
	var location := ""
	if legacy.has("player") and legacy.player is Dictionary:
		location = legacy.player.get("location", "")
	var slots: Array = []
	slots.append({
		"filled": true,
		"tick": legacy.get("universe", {}).get("tick", 0),
		"created_at": legacy.get("created_at", ""),
		"updated_at": legacy.get("updated_at", ""),
		"location": location,
		"mission_id": "",
	})
	for i in range(1, RING_SIZE):
		slots.append({"filled": false, "tick": 0})
	_write_index(slots, 1)


func _write_index(slots: Array, next_slot_override := -1) -> void:
	var ns: int = _next_slot if next_slot_override < 0 else next_slot_override
	var data := {
		"ring_size": RING_SIZE,
		"next_slot": ns,
		"slots": slots,
	}
	DirAccess.make_dir_recursive_absolute(SAVE_DIR)
	var file := FileAccess.open(INDEX_PATH, FileAccess.WRITE)
	if file == null:
		push_error("game_state: cannot write index file")
		return
	file.store_string(JSON.stringify(data, "  "))
	file.close()
