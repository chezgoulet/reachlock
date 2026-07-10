extends Node
## Ring 0 — Ship-Share: co-op crew on one boat
## (godot/framework/protocol/SHIP-SHARE.md, v0).
##
## Host-authoritative by contract: souls, sim, missions, weave resolution,
## RNG, and the save live on the host; clients send INTENTS and receive
## STATE. This node is both halves — a solo game IS a hosted game with
## zero peers, minus the listen socket (nothing binds until the player
## opens the hatch), so multiplayer can never block single-player.
##
## The core is deliberately transport-free: every intent lands in
## `handle_intent(peer, payload)` and every state message leaves through
## `_deliver(peer, payload)`, so the whole seat/station/dialogue contract
## is tested headlessly with the fixture payloads (test_ship_share.gd) and
## Godot's ENet is a thin pipe underneath.

signal roster_changed
signal seats_changed
signal pawn_updated(peer: int, position: Vector2, facing: String, anim: String)
signal share_refused(reason: String, host_version: int, yours: int)
signal share_joined
signal dialogue_state(kind: String, body: Dictionary)

const SHARE_VERSION := 0
const DEFAULT_PORT := 40710
const MAX_PEERS := 6
const HOST_PEER := 1

enum Mode { SOLO, HOSTING, JOINED }

var mode: int = Mode.SOLO

## Host truth: peer id -> {name, npc_id} ("" npc_id = unseated).
var players: Dictionary = {}
## Host truth: npc_id -> peer id. Claimed crew stop being NPCs.
var seats: Dictionary = {}

var _peer: ENetMultiplayerPeer = null


func _ready() -> void:
	multiplayer.peer_connected.connect(_on_peer_connected)
	multiplayer.peer_disconnected.connect(_on_peer_disconnected)
	multiplayer.connected_to_server.connect(_on_connected_to_host)
	multiplayer.connection_failed.connect(func() -> void: stop())
	multiplayer.server_disconnected.connect(func() -> void: stop())


func is_authority() -> bool:
	return mode != Mode.JOINED


## Hosting is one button. Returns false (and stays solo) on a bind failure.
func host(port := 0) -> bool:
	if mode != Mode.SOLO:
		return false
	if port == 0:
		port = _default_port()
	_peer = ENetMultiplayerPeer.new()
	if _peer.create_server(port, MAX_PEERS) != OK:
		_peer = null
		print("share: could not bind port %d — staying solo" % port)
		return false
	multiplayer.multiplayer_peer = _peer
	mode = Mode.HOSTING
	players[HOST_PEER] = {"name": _player_name(), "npc_id": GameState.player_character()}
	if GameState.player_character() != "":
		seats[GameState.player_character()] = HOST_PEER
	print("share: hosting on port %d" % port)
	roster_changed.emit()
	seats_changed.emit()
	return true


func join(address: String, port := 0) -> bool:
	if mode != Mode.SOLO:
		return false
	if port == 0:
		port = _default_port()
	_peer = ENetMultiplayerPeer.new()
	if _peer.create_client(address, port) != OK:
		_peer = null
		return false
	multiplayer.multiplayer_peer = _peer
	mode = Mode.JOINED
	print("share: joining %s:%d" % [address, port])
	return true


## Back to solo. The deck empties; the world (host's) sails on.
func stop() -> void:
	if _peer != null:
		_peer.close()
		_peer = null
		multiplayer.multiplayer_peer = OfflineMultiplayerPeer.new()
	mode = Mode.SOLO
	players.clear()
	seats.clear()
	roster_changed.emit()
	seats_changed.emit()


## A crew member with a player inside is not an NPC (SHIP-SHARE.md seats;
## generalizes the single-player chosen-character substitution).
func is_claimed(npc_id: String) -> bool:
	if npc_id != "" and npc_id == GameState.player_character():
		return true
	return seats.has(npc_id)


## --- intents (client side sends; host side handles) --------------------------


func send_intent(payload: Dictionary) -> void:
	if mode == Mode.JOINED:
		_intent_rpc.rpc_id(HOST_PEER, payload)
	else:
		# Solo/hosting: the local player's intents take the same door as
		# everyone else's — one code path, one set of rules.
		handle_intent(HOST_PEER, payload)


func send_move(position: Vector2, facing: String, anim: String) -> void:
	if mode == Mode.SOLO:
		return  # nobody to tell
	send_intent({"kind": "move", "body": {
		"position": [position.x, position.y], "facing": facing, "anim": anim}})


## --- the host core (transport-free, fixture-tested) --------------------------


