extends Node
## Ring 0 — the host side of the Ear Protocol
## (godot/framework/protocol/EAR-PROTOCOL.md, v0).
##
## Talks NDJSON over TCP loopback to the speech daemon (reachlock-eard).
## Push-to-talk shaped: start_listening() opens an utterance and streams
## mic audio as ~250 ms PCM16 chunks; stop_listening() closes it and the
## transcript arrives via `transcript_ready`. The transcript is TEXT — it
## enters the game exactly like typed input, which is the whole contract.
##
## Optional and silent like pan: no daemon on the port → state stays
## OFFLINE, available() is false, and the voice affordance simply does not
## exist anywhere in the UI. No error spam, no greyed-out button.

signal connected
signal disconnected
signal listening_changed(listening: bool)
signal partial_ready(text: String)
signal transcript_ready(text: String, confidence: float)

const PROTOCOL_VERSION := 0
const PROFILE := "reachlock/0"
const CLIENT_NAME := "reachlock-godot/0.1.0"
const DEFAULT_PORT := 40709
const TARGET_RATE := 16000
const CHUNK_SECONDS := 0.25

enum State { OFFLINE, CONNECTING, HANDSHAKING, READY }

var state: int = State.OFFLINE

var _peer := StreamPeerTCP.new()
var _rx_buffer := ""
var _seq := 0
var _utterance_seq := 0
var _active_utterance := ""  # "" = not listening
var _awaiting_utterance := ""  # transcript may arrive after key-up

var _mic_player: AudioStreamPlayer = null
var _capture: AudioEffectCapture = null
var _pending_samples := PackedFloat32Array()


func _ready() -> void:
	var port := DEFAULT_PORT
	var env_port := OS.get_environment("REACHLOCK_EAR_PORT")
	if env_port.is_valid_int():
		port = env_port.to_int()
	connect_to_daemon(port)


func connect_to_daemon(port: int) -> void:
	if _peer.connect_to_host("127.0.0.1", port) == OK:
		state = State.CONNECTING
	else:
		state = State.OFFLINE
		print("ear: no speech daemon on 127.0.0.1:%d — voice does not exist" % port)


## The one question every UI asks before showing a voice affordance.
func available() -> bool:
	return state == State.READY


func is_listening() -> bool:
	return _active_utterance != ""


## --- push-to-talk -----------------------------------------------------------


## Key down. Returns false when voice is unavailable or already listening.
func start_listening() -> bool:
	if not available() or is_listening():
		return false
	if not _ensure_microphone():
		return false
	_utterance_seq += 1
	_active_utterance = "utt_%d" % _utterance_seq
	_awaiting_utterance = _active_utterance
	_pending_samples.clear()
	_capture.clear_buffer()
	_send("audio_begin", {
		"utterance_id": _active_utterance,
		"sample_rate": TARGET_RATE,
		"format": "pcm16",
	})
	listening_changed.emit(true)
	return true


## Key up: flush the tail and ask for the transcript.
func stop_listening() -> void:
	if not is_listening():
		return
	_drain_microphone()
	_flush_chunk(true)
	_send("audio_end", {"utterance_id": _active_utterance})
	_active_utterance = ""
	listening_changed.emit(false)


## Abandon the in-flight utterance (dialogue closed mid-listen).
func cancel_listening() -> void:
	if _awaiting_utterance == "" and not is_listening():
		return
	var cancelled := _active_utterance if _active_utterance != "" else _awaiting_utterance
	_send("cancel", {"utterance_id": cancelled})
	if is_listening():
		listening_changed.emit(false)
	_active_utterance = ""
	_awaiting_utterance = ""


## --- microphone -------------------------------------------------------------


