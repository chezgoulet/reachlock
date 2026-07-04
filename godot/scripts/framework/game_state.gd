extends Node
## Ring 0 — the runtime-state counterpart of the authored contracts, and the
## implementation of the save schema (godot/framework/schemas/save.schema.json).
##
## Everything that changes during play lives here: player, ship, soul runtime
## state, faction standings, the universe tick. Authored data (DataRegistry)
## is read-only birth-state; this node owns the drift. It is also the single
## source of the trigger-DSL context — conditions evaluate against `context()`.

signal state_changed
signal soul_memory_pending(soul_id: String, memory: Dictionary)
signal universe_loaded  # a save's universe block (incl. sim snapshot) was restored

const SAVE_VERSION := 0
const SAVE_PATH := "user://saves/slot0.json"

var universe := {"tick": 0, "flags": []}
var player := {
	"location": "",       # location id when landed/docked, "" in space
	"credits": 200,
	"flags": [],
	"ship": {
		"hull_id": "",
		"hull_integrity": 1.0,
		"position": [0.0, 0.0, 0.0],
		"cargo": {},        # good id -> qty
	},
}
var souls := {}            # npc id -> {relationships:{to:{axis:val}}, emotions:{}, flags:[], pending_memories:[]}
var factions := {}         # faction id -> {standing:{axis:val}}
# The crew block (P6): membership, station assignments, the relationship
# graph between crew members, and their shared history. CrewRoster owns
# the semantics; this node owns persistence. `seeded` marks first-touch
# derivation from authored content.
var crew := {"seeded": false, "aboard": [], "assignments": {}, "edges": {}, "history": []}


func _ready() -> void:
	DataRegistry.mods_loaded.connect(_on_mods_loaded)


func _on_mods_loaded(_order: Array) -> void:
	if player.ship.hull_id == "":
		player.ship.hull_id = DataRegistry.start_config().get("player_ship", "")


func is_docked() -> bool:
	return player.location != ""


## --- flags -------------------------------------------------------------------


func set_flag(flag: String) -> void:
	if flag not in player.flags:
		player.flags.append(flag)
		state_changed.emit()


func clear_flag(flag: String) -> void:
	player.flags.erase(flag)
	state_changed.emit()


func has_flag(flag: String) -> bool:
	return flag in player.flags


## --- cargo / credits ----------------------------------------------------------


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


## --- soul runtime state --------------------------------------------------------


## Lazily create runtime state for a soul from its authored birth-state.
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


## Apply one dialogue-graph mutation (dialogue schema $defs/mutation) or the
## equivalent from a soul's `npc.adjust_relationship` / `npc.remember` invoke.
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
			# The memory interface (Ragamuffin) drains pending memories when
			# connected (M3); until then they persist in the save.
			soul_memory_pending.emit(soul_id, memory)
		"set_flag":
			if mutation.get("flag", "") not in state.flags:
				state.flags.append(mutation.get("flag", ""))
		"clear_flag":
			state.flags.erase(mutation.get("flag", ""))
		_:
			push_warning("game_state: unknown mutation op %s" % mutation.get("op", "?"))
	state_changed.emit()


## --- trigger-DSL context --------------------------------------------------------


## The nested dictionary conditions evaluate against. Namespaces per the
## DSL contract: player.*, soul.*, faction.*, universe.*.
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


## --- save / load (C4) -----------------------------------------------------------


func save_game() -> bool:
	var snapshot := {
		"save_version": SAVE_VERSION,
		"created_at": Time.get_datetime_string_from_system(true),
		"updated_at": Time.get_datetime_string_from_system(true),
		"universe": universe,
		"player": player,
		"factions": factions,
		"crew": crew,
		"souls": _souls_for_save(),
		"mods": {
			"load_order": DataRegistry.load_order(),
			"framework_version": 0,
		},
	}
	DirAccess.make_dir_recursive_absolute(SAVE_PATH.get_base_dir())
	var file := FileAccess.open(SAVE_PATH, FileAccess.WRITE)
	if file == null:
		push_error("game_state: cannot open %s for writing" % SAVE_PATH)
		return false
	file.store_string(JSON.stringify(snapshot, "  "))
	file.close()
	print("game_state: saved to %s (tick %d)" % [SAVE_PATH, universe.tick])
	return true


func load_game() -> bool:
	if not FileAccess.file_exists(SAVE_PATH):
		return false
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(SAVE_PATH))
	if not parsed is Dictionary:
		push_error("game_state: save file is corrupt")
		return false
	var snapshot: Dictionary = parsed
	if int(snapshot.get("save_version", -1)) != SAVE_VERSION:
		push_error("game_state: save version %s unsupported" % str(snapshot.get("save_version")))
		return false
	universe = snapshot.get("universe", universe)
	player = snapshot.get("player", player)
	# Saves from before the hull field carry "": re-apply the content
	# fallback (mods are loaded before any save load in the boot order).
	if player.ship.get("hull_id", "") == "":
		player.ship.hull_id = DataRegistry.start_config().get("player_ship", "")
	factions = snapshot.get("factions", {})
	var saved_crew: Dictionary = snapshot.get("crew", {})
	if not saved_crew.is_empty():
		crew = saved_crew
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
	print("game_state: loaded %s (tick %d)" % [SAVE_PATH, universe.tick])
	universe_loaded.emit()
	state_changed.emit()
	return true


func has_save() -> bool:
	return FileAccess.file_exists(SAVE_PATH)


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