## Process one intent payload from `peer`. Every rule the contract states
## lives here; the RPC layer below is just delivery.
func handle_intent(peer: int, payload: Dictionary) -> void:
	if not is_authority():
		return
	var body: Dictionary = payload.get("body", {})
	match str(payload.get("kind", "")):
		"hello":
			_on_hello(peer, body)
		"claim_seat":
			_on_claim_seat(peer, body)
		"release_seat":
			_release_seat(peer)
			_broadcast_seats()
		"move":
			_on_move(peer, body)
		"station":
			_on_station(peer, body)
		"control":
			_on_control(peer, body)
		"choose":
			_on_choose(peer, body)
		"say":
			_on_say(peer, body)
		_:
			print("share: unknown intent kind %s from peer %d" % [payload.get("kind", "?"), peer])


func _on_hello(peer: int, body: Dictionary) -> void:
	var theirs := int(body.get("share_version", -1))
	if theirs != SHARE_VERSION:
		# Refused loudly: the payload names both versions so the joining
		# player reads a sentence, not a log line.
		_deliver(peer, {"kind": "refuse", "body": {
			"reason": "version_mismatch", "host_version": SHARE_VERSION, "yours": theirs}})
		_disconnect_peer(peer)
		return
	players[peer] = {"name": str(body.get("name", "crew")), "npc_id": ""}
	_deliver(peer, {"kind": "welcome", "body": {
		"share_version": SHARE_VERSION, "roster": _roster(), "seats": seats.duplicate()}})
	_broadcast_roster()


func _on_claim_seat(peer: int, body: Dictionary) -> void:
	var npc_id := str(body.get("npc_id", ""))
	if npc_id == "" or not players.has(peer):
		return
	if seats.has(npc_id) and int(seats[npc_id]) != peer:
		_deliver(peer, {"kind": "seat_denied", "body": {
			"npc_id": npc_id, "held_by": int(seats[npc_id])}})
		return
	_release_seat(peer)  # one body per player
	seats[npc_id] = peer
	players[peer]["npc_id"] = npc_id
	_broadcast_seats()
	_broadcast_roster()


func _release_seat(peer: int) -> void:
	for npc_id: String in seats.keys():
		if int(seats[npc_id]) == peer:
			seats.erase(npc_id)
	if players.has(peer):
		players[peer]["npc_id"] = ""


func _on_move(peer: int, body: Dictionary) -> void:
	var position: Array = body.get("position", [0, 0])
	var pawn := {"kind": "pawn", "body": {
		"peer": peer,
		"position": position,
		"facing": str(body.get("facing", "down")),
		"anim": str(body.get("anim", "idle")),
	}}
	for other: int in players:
		if other != peer:
			_deliver(other, pawn)


func _on_station(peer: int, body: Dictionary) -> void:
	var station_id := str(body.get("station_id", ""))
	var who := _crew_id_for(peer)
	match str(body.get("op", "")):
		"occupy":
			# Same rule as solo: an occupied station is occupied. First
			# pair of hands wins; the loser sees the truth in state.
			if not ShipOperation.is_occupied(station_id):
				ShipOperation.occupy(station_id, who)
		"vacate":
			if ShipOperation.occupied_by(station_id) == who:
				ShipOperation.vacate(station_id)
	_broadcast_station_state()


func _on_control(peer: int, body: Dictionary) -> void:
	var station_id := str(body.get("station_id", ""))
	# Only the hands on the station move its controls.
	if ShipOperation.occupied_by(station_id) != _crew_id_for(peer):
		return
	ShipOperation.set_control(station_id, str(body.get("axis", "")), body.get("value"))
	_broadcast_station_state()


func _on_choose(peer: int, body: Dictionary) -> void:
	# The host runs every DialogueRunner; a client answer arrives here.
	dialogue_state.emit("choose", {"peer": peer,
		"dialogue_id": str(body.get("dialogue_id", "")),
		"index": int(body.get("index", -1))})


func _on_say(peer: int, body: Dictionary) -> void:
	# Typed or a voice transcript — the host cannot tell, by design
	# (EAR-PROTOCOL.md). Rides to the open dialogue as free speech.
	dialogue_state.emit("say", {"peer": peer, "text": str(body.get("text", ""))})


func _crew_id_for(peer: int) -> String:
	var npc_id := str(players.get(peer, {}).get("npc_id", ""))
	if npc_id != "":
		return npc_id
	return "player" if peer == HOST_PEER else "guest_%d" % peer


## --- state fan-out ------------------------------------------------------------


