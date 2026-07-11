extends Node
## Ring 0 — the host side of the Sim Protocol
## (godot/framework/protocol/SIM-PROTOCOL.md, v0).
##
## Talks NDJSON over TCP loopback to the simulation daemon (reachlock-simd),
## the same sidecar pattern as SoulGateway/Pan. The HOST drives time: one
## tick per second while playing (this node's _process), batch advances
## across time skips (advance_batch). Fully offline-tolerant: with no daemon
## the game renders static fallback prices from authored content and shows
## no news — the game must always run without the sim.
##
## Persistence: every `advanced` reply carries a full universe snapshot,
## cached into GameState.universe["sim"], so a save at ANY moment captures
## the universe mid-motion. On connect (and on save load) the cached
## snapshot is pushed back with `load` — the universe resumes where it was.

signal connected
signal disconnected
signal advanced(tick: int)
signal prices_received(location_id: String, tick: int, prices: Array)
signal factions_received(tick: int, factions: Array)
signal news(entries: Array)

const PROTOCOL_VERSION := 0
const PROFILE := "reachlock-sim/0"
const CLIENT_NAME := "reachlock-godot/0.1.0"
const DEFAULT_PORT := 40708
const SECONDS_PER_TICK := 1.0
const NEWS_CACHE_MAX := 100

enum State { OFFLINE, CONNECTING, HANDSHAKING, READY }

var state: int = State.OFFLINE

var _peer := StreamPeerTCP.new()
var _rx_buffer := ""
var _seq := 0
var _tick_accumulator := 0.0
var _journal_after := 0        # next query_journal.since_tick
var _news_cache: Array = []    # recent entries, oldest first


func _ready() -> void:
	var port := DEFAULT_PORT
	var env_port := OS.get_environment("REACHLOCK_SIM_PORT")
	if env_port.is_valid_int():
		port = env_port.to_int()
	GameState.universe_loaded.connect(_on_universe_loaded)
	connect_to_daemon(port)


func connect_to_daemon(port: int) -> void:
	if _peer.connect_to_host("127.0.0.1", port) == OK:
		state = State.CONNECTING
	else:
		state = State.OFFLINE
		print("sim: no simulation daemon on 127.0.0.1:%d — static universe" % port)


func is_ready() -> bool:
	return state == State.READY


func _process(delta: float) -> void:
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
			if state == State.READY:
				_tick_accumulator += delta
				if _tick_accumulator >= SECONDS_PER_TICK:
					var ticks := int(_tick_accumulator / SECONDS_PER_TICK)
					_tick_accumulator -= float(ticks) * SECONDS_PER_TICK
					advance_batch(ticks)
		StreamPeerTCP.STATUS_ERROR, StreamPeerTCP.STATUS_NONE:
			if state != State.OFFLINE:
				state = State.OFFLINE
				print("sim: simulation daemon connection lost — static universe")
				disconnected.emit()


## --- outbound -----------------------------------------------------------------


## Advance the universe by `ticks` in one deterministic batch — the
## time-skip primitive (undock clearance, jump transit). The steady
## 1 tick/second while playing goes through here too.
func advance_batch(ticks: int) -> void:
	if state == State.READY and ticks > 0:
		_send("advance", {"ticks": ticks})


## Ask for the local price table of a location. The answer arrives via
## `prices_received` — live from the daemon when connected, or the static
## authored fallback (deferred, same signal) when offline. Callers never
## need to know which.
func request_prices(location_id: String) -> void:
	if state == State.READY:
		_send("query_prices", {"location_id": location_id})
	else:
		prices_received.emit.call_deferred(
			location_id, int(GameState.universe.tick), static_prices())


func request_factions() -> void:
	if state == State.READY:
		_send("query_factions", {})


