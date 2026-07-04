extends Node
## Ring 0 — walks a dialogue graph (dialogue schema, C6). Authored nodes show
## fixed text; generated nodes hand their prompt_hint to the NPC's soul as the
## perceive objective and wait (bounded) for the mind's line — authored
## structure and improvisation interleave in one conversation. Works with no
## daemon: generated nodes fall back to their optional `text`, else a quiet
## beat. Mutations and choice conditions run against GameState.

class_name DialogueRunner

const TriggerDSLScript := preload("res://scripts/framework/trigger_dsl.gd")
# Local CPU inference takes seconds; the abandon fail-fast below covers true
# failures, so the ceiling only guards a hung daemon.
const GENERATED_TIMEOUT := 15.0

signal line_shown(speaker: String, text: String)
signal choices_shown(choices: Array)  # [{index, text}]
signal ended

var _dialogue: Dictionary = {}
var _soul: SoulInstance = null
var _npc_name := ""
var _last_player_line := ""
var _awaiting_generated := false
var _current_choices: Array = []
var _transcript: Array = []  # [{role: "user"|"assistant", content}] for memory ingest


## Guard-check and begin. Returns false if the dialogue's condition fails.
func start(dialogue: Dictionary, soul: SoulInstance) -> bool:
	var guard: String = dialogue.get("condition", "")
	if guard != "" and not TriggerDSLScript.evaluate(guard, GameState.context()):
		return false
	_dialogue = dialogue
	_soul = soul
	var npc: Dictionary = DataRegistry.get_entity("npcs", dialogue.get("npc", ""))
	_npc_name = npc.get("name", dialogue.get("npc", "?"))
	if _soul != null:
		_soul.spoke.connect(_on_soul_spoke)
		_soul.concluded.connect(_on_soul_concluded)
	_enter_node(dialogue.get("entry", ""))
	return true


## The finished conversation in memory-interface message form.
func transcript() -> Array:
	return _transcript.duplicate()


func npc_id() -> String:
	return _dialogue.get("npc", "")


func choose(index: int) -> void:
	if index < 0 or index >= _current_choices.size():
		return
	var choice: Dictionary = _current_choices[index]
	_last_player_line = choice.get("text", "")
	_transcript.append({"role": "user", "content": _last_player_line})
	line_shown.emit("You", _last_player_line)
	_apply_mutations(choice.get("mutations", []))
	_current_choices = []
	_enter_node(choice.get("goto", "end"))


func _enter_node(node_id: String) -> void:
	if node_id == "end" or node_id == "":
		_finish()
		return
	var node: Dictionary = _dialogue.get("nodes", {}).get(node_id, {})
	if node.is_empty():
		push_warning("dialogue: node '%s' missing in '%s'" % [node_id, _dialogue.get("id", "?")])
		_finish()
		return
	_apply_mutations(node.get("mutations", []))
	match node.get("kind", "authored"):
		"authored":
			_npc_line(node.get("text", ""))
			_offer_or_continue(node)
		"generated":
			_run_generated(node)


func _run_generated(node: Dictionary) -> void:
	_generated_node = node
	if _soul != null and SoulGateway.is_ready():
		_awaiting_generated = true
		# Multi-turn coherence (M7): the mind sees the conversation so far
		# via the history channel. The last transcript entry IS the current
		# player line (it rides the trigger), so exclude it.
		var history := _transcript.slice(maxi(0, _transcript.size() - 9), _transcript.size() - 1) \
			if _transcript.size() > 1 else []
		_soul.perceive_utterance("player", _last_player_line, node.get("prompt_hint", ""), history)
		var timer := get_tree().create_timer(GENERATED_TIMEOUT)
		timer.timeout.connect(_on_generated_timeout.bind(node), CONNECT_ONE_SHOT)
	else:
		_npc_line(_generated_fallback(node))
		_offer_or_continue(node)


func _on_soul_spoke(text: String) -> void:
	if not _awaiting_generated:
		return
	_awaiting_generated = false
	_npc_line(text)
	_offer_or_continue(_current_generated_node())


func _on_generated_timeout(node: Dictionary) -> void:
	if not _awaiting_generated:
		return
	_awaiting_generated = false
	_npc_line(_generated_fallback(node))
	_offer_or_continue(node)


## The mind gave up (inference failure) — fall back now, don't make the
## player wait out the timeout. `achieved` outcomes ride behind an express,
## which clears _awaiting_generated first, so only failures land here.
func _on_soul_concluded(outcome: String) -> void:
	if not _awaiting_generated or outcome != "abandoned":
		return
	_awaiting_generated = false
	_npc_line(_generated_fallback(_generated_node))
	_offer_or_continue(_generated_node)


var _generated_node: Dictionary = {}

func _current_generated_node() -> Dictionary:
	return _generated_node


func _npc_line(text: String) -> void:
	if text.strip_edges() != "":
		_transcript.append({"role": "assistant", "content": text})
	line_shown.emit(_npc_name, text)


func _generated_fallback(node: Dictionary) -> String:
	if node.get("text", "") != "":
		return node.text
	return "…"  # the mind is elsewhere; the moment passes


func _offer_or_continue(node: Dictionary) -> void:
	_generated_node = node
	var context := GameState.context()
	_current_choices = []
	var offered: Array = []
	var choices: Array = node.get("choices", [])
	for choice: Dictionary in choices:
		var condition: String = choice.get("condition", "")
		if condition == "" or TriggerDSLScript.evaluate(condition, context):
			_current_choices.append(choice)
			offered.append({"index": _current_choices.size() - 1, "text": choice.get("text", "")})
	if not _current_choices.is_empty():
		choices_shown.emit(offered)
	elif choices.is_empty():
		_enter_node(node.get("goto", "end"))
	else:
		_finish()  # all choices condition-gated away: close cleanly


func _apply_mutations(mutations: Array) -> void:
	for mutation: Dictionary in mutations:
		GameState.apply_soul_mutation(_dialogue.get("npc", ""), mutation)


func _finish() -> void:
	if _soul != null:
		if _soul.spoke.is_connected(_on_soul_spoke):
			_soul.spoke.disconnect(_on_soul_spoke)
		if _soul.concluded.is_connected(_on_soul_concluded):
			_soul.concluded.disconnect(_on_soul_concluded)
	ended.emit()
