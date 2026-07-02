extends Node
## Ring 0 — the host side of the Soul Protocol
## (godot/framework/protocol/SOUL-PROTOCOL.md, frozen v0).
##
## Talks NDJSON over TCP loopback to a mind daemon (Pan). Fully asynchronous:
## `perceive()` returns immediately and the decision arrives later via
## `decision_received`. If no daemon is reachable the gateway goes into
## offline mode — souls simply never decide, and hosts fall back to static
## behavior. The game must always be playable with no daemon running.

signal connected
signal disconnected
signal decision_received(soul_id: String, goal_id: String, goal_revision: int, decision: Dictionary)
signal daemon_error(code: String, message: String)

const PROTOCOL_VERSION := 0
const PROFILE := "reachlock/0"
const CLIENT_NAME := "reachlock-godot/0.1.0"
const DEFAULT_PORT := 40707

const SoulProfileScript := preload("res://scripts/framework/soul_profile.gd")

enum State { OFFLINE, CONNECTING, HANDSHAKING, READY }

var state: int = State.OFFLINE

var _peer := StreamPeerTCP.new()
var _rx_buffer := ""
var _seq := 0
var _latest_goal_revision := {}  # goal_id -> highest revision perceived
var _instantiated: Dictionary = {}  # soul_id -> true


func _ready() -> void:
	var port := DEFAULT_PORT
	var env_port := OS.get_environment("REACHLOCK_PAN_PORT")
	if env_port.is_valid_int():
		port = env_port.to_int()
	connect_to_daemon(port)


func connect_to_daemon(port: int) -> void:
	if _peer.connect_to_host("127.0.0.1", port) == OK:
		state = State.CONNECTING
	else:
		state = State.OFFLINE
		print("souls: no mind daemon on 127.0.0.1:%d — running offline" % port)


func is_ready() -> bool:
	return state == State.READY


func _process(_delta: float) -> void:
	if state == State.OFFLINE:
		return
	_peer.poll()
	match _peer.get_status():
		StreamPeerTCP.STATUS_CONNECTED:
			if state == State.CONNECTING:
				state = State.HANDSHAKING
				_send("hello", {
					"protocol_version": PROTOCOL_VERSION,
					"profile": PROFILE,
					"client": CLIENT_NAME,
				})
			_drain()
		StreamPeerTCP.STATUS_ERROR, StreamPeerTCP.STATUS_NONE:
			if state != State.OFFLINE:
				state = State.OFFLINE
				print("souls: mind daemon connection lost — running offline")
				disconnected.emit()


## --- outbound ---------------------------------------------------------------


func register_capabilities(capabilities: Array) -> void:
	_send("register_capabilities", {"capabilities": capabilities})


func instantiate_soul(soul_id: String, mind: String, soul: Dictionary) -> void:
	_instantiated[soul_id] = true
	_send("instantiate_soul", {"soul_id": soul_id, "mind": mind, "soul": soul})


func release_soul(soul_id: String) -> void:
	_instantiated.erase(soul_id)
	_send("release_soul", {"soul_id": soul_id})


## Fire-and-forget: the decision arrives via `decision_received`. A repeat
## call with the same goal id and a higher revision supersedes the older one.
func perceive(soul_id: String, goal: Dictionary, context: Dictionary) -> void:
	var goal_id: String = goal.get("id", "")
	_latest_goal_revision[goal_id] = int(goal.get("revision", 0))
	_send("perceive", {"soul_id": soul_id, "goal": goal, "context": context})


func shutdown_daemon() -> void:
	_send("shutdown", {})


func _send(type: String, body: Dictionary) -> void:
	if state == State.OFFLINE:
		return
	var envelope := {"v": PROTOCOL_VERSION, "seq": _seq, "type": type, "body": body}
	_seq += 1
	_peer.put_data((JSON.stringify(envelope) + "\n").to_utf8_buffer())


## --- inbound ----------------------------------------------------------------


func _drain() -> void:
	var available := _peer.get_available_bytes()
	if available <= 0:
		return
	var chunk := _peer.get_data(available)
	if chunk[0] != OK:
		return
	_rx_buffer += (chunk[1] as PackedByteArray).get_string_from_utf8()
	while true:
		var newline := _rx_buffer.find("\n")
		if newline < 0:
			break
		var line := _rx_buffer.substr(0, newline)
		_rx_buffer = _rx_buffer.substr(newline + 1)
		if line.strip_edges() != "":
			_handle_line(line)


func _handle_line(line: String) -> void:
	var parsed: Variant = JSON.parse_string(line)
	if not parsed is Dictionary:
		push_warning("souls: unparseable frame from daemon: %s" % line.left(120))
		return
	var envelope: Dictionary = parsed
	var body: Dictionary = envelope.get("body", {})
	match envelope.get("type", ""):
		"welcome":
			state = State.READY
			print("souls: connected to %s (protocol %d)" % [
				body.get("server", "?"), int(body.get("protocol_version", -1))])
			register_capabilities(SoulProfileScript.capabilities())
			connected.emit()
		"decision":
			_handle_decision(body)
		"ack":
			pass
		"error":
			var code: String = body.get("code", "unknown")
			push_warning("souls: daemon error %s: %s" % [code, body.get("message", "")])
			daemon_error.emit(code, body.get("message", ""))
		_:
			push_warning("souls: unknown message type %s" % envelope.get("type", "?"))


func _handle_decision(body: Dictionary) -> void:
	var goal_id: String = body.get("goal_id", "")
	var revision := int(body.get("goal_revision", 0))
	# Supersession (spec): drop any decision for a stale revision.
	if revision < int(_latest_goal_revision.get(goal_id, 0)):
		print("souls: dropped stale decision for %s rev %d" % [goal_id, revision])
		return
	decision_received.emit(
		body.get("soul_id", ""), goal_id, revision, body.get("decision", {})
	)