func _roster() -> Array:
	var out: Array = []
	for peer: int in players:
		out.append({"peer": peer, "name": players[peer].get("name", ""),
			"npc_id": players[peer].get("npc_id", "")})
	return out


func _broadcast_roster() -> void:
	_broadcast({"kind": "roster", "body": {"players": _roster()}})
	roster_changed.emit()


func _broadcast_seats() -> void:
	_broadcast({"kind": "seats", "body": {"claimed": seats.duplicate()}})
	seats_changed.emit()


func _broadcast_station_state() -> void:
	_broadcast({"kind": "station_state", "body": {
		"stations": ShipOperation.stations.duplicate(true),
		"controls": ShipOperation.controls.duplicate(true)}})


func _broadcast(payload: Dictionary) -> void:
	for peer: int in players:
		if peer != HOST_PEER:
			_deliver(peer, payload)


## Delivery seam: state to one peer. The host delivering to itself is a
## local dispatch (no socket); tests override nothing — they read the
## sent log via `deliveries` when transport is absent.
var deliveries: Array = []  # [{peer, payload}] — populated when offline (tests)

func _deliver(peer: int, payload: Dictionary) -> void:
	if peer == HOST_PEER:
		_apply_state(payload)
		return
	if _peer == null:
		deliveries.append({"peer": peer, "payload": payload})
		return
	_state_rpc.rpc_id(peer, payload)


## --- client side --------------------------------------------------------------


func _on_connected_to_host() -> void:
	send_intent({"kind": "hello", "body": {
		"share_version": SHARE_VERSION,
		"game_version": ProjectSettings.get_setting("application/config/version", "0"),
		"name": _player_name()}})


## Apply one state payload (client, or host self-dispatch).
func _apply_state(payload: Dictionary) -> void:
	var body: Dictionary = payload.get("body", {})
	match str(payload.get("kind", "")):
		"welcome":
			seats = body.get("seats", {})
			_ingest_roster(body.get("roster", []))
			share_joined.emit()
		"refuse":
			var reason := str(body.get("reason", "?"))
			print("share: refused — %s (host runs share v%d, you run v%d)" % [
				reason, int(body.get("host_version", -1)), int(body.get("yours", -1))])
			share_refused.emit(reason, int(body.get("host_version", -1)), int(body.get("yours", -1)))
			stop()
		"roster":
			_ingest_roster(body.get("players", []))
		"seats":
			seats = body.get("claimed", {})
			seats_changed.emit()
		"pawn":
			var position: Array = body.get("position", [0, 0])
			pawn_updated.emit(int(body.get("peer", 0)),
				Vector2(float(position[0]), float(position[1])),
				str(body.get("facing", "down")), str(body.get("anim", "idle")))
		"station_state":
			if mode == Mode.JOINED:
				ShipOperation.stations = body.get("stations", {})
				ShipOperation.controls = body.get("controls", {})
		"dialogue_line", "dialogue_choices", "dialogue_ended", "world":
			dialogue_state.emit(str(payload.get("kind", "")), body)
		_:
			pass


func _ingest_roster(roster: Array) -> void:
	players.clear()
	for entry: Dictionary in roster:
		players[int(entry.get("peer", 0))] = {
			"name": str(entry.get("name", "")), "npc_id": str(entry.get("npc_id", ""))}
	roster_changed.emit()


## --- transport (thin) -----------------------------------------------------------


@rpc("any_peer", "call_remote", "reliable")
func _intent_rpc(payload: Dictionary) -> void:
	# Trust the transport for identity, never the payload.
	handle_intent(multiplayer.get_remote_sender_id(), payload)


@rpc("authority", "call_remote", "reliable")
func _state_rpc(payload: Dictionary) -> void:
	_apply_state(payload)


func _on_peer_connected(peer: int) -> void:
	if is_authority():
		print("share: peer %d connected — awaiting hello" % peer)


func _on_peer_disconnected(peer: int) -> void:
	if not is_authority():
		return
	_release_seat(peer)
	players.erase(peer)
	_broadcast_seats()
	_broadcast_roster()


func _disconnect_peer(peer: int) -> void:
	if _peer != null:
		_peer.disconnect_peer(peer)
	players.erase(peer)


func _default_port() -> int:
	var env_port := OS.get_environment("REACHLOCK_SHARE_PORT")
	return env_port.to_int() if env_port.is_valid_int() else DEFAULT_PORT


func _player_name() -> String:
	var character := GameState.player_character()
	if character != "":
		return DataRegistry.get_entity("npcs", character).get("name", character)
	return "Captain"
