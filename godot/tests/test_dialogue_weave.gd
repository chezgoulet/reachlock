extends GutTest
## Contract test: woven nodes through the DialogueRunner — generation is
## proposed once, clamped by the loom, persisted in the save, and replayed
## as ordinary dialogue data (WEAVE-CONTRACT.md). Runs with no daemon and
## no network: the mind is stubbed and proposals are delivered by hand.

class StubRunner extends DialogueRunner:
	var requested := 0
	var live_mind := true

	func _can_generate() -> bool:
		return live_mind

	func _request_generation(_node: Dictionary) -> void:
		requested += 1


var _lines: Array = []
var _choice_batches: Array = []
var _ended := false

const PROPOSAL_JSON := """{"line": "Third shift, the picket thins.",
	"mutations": [{"op": "adjust_faction", "faction": "reach_compact", "axis": "trust", "amount": 40}],
	"choices": [{"text": "Why tell me?"},
		{"text": "I owe you one.", "mutations": [{"op": "set_player_flag", "flag": "heard_cordon_rumor"}]}]}"""


func before_each() -> void:
	GameState.weaves.clear()
	GameState.factions.clear()
	GameState.player.flags = []


func _make_runner() -> StubRunner:
	var runner := StubRunner.new()
	add_child_autofree(runner)
	_lines = []
	_choice_batches = []
	_ended = false
	runner.line_shown.connect(func(_speaker: String, text: String) -> void:
		_lines.append(text))
	runner.choices_shown.connect(func(choices: Array) -> void:
		_choice_batches.append(choices))
	runner.ended.connect(func() -> void: _ended = true)
	return runner


func _woven_dialogue() -> Dictionary:
	return {"id": "test_weave", "npc": "test_npc", "entry": "rumor", "nodes": {
		"rumor": {"kind": "woven",
			"prompt_hint": "Trade a rumor.",
			"text": "Ask me another day.",
			"return_to": "after",
			"goto": "end",
			"may": {"grants": [
				{"op": "adjust_faction", "factions": ["reach_compact"], "axes": ["trust"], "max_amount": 3},
				{"op": "set_player_flag", "flags": ["heard_cordon_rumor"]},
			], "max_choices": 3, "max_mutations": 3}},
		"after": {"kind": "authored", "text": "Anyway.", "goto": "end"},
	}}


func test_offline_woven_plays_the_authored_fallback() -> void:
	var runner := _make_runner()
	runner.live_mind = false
	runner.start(_woven_dialogue(), null)
	assert_eq(runner.requested, 0, "no mind was asked")
	assert_has(_lines, "Ask me another day.", "the authored fallback line played")
	assert_true(_ended, "the authored goto closed the scene")
	assert_true(GameState.weaves.is_empty(), "nothing was persisted offline")


func test_proposal_is_clamped_persisted_and_played() -> void:
	var runner := _make_runner()
	runner.start(_woven_dialogue(), null)
	assert_eq(runner.requested, 1, "the mind was asked once")
	runner._on_soul_spoke(PROPOSAL_JSON)
	assert_has(_lines, "Third shift, the picket thins.", "the woven line played")
	assert_eq(_choice_batches.size(), 1, "the proposed choices went up")
	assert_eq(_choice_batches[0].size(), 2)
	var persisted: Dictionary = GameState.weave_for("test_weave/rumor")
	assert_false(persisted.is_empty(), "the resolution persisted in the save block")
	assert_eq(float(persisted.mutations[0].amount), 3.0,
		"the 40-point grab was clamped to the granted 3 before persisting")
	var standing := GameState.faction_standing("reach_compact")
	assert_eq(int(standing.trust), 3, "the world moved by the clamp, not the proposal")


func test_prose_wrapped_json_still_parses() -> void:
	var runner := _make_runner()
	runner.start(_woven_dialogue(), null)
	runner._on_soul_spoke("Right then. " + PROPOSAL_JSON + " That's all.")
	assert_has(_lines, "Third shift, the picket thins.")


func test_unparseable_proposal_falls_back_to_authored() -> void:
	var runner := _make_runner()
	runner.start(_woven_dialogue(), null)
	runner._on_soul_spoke("The picket thins on third shift, friend.")
	assert_has(_lines, "Ask me another day.", "no JSON means the authored fallback")
	assert_true(GameState.weaves.is_empty(), "a discarded proposal is not persisted")


func test_replay_consumes_the_persisted_resolution_without_generating() -> void:
	var first := _make_runner()
	first.start(_woven_dialogue(), null)
	first._on_soul_spoke(PROPOSAL_JSON)
	var second := _make_runner()
	second.start(_woven_dialogue(), null)
	assert_eq(second.requested, 0, "replays never re-generate")
	assert_has(_lines, "Third shift, the picket thins.", "the persisted line replayed")
	assert_eq(_choice_batches.size(), 1, "the persisted choices replayed")


func test_resolved_choice_applies_its_mutations_and_rejoins_the_spine() -> void:
	var runner := _make_runner()
	runner.start(_woven_dialogue(), null)
	runner._on_soul_spoke(PROPOSAL_JSON)
	runner.choose(1)  # "I owe you one."
	assert_true(GameState.has_flag("heard_cordon_rumor"),
		"the granted flag landed via the ordinary choose() path")
	assert_has(_lines, "Anyway.", "the branch rejoined at return_to")


func test_buffer_line_masks_the_weave_latency() -> void:
	var runner := _make_runner()
	var dialogue := _woven_dialogue()
	dialogue.nodes.rumor["buffer_line"] = "He turns the glass one full turn."
	runner.start(dialogue, null)
	assert_has(_lines, "He turns the glass one full turn.", "the buffer landed instantly")
	assert_eq(_choice_batches.size(), 0, "nothing offered while the mind works")
	runner._on_soul_spoke(PROPOSAL_JSON)
	assert_has(_lines, "Third shift, the picket thins.", "the woven beat followed")
	assert_eq(_choice_batches.size(), 1)