## Lazily build the capture path: a dedicated muted bus with an
## AudioEffectCapture fed by an AudioStreamMicrophone. Built only on the
## first key-down, so headless runs and voiceless players never touch
## audio input at all.
func _ensure_microphone() -> bool:
	if _capture != null:
		if not _mic_player.playing:
			_mic_player.play()
		return true
	var bus_index := AudioServer.bus_count
	AudioServer.add_bus(bus_index)
	AudioServer.set_bus_name(bus_index, "EarCapture")
	AudioServer.set_bus_mute(bus_index, true)
	var effect := AudioEffectCapture.new()
	effect.buffer_length = 1.0
	AudioServer.add_bus_effect(bus_index, effect)
	_capture = AudioServer.get_bus_effect(bus_index, 0) as AudioEffectCapture
	_mic_player = AudioStreamPlayer.new()
	_mic_player.stream = AudioStreamMicrophone.new()
	_mic_player.bus = "EarCapture"
	add_child(_mic_player)
	_mic_player.play()
	return _capture != null


func _drain_microphone() -> void:
	if _capture == null or not is_listening():
		return
	var frames := _capture.get_frames_available()
	if frames <= 0:
		return
	var buffer := _capture.get_buffer(frames)
	for frame: Vector2 in buffer:
		_pending_samples.append((frame.x + frame.y) * 0.5)


## Downsample the pending mono floats to 16 kHz PCM16 and ship a chunk
## when ~250 ms has accumulated (or on `force`, the key-up flush).
func _flush_chunk(force := false) -> void:
	var mix_rate := AudioServer.get_mix_rate()
	var needed := int(mix_rate * CHUNK_SECONDS)
	if _pending_samples.size() < needed and not (force and _pending_samples.size() > 0):
		return
	var ratio := mix_rate / float(TARGET_RATE)
	var out_count := int(_pending_samples.size() / ratio)
	if out_count <= 0:
		_pending_samples.clear()
		return
	var bytes := PackedByteArray()
	bytes.resize(out_count * 2)
	for i in out_count:
		var source := i * ratio
		var lo := int(source)
		var hi := mini(lo + 1, _pending_samples.size() - 1)
		var t := source - float(lo)
		var sample := lerpf(_pending_samples[lo], _pending_samples[hi], t)
		var value := int(clampf(sample, -1.0, 1.0) * 32767.0)
		bytes.encode_s16(i * 2, value)
	_pending_samples.clear()
	_send("audio_chunk", {
		"utterance_id": _active_utterance,
		"data": Marshalls.raw_to_base64(bytes),
	})


## --- wire -------------------------------------------------------------------


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
			if is_listening():
				_drain_microphone()
				_flush_chunk()
			_drain_wire()
		StreamPeerTCP.STATUS_ERROR, StreamPeerTCP.STATUS_NONE:
			if state != State.OFFLINE:
				state = State.OFFLINE
				_active_utterance = ""
				_awaiting_utterance = ""
				print("ear: speech daemon connection lost — voice folds away")
				disconnected.emit()


func _send(type: String, body: Dictionary) -> void:
	if state == State.OFFLINE:
		return
	var envelope := {"v": PROTOCOL_VERSION, "seq": _seq, "type": type, "body": body}
	_seq += 1
	_peer.put_data((JSON.stringify(envelope) + "\n").to_utf8_buffer())


func _drain_wire() -> void:
	var available_bytes := _peer.get_available_bytes()
	if available_bytes <= 0:
		return
	var chunk := _peer.get_data(available_bytes)
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
		push_warning("ear: unparseable frame from daemon: %s" % line.left(120))
		return
	var body: Dictionary = (parsed as Dictionary).get("body", {})
	match (parsed as Dictionary).get("type", ""):
		"welcome":
			state = State.READY
			print("ear: connected to %s (%s, model %s)" % [
				body.get("server", "?"), body.get("engine", "?"), body.get("model", "?")])
			connected.emit()
		"partial":
			if body.get("utterance_id", "") == _awaiting_utterance:
				partial_ready.emit(str(body.get("text", "")))
		"transcript":
			# Late transcripts for utterances we dropped resolve in our
			# favor (spec): only the one we're waiting on gets through.
			if body.get("utterance_id", "") != _awaiting_utterance:
				return
			_awaiting_utterance = ""
			transcript_ready.emit(str(body.get("text", "")),
				float(body.get("confidence", 0.0)))
		"ack":
			pass
		"error":
			push_warning("ear: daemon error %s: %s" % [
				body.get("code", "?"), body.get("message", "")])
		_:
			push_warning("ear: unknown message type %s" % (parsed as Dictionary).get("type", "?"))
