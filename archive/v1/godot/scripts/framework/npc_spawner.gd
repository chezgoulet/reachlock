extends Node
## Ring 0 — the G2 host. Given a location, instantiate a SoulInstance for
## every NPC the location declares is present. Sets world_context from the
## location so Pan's perceive→decide loop has current facts.
##
## Generic: the spawner never names a content id. It reads `npcs_present`
## from the location dictionary (a content list) and looks up each soul by
## id through DataRegistry. The same spawner works for any mod.

class_name NpcSpawner

var _spawned: Array[SoulInstance] = []  # the live instances for this location


## Spawn every NPC the location declares. Returns the spawned instances.
## Pass the same dictionary the landed scene loaded (so `npcs_present` is
## already resolved). Idempotent: calling again with a different location
## releases the old set and spawns the new one.
func spawn_at_location(location: Dictionary) -> Array[SoulInstance]:
	release_all()
	var npc_ids: Array = location.get("npcs_present", [])
	var location_context := _world_context_for(location)
	for npc_id: String in npc_ids:
		var soul: Dictionary = DataRegistry.get_entity("npcs", npc_id)
		if soul.is_empty():
			push_warning("npc_spawner: no soul for npc id '%s' at location '%s'" % [
				npc_id, location.get("id", "?")])
			continue
		var inst := SoulInstance.new()
		inst.setup(soul)
		inst.world_context = location_context
		add_child(inst)
		_spawned.append(inst)
	return _spawned


## Spawn an explicit list of souls (the crew aboard a ship, per
## CrewRoster) with a shared world context. Same lifecycle rules as
## spawn_at_location; the two entry points differ only in where the id
## list comes from (location data vs the crew roster).
func spawn_souls(soul_ids: Array, world_context_text: String) -> Array[SoulInstance]:
	release_all()
	for npc_id: String in soul_ids:
		var soul: Dictionary = DataRegistry.get_entity("npcs", npc_id)
		if soul.is_empty():
			push_warning("npc_spawner: no soul for npc id '%s'" % npc_id)
			continue
		var inst := SoulInstance.new()
		inst.setup(soul)
		inst.world_context = world_context_text
		add_child(inst)
		_spawned.append(inst)
	return _spawned


## Send the same event to every spawned soul (e.g. `ship.docked`,
## `combat.engagement_started`). The decision arrives later via each
## soul's `spoke` / `acted` / `concluded` signals.
func broadcast_event(topic: String, payload: Dictionary, objective := "") -> void:
	for inst in _spawned:
		inst.perceive_event(topic, payload, objective)


## Convenience: look up a spawned soul by id (the soul file's `id` field).
## Returns null if the soul isn't currently spawned.
func get_spawned(soul_id: String) -> SoulInstance:
	for inst in _spawned:
		if inst.soul_id == soul_id:
			return inst
	return null


## Free every spawned soul.
func release_all() -> void:
	for inst in _spawned:
		inst.queue_free()
	_spawned.clear()


## Build the `world` channel text the soul's context will see. Generic;
## reads only the location's name, faction, and description.
func _world_context_for(location: Dictionary) -> String:
	var parts: PackedStringArray = []
	var loc_id: String = location.get("id", "unknown")
	var loc_name: String = location.get("name", loc_id)
	parts.append("Location: %s (%s)." % [loc_name, loc_id])
	var faction: String = location.get("faction_control", "")
	if faction != "":
		parts.append("Faction control: %s." % faction)
	var desc: String = location.get("description", "")
	if desc != "":
		parts.append(desc)
	return "\n".join(parts)
