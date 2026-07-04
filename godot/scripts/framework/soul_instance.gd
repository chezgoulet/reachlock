extends Node
## Ring 0 — one living soul in the world. Wraps a soul file (from DataRegistry)
## and speaks the Soul Protocol through SoulGateway. Hosts (dialogue UI,
## on-board crew sim, landed NPCs) listen to `spoke`/`acted` and provide the
## `world_context` string; this node owns goal bookkeeping and supersession.
##
## Works with no daemon: perceive calls are dropped in offline mode and the
## host falls back to authored-only behavior.

class_name SoulInstance

const SoulProfileScript := preload("res://scripts/framework/soul_profile.gd")

signal spoke(text: String)
signal acted(capability: String, args: Dictionary)
signal concluded(outcome: String)

var soul_id := ""
var mind := "rules"

var world_context := ""  # the host keeps this fresh: location, ship state…

var _soul: Dictionary = {}
var _goal_seq := 0
var _active_goal_id := ""
var _active_revision := 0
var _instantiated := false
var _pending: Array[Dictionary] = []  # perceives queued while the daemon connects


func setup(soul: Dictionary) -> void:
	_soul = soul
	soul_id = soul.get("id", "")
	mind = soul.get("mind", "rules")
	name = "Soul_" + soul_id


func _ready() -> void:
	SoulGateway.decision_received.connect(_on_decision)
	SoulGateway.connected.connect(_ensure_instantiated)
	# The store probe is async and may finish after instantiation — seed the
	# vault whenever the store comes up (ensure_seeds is idempotent).
	MemoryStore.store_online.connect(func() -> void: MemoryStore.ensure_seeds(soul_id, _soul))
	_ensure_instantiated()


func _exit_tree() -> void:
	if _instantiated:
		SoulGateway.release_soul(soul_id)


func _ensure_instantiated() -> void:
	if _instantiated or not SoulGateway.is_ready():
		return
	SoulGateway.instantiate_soul(soul_id, mind, _soul)
	MemoryStore.ensure_seeds(soul_id, _soul)
	_instantiated = true
	for queued in _pending:
		SoulGateway.perceive(soul_id, queued.goal, queued.context)
	_pending.clear()


## Someone speaks to this soul. `objective` frames the turn for the mind
## (dialogue nodes pass their prompt_hint here). What was said is also the
## recall query — the soul remembers what the moment is about. `history`
## is the running transcript of THIS conversation (memory-interface
## message form) — it rides the protocol's history channel so multi-turn
## exchanges cohere: the mind sees what was already said, not just the
## latest line.
func perceive_utterance(from: String, content: String, objective := "", history: Array = []) -> void:
	var trigger := {"kind": "utterance", "from": from, "content": content}
	var framed := objective if objective != "" else "Respond in character."
	MemoryStore.recall(soul_id, content, func(fragments: Array) -> void:
		if not is_instance_valid(self):
			return
		_perceive(trigger, framed, fragments, history))


func perceive_event(topic: String, payload: Dictionary, objective := "") -> void:
	_perceive({
		"kind": "event", "topic": topic, "payload": payload,
	}, objective if objective != "" else "React to what just happened.")


## Abandon whatever this soul is currently deciding (player walked away).
func supersede() -> void:
	if _active_goal_id == "":
		return
	_active_revision += 1
	SoulGateway.perceive(soul_id, {
		"id": _active_goal_id,
		"revision": _active_revision,
		"objective": "The moment passed; let it go.",
		"trigger": {"kind": "event", "topic": "dialogue.abandoned", "payload": {}},
	}, {"fragments": []})


func _perceive(trigger: Dictionary, objective: String, recalled: Array = [], history: Array = []) -> void:
	_ensure_instantiated()
	_goal_seq += 1
	_active_goal_id = "%s_g%d" % [soul_id, _goal_seq]
	_active_revision = 1
	var goal := {
		"id": _active_goal_id,
		"revision": _active_revision,
		"objective": objective,
		"trigger": trigger,
	}
	if SoulGateway.is_ready():
		SoulGateway.perceive(soul_id, goal, _assemble_context(recalled, history))
	elif SoulGateway.state != SoulGateway.State.OFFLINE:
		# Mid-handshake: hold the moment until the daemon is with us.
		_pending.append({"goal": goal, "context": _assemble_context(recalled, history)})


## Channel order per the protocol's REACHLOCK profile: persona, memory,
## history, world.
func _assemble_context(recalled: Array = [], history: Array = []) -> Dictionary:
	var fragments: Array = [
		{"channel": "persona", "body": SoulProfileScript.persona_fragment(_soul)},
	]
	if not recalled.is_empty():
		# The vault owns the past: live recall, most relevant first.
		var lines: PackedStringArray = []
		for fragment: String in recalled:
			lines.append("- " + fragment.strip_edges())
		fragments.append({"channel": "memory", "body": "You remember:\n" + "\n".join(lines)})
	else:
		# Store offline (or nothing relevant): authored seeds are the past.
		var seeds := SoulProfileScript.seed_memory_fragment(_soul)
		if seeds != "":
			fragments.append({"channel": "memory", "body": seeds})
	if not history.is_empty():
		var turns: PackedStringArray = []
		for turn: Dictionary in history:
			var who := "You" if turn.get("role", "") == "assistant" else "They"
			turns.append("%s: %s" % [who, turn.get("content", "")])
		fragments.append({"channel": "history",
			"body": "The conversation so far:\n" + "\n".join(turns)})
	if world_context != "":
		fragments.append({"channel": "world", "body": world_context})
	return {"fragments": fragments}


func _on_decision(for_soul: String, goal_id: String, _revision: int, decision: Dictionary) -> void:
	if for_soul != soul_id or goal_id != _active_goal_id:
		return
	for intent: Dictionary in decision.get("intents", []):
		match intent.get("intent", ""):
			"express":
				spoke.emit(str(intent.get("body", "")))
			"invoke":
				acted.emit(str(intent.get("capability", "")), intent.get("args", {}))
			"conclude":
				concluded.emit(str(intent.get("outcome", "continue")))