## A player trade, as a sim input: amount > 0 sells TO the location
## (supply rises, price falls), amount < 0 buys FROM it. Applied exactly
## once; the refreshed local prices come back via `prices_received`.
func apply_trade(location_id: String, good_id: String, amount: int) -> void:
	if state != State.READY:
		return  # offline: static prices don't move, nothing to record
	_send("apply_input", {"input": {
		"kind": "trade", "at_location": location_id,
		"good_id": good_id, "amount": amount,
	}})
	request_prices(location_id)


## Recent journal entries already seen this session (for a feed that
## opens after the news happened). Oldest first.
func recent_news() -> Array:
	return _news_cache.duplicate()


## Static fallback price table from authored content: base prices, no
## supply/demand. What an offline universe trades at.
func static_prices() -> Array:
	var prices: Array = []
	for good_id in DataRegistry.ids("goods"):
		var good := DataRegistry.get_entity("goods", good_id)
		prices.append({
			"good_id": good_id,
			"base_price": int(good.get("base_price", 1)),
			"price": int(good.get("base_price", 1)),
			"supply": 0, "demand": 0,
		})
	return prices


func _send(type: String, body: Dictionary) -> void:
	if state == State.OFFLINE:
		return
	var envelope := {"v": PROTOCOL_VERSION, "seq": _seq, "type": type, "body": body}
	_seq += 1
	_peer.put_data((JSON.stringify(envelope) + "\n").to_utf8_buffer())


## --- inbound ------------------------------------------------------------------


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
		push_warning("sim: unparseable frame from daemon: %s" % line.left(120))
		return
	var envelope: Dictionary = parsed
	var body: Dictionary = envelope.get("body", {})
	match envelope.get("type", ""):
		"welcome":
			state = State.READY
			print("sim: connected to %s (tick %d, seed %d)" % [
				body.get("server", "?"), int(body.get("tick", 0)), int(body.get("seed", 0))])
			_push_saved_snapshot()
			connected.emit()
		"advanced":
			_on_advanced(body)
		"prices":
			prices_received.emit(
				str(body.get("location_id", "")), int(body.get("tick", 0)),
				body.get("prices", []))
		"factions":
			factions_received.emit(int(body.get("tick", 0)), body.get("factions", []))
		"journal":
			_on_journal(body)
		"ack":
			pass
		"error":
			push_warning("sim: daemon error %s: %s" % [
				body.get("code", "?"), body.get("message", "")])
		_:
			push_warning("sim: unknown message type %s" % envelope.get("type", "?"))


func _on_advanced(body: Dictionary) -> void:
	var tick := int(body.get("tick", 0))
	GameState.universe.tick = tick
	# The full snapshot rides every advance: the save's universe block is
	# always current, so a quit at any moment resumes mid-motion.
	GameState.universe["sim"] = body.get("snapshot", {})
	advanced.emit(tick)
	_send("query_journal", {"since_tick": _journal_after})


func _on_journal(body: Dictionary) -> void:
	var fresh: Array = []
	for entry: Dictionary in body.get("entries", []):
		if int(entry.get("tick", 0)) >= _journal_after:
			fresh.append(entry)
	if fresh.is_empty():
		return
	_journal_after = int(fresh[-1].get("tick", 0)) + 1
	_news_cache.append_array(fresh)
	if _news_cache.size() > NEWS_CACHE_MAX:
		_news_cache = _news_cache.slice(_news_cache.size() - NEWS_CACHE_MAX)
	news.emit(fresh)


## --- save / load --------------------------------------------------------------


## The save carried a universe snapshot: push it to the daemon so the
## sim resumes exactly where the save left it (mid-motion).
func _on_universe_loaded() -> void:
	_push_saved_snapshot()


func _push_saved_snapshot() -> void:
	if state != State.READY:
		return
	var snapshot: Dictionary = GameState.universe.get("sim", {})
	if snapshot.is_empty():
		return
	_send("load", {"snapshot": snapshot})
	GameState.universe.tick = int(snapshot.get("tick", GameState.universe.tick))
	_journal_after = int(snapshot.get("tick", 0))
	print("sim: pushed saved universe (tick %d) to the daemon" % int(snapshot.get("tick", 0)))
