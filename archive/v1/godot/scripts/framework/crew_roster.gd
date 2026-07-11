extends Node
## Ring 0 — the CrewRoster framework autoload (P6, Sprint 02).
##
## The data structure other systems query about the crew: who is aboard,
## where each member is stationed, and the relationship graph between them.
## It deliberately does NOT manage soul lifecycle (SoulGateway owns that),
## run dialogue (DialogueRunner), or render crew (ShipInterior). One
## structure, many readers.
##
## Data-driven: crew membership comes from npc files whose `ship` matches
## the player's hull; stations come from the npc's `station` field (falling
## back to the hull's first room). Relationship edges seed from the npc
## files' `relationships` arrays and then EVOLVE: shared experiences and
## dialogue mutations move them, and the whole graph persists in the save
## (GameState.crew), so the social fabric of the ship is part of the game
## state, not the authored data.

signal relationship_changed(a: String, b: String, axis: String, value: int)
signal shared_event_recorded(participants: Array, topic: String)

const HISTORY_MAX := 60


func _ready() -> void:
	DataRegistry.mods_loaded.connect(_on_mods_loaded)


func _on_mods_loaded(_order: Array) -> void:
	_ensure_seeded()


## --- membership ----------------------------------------------------------------


## Souls aboard the player's ship, sorted. Authored `ship` field decides;
## a future hire/leave system mutates GameState.crew.aboard directly.
func aboard() -> Array[String]:
	_ensure_seeded()
	var ids: Array[String] = []
	ids.assign(GameState.crew.aboard)
	ids.sort()
	return ids


func is_aboard(soul_id: String) -> bool:
	return soul_id in aboard()


## The room a crew member is stationed in ("" if not aboard).
func assignment(soul_id: String) -> String:
	_ensure_seeded()
	return GameState.crew.assignments.get(soul_id, "")


## Everyone stationed in the given room.
func assigned_to(room: String) -> Array[String]:
	_ensure_seeded()
	var ids: Array[String] = []
	for soul_id: String in GameState.crew.assignments:
		if GameState.crew.assignments[soul_id] == room:
			ids.append(soul_id)
	ids.sort()
	return ids


func assign(soul_id: String, room: String) -> void:
	_ensure_seeded()
	GameState.crew.assignments[soul_id] = room
	GameState.state_changed.emit()


## --- the relationship graph -----------------------------------------------------


## The live edge between two crew members: {trust, affinity}. Symmetric:
## relationship(a, b) == relationship(b, a). Missing edges read as zeros.
func relationship(a: String, b: String) -> Dictionary:
	_ensure_seeded()
	var edge: Dictionary = GameState.crew.edges.get(_edge_key(a, b), {})
	return {"trust": int(edge.get("trust", 0)), "affinity": int(edge.get("affinity", 0))}


## Move one axis of one edge (dialogue mutations, story beats, systems).
func adjust_relationship(a: String, b: String, axis: String, amount: int, note := "") -> void:
	_ensure_seeded()
	var key := _edge_key(a, b)
	var edge: Dictionary = GameState.crew.edges.get(key, {"trust": 0, "affinity": 0})
	edge[axis] = clampi(int(edge.get(axis, 0)) + amount, -100, 100)
	GameState.crew.edges[key] = edge
	_history({"tick": GameState.universe.tick, "kind": "adjust",
		"between": [a, b], "axis": axis, "amount": amount, "note": note})
	relationship_changed.emit(a, b, axis, edge[axis])
	GameState.state_changed.emit()


## A shared experience: everyone present lived the same moment together.
## Familiarity accretes (affinity +1 per pair); what it DID to trust is the
## caller's judgment (a survived firefight bonds, a botched job frays) via
## `trust_delta`. The event lands in the shared history either way.
func record_shared_event(participants: Array, topic: String, trust_delta := 0) -> void:
	_ensure_seeded()
	for i in participants.size():
		for j in range(i + 1, participants.size()):
			var a: String = participants[i]
			var b: String = participants[j]
			var key := _edge_key(a, b)
			var edge: Dictionary = GameState.crew.edges.get(key, {"trust": 0, "affinity": 0})
			edge["affinity"] = clampi(int(edge.get("affinity", 0)) + 1, -100, 100)
			if trust_delta != 0:
				edge["trust"] = clampi(int(edge.get("trust", 0)) + trust_delta, -100, 100)
			GameState.crew.edges[key] = edge
			relationship_changed.emit(a, b, "trust", edge["trust"])
	_history({"tick": GameState.universe.tick, "kind": "shared_event",
		"between": participants.duplicate(), "topic": topic, "trust_delta": trust_delta})
	shared_event_recorded.emit(participants, topic)
	GameState.state_changed.emit()


## The most recent shared history, newest last.
func history(limit := 10) -> Array:
	_ensure_seeded()
	var h: Array = GameState.crew.history
	return h.slice(maxi(0, h.size() - limit))


## --- seeding -------------------------------------------------------------------


## First touch (new game, or a pre-crew save): derive membership from the
## authored npc files (ship == player hull), stations from `station`
## (fallback: the hull's first interior room), and relationship edges from
## the npc `relationships` arrays (strength seeds trust; kin declared by
## both sides averages). Idempotent; a loaded save's crew block wins.
func _ensure_seeded() -> void:
	if GameState.crew.get("seeded", false):
		return
	var hull_id: String = GameState.player.ship.hull_id
	if hull_id == "":
		return  # no hull yet (boot order, or an empty new game): retry later
	GameState.crew.seeded = true
	var hull := DataRegistry.get_entity("ships", hull_id)
	var rooms: Array = hull.get("interior_rooms", [])
	if rooms.is_empty():
		# Freeform-room hulls (ship schema `rooms`) carry ids per entry.
		for entry: Dictionary in hull.get("rooms", []):
			rooms.append(entry.get("id", ""))
	var fallback_room: String = rooms[0] if not rooms.is_empty() else ""
	for npc_id in DataRegistry.ids("npcs"):
		var npc := DataRegistry.get_entity("npcs", npc_id)
		if npc.get("ship", "") != hull_id or not npc.get("aboard", false):
			continue
		GameState.crew.aboard.append(npc_id)
		var station: String = npc.get("station", "")
		GameState.crew.assignments[npc_id] = station if station in rooms else fallback_room
	for npc_id: String in GameState.crew.aboard:
		var npc := DataRegistry.get_entity("npcs", npc_id)
		for rel: Dictionary in npc.get("relationships", []):
			var other: String = rel.get("to", "")
			if other not in GameState.crew.aboard:
				continue
			var key := _edge_key(npc_id, other)
			var edge: Dictionary = GameState.crew.edges.get(key, {})
			if edge.is_empty():
				GameState.crew.edges[key] = {
					"trust": int(rel.get("strength", 0)), "affinity": 10}
			else:
				# Both sides authored the edge: average the strengths.
				edge["trust"] = int((int(edge.get("trust", 0)) + int(rel.get("strength", 0))) / 2.0)
				GameState.crew.edges[key] = edge


func _history(entry: Dictionary) -> void:
	GameState.crew.history.append(entry)
	if GameState.crew.history.size() > HISTORY_MAX:
		GameState.crew.history = GameState.crew.history.slice(
			GameState.crew.history.size() - HISTORY_MAX)


## Canonical undirected edge key.
func _edge_key(a: String, b: String) -> String:
	return a + "|" + b if a < b else b + "|" + a
