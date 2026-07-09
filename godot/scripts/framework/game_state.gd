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
	"current_space": "",  # location id whose space we fly in; survives undock
	"credits": 200,
	"flags": [],
	"upgrades": [],       # upgrade ids owned (upgrade contract)
	"character": "",      # crew member the player embodies ("" = unnamed captain)
	"ship": {
		"hull_id": "",
		"hull_integrity": 1.0,
		"position": [0.0, 0.0, 0.0],
		"cargo": {},        # good id -> qty
		# Power routing set at the engineering station; flight reads it.
		"power": {"weapons": 0.33, "shields": 0.33, "engines": 0.34},
		# Interior damage from combat: manifests in the ship, degrades flight
		# until someone (player or crew) fixes it. Save schema ship.damage.
		"damage": [],
		"damage_seq": 0,
		"weapons_calibrated": false,
	},
}
# Active mission progress (save schema `mission` block). MissionManager owns
# the semantics and keeps this mirrored; this node owns persistence.
var mission := {}
var souls := {}            # npc id -> {relationships:{to:{axis:val}}, emotions:{}, flags:[], pending_memories:[]}
var factions := {}         # faction id -> {standing:{axis:val}}
# The crew block (P6): membership, station assignments, the relationship
# graph between crew members, and their shared history. CrewRoster owns
# the semantics; this node owns persistence. `seeded` marks first-touch
# derivation from authored content.
var crew := {"seeded": false, "aboard": [], "assignments": {}, "edges": {}, "history": []}


func _ready() -> void:
	DataRegistry.mods_loaded.connect(_on_mods_loaded)
	# DataRegistry is an earlier autoload: on a normal boot its mods_loaded
	# fired before this node existed. Apply content fallbacks now.
	if DataRegistry.entity_count() > 0:
		_on_mods_loaded(DataRegistry.load_order())


func _on_mods_loaded(_order: Array) -> void:
	if player.ship.hull_id == "":
		player.ship.hull_id = DataRegistry.start_config().get("player_ship", "")
	if player.current_space == "":
		player.current_space = DataRegistry.start_config().get("location", "")


func is_docked() -> bool:
	return player.location != ""


## The location id whose space the ship occupies (start location fallback,
## for saves and states from before the field existed).
func current_space() -> String:
	if player.get("current_space", "") != "":
		return player.current_space
	return DataRegistry.start_config().get("location", "")


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


## True when any owned upgrade carries a truthy boolean effect under `key`
## (mag_boots, auto_suppress, ...).
func upgrade_effect_bool(key: String) -> bool:
	for upgrade_id: String in player.get("upgrades", []):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		if bool(effects.get(key, false)):
			return true
	return false


## Multiply a named numeric effect across every owned upgrade (multiplicative
## effects: detection_mult, speed_mult, ...). 1.0 when none owned.
func upgrade_effect_product(key: String) -> float:
	var product := 1.0
	for upgrade_id: String in player.get("upgrades", []):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		product *= float(effects.get(key, 1.0))
	return product


## --- the player character (Sprint 3) --------------------------------------------


## The crew member the player embodies ("" = the unnamed-captain fallback).
func player_character() -> String:
	return str(player.get("character", ""))


func set_player_character(npc_id: String) -> void:
	player["character"] = npc_id
	state_changed.emit()


## A named character stat (playable contract: 1..5 authored, upgrades can push
## past). Base comes from the chosen character's playable.stats; the unnamed
## captain reads as an even 2 across the board. Upgrades add via the effect
## key "stat_<name>" (upgrade contract: flat numeric bag).
func player_stat(stat: String) -> int:
	var base := 2
	var character := player_character()
	if character != "":
		var playable: Dictionary = DataRegistry.get_entity("npcs", character).get("playable", {})
		base = int(playable.get("stats", {}).get(stat, 2))
	return clampi(base + int(upgrade_effect_sum("stat_" + stat)), 1, 7)


## --- interior ship damage (Sprint 3) ---------------------------------------------


## Record a new interior damage event (combat spawns these). Returns the entry.
func add_ship_damage(room: String, kind: String, pos: Array, severity := 1.0) -> Dictionary:
	var seq := int(player.ship.get("damage_seq", 0)) + 1
	player.ship["damage_seq"] = seq
	var entry := {"id": seq, "room": room, "kind": kind,
		"severity": clampf(severity, 0.0, 1.0), "pos": pos}
	ship_damage().append(entry)
	state_changed.emit()
	return entry


func ship_damage() -> Array:
	if not player.ship.has("damage"):
		player.ship["damage"] = []
	return player.ship.damage


func repair_ship_damage(damage_id: int) -> void:
	var damage := ship_damage()
	for i in damage.size():
		if int(damage[i].get("id", -1)) == damage_id:
			damage.remove_at(i)
			state_changed.emit()
			return


func damage_severity_total() -> float:
	var total := 0.0
	for entry: Dictionary in ship_damage():
		total += float(entry.get("severity", 1.0))
	return total


## What unrepaired damage costs in flight. Engineering trims the bleeding —
## a good engineer keeps a wounded ship flying closer to spec.
func flight_damage_penalty() -> Dictionary:
	var soften := clampf(1.0 - 0.06 * float(player_stat("engineering") - 2), 0.7, 1.15)
	var s := damage_severity_total() * soften
	return {
		"speed_mult": clampf(1.0 - 0.06 * s, 0.65, 1.0),
		"cooldown_mult": clampf(1.0 + 0.08 * s, 1.0, 1.6),
		"vulnerability": clampf(1.0 + 0.05 * s, 1.0, 1.5),
	}


## Gunnery calibration: set at the weapons station, spent by the next flight.
func set_weapons_calibrated(value: bool) -> void:
	player.ship["weapons_calibrated"] = value
	state_changed.emit()


func consume_weapons_calibration() -> bool:
	var calibrated := bool(player.ship.get("weapons_calibrated", false))
	if calibrated:
		player.ship["weapons_calibrated"] = false
	return calibrated


## --- faction standing ---------------------------------------------------------


## Get the player's current standing with a faction (trust, contribution, notoriety).
## Returns default {trust:0, contribution:0, notoriety:0} for unknown factions.
func faction_standing(faction_id: String) -> Dictionary:
	if not factions.has(faction_id):
		return {"trust": 0, "contribution": 0, "notoriety": 0}
	return factions[faction_id].get("standing", {})


## Adjust the player's standing with a faction along one axis. Values clamp
## to [-100, 100]. Emits state_changed so the ReputationPanel re-renders.
func adjust_faction_standing(faction_id: String, axis: String, amount: int) -> void:
	if not factions.has(faction_id):
		factions[faction_id] = {"standing": {"trust": 0, "contribution": 0, "notoriety": 0}}
	var standing: Dictionary = factions[faction_id].get("standing", {})
	standing[axis] = clampi(int(standing.get(axis, 0)) + amount, -100, 100)
	factions[faction_id]["standing"] = standing
	state_changed.emit()


## Price modifier for a given faction based on the player's standing.
## Returns a float multiplier in [-0.25, 0.25] — positive standing lowers
## prices (trusted operators get better rates). Applied per faction_control.
func price_modifier_for(faction_id: String) -> float:
	var s: Dictionary = faction_standing(faction_id)
	var trust: int = int(s.get("trust", 0))
	var contrib: int = int(s.get("contribution", 0))
	var notoriety: int = int(s.get("notoriety", 0))
	# Trust thresholds: -100 = +0.25 (hostile markup), +100 = -0.25 (discount)
	var trust_mod := -float(trust) / 400.0
	var contrib_mod := -float(contrib) / 600.0
	var notoriety_mod := float(notoriety) / 500.0
	return clampf(trust_mod + contrib_mod + notoriety_mod, -0.25, 0.25)


## Check if a good is restricted by standing. A good with an `unlocks` gate
## in any FactionAction is only available when the player's trust >= 25 AND
## notoriety <= 50 for the controlling faction. Default: unlocked.
func is_good_unlocked(good_id: String, faction_id: String) -> bool:
	var s: Dictionary = faction_standing(faction_id)
	return int(s.get("trust", 0)) >= 25 and int(s.get("notoriety", 0)) <= 50


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
		"set_player_flag":
			set_flag(mutation.get("flag", ""))
		"clear_player_flag":
			clear_flag(mutation.get("flag", ""))
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
			"upgrades": player.get("upgrades", []),
			"docked": is_docked(),
			"character": player_character(),
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
		"mission": mission,
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
	# Fields added after v0 saves existed: default them in place.
	if not player.has("current_space") or player.current_space == "":
		player["current_space"] = DataRegistry.start_config().get("location", "")
	if not player.has("upgrades"):
		player["upgrades"] = []
	if not player.ship.has("power"):
		player.ship["power"] = {"weapons": 0.33, "shields": 0.33, "engines": 0.34}
	if not player.has("character"):
		player["character"] = ""
	if not player.ship.has("damage"):
		player.ship["damage"] = []
	if not player.ship.has("damage_seq"):
		player.ship["damage_seq"] = 0
	if not player.ship.has("weapons_calibrated"):
		player.ship["weapons_calibrated"] = false
	mission = snapshot.get("mission", {})
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


## Power share for one channel (weapons/shields/engines), defaulting to an
## even split for pre-power saves.
func power_share(channel: String) -> float:
	var defaults := {"weapons": 0.33, "shields": 0.33, "engines": 0.34}
	return float(player.ship.get("power", defaults).get(channel, defaults.get(channel, 0.33)))


## Set one power channel and renormalize the others so the budget stays 1.0.
func set_power_share(channel: String, value: float) -> void:
	var power: Dictionary = player.ship.get("power",
		{"weapons": 0.33, "shields": 0.33, "engines": 0.34})
	value = clampf(value, 0.0, 0.9)
	var others: Array = []
	for key: String in power:
		if key != channel:
			others.append(key)
	var rest_total := 0.0
	for key: String in others:
		rest_total += float(power[key])
	var remaining := 1.0 - value
	for key: String in others:
		var share := float(power[key]) / rest_total if rest_total > 0.0 else 1.0 / others.size()
		power[key] = remaining * share
	power[channel] = value
	player.ship["power"] = power
	state_changed.emit()


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
