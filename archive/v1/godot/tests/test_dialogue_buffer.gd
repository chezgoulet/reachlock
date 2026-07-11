extends GutTest
## Contract test: the hybrid dialogue latency system (Sprint 2 hard
## requirement). A generated node with a buffer_line speaks instantly and
## never stalls the conversation; the mind's line lands as a follow-up
## beat or is dropped once the player moves on.

## A runner with the mind stubbed out: generation is "available" but goes
## nowhere until the test delivers a line by hand.
class StubRunner extends DialogueRunner:
	var requested := 0

	func _can_generate() -> bool:
		return true

	func _request_generation(_node: Dictionary) -> void:
		requested += 1


var _lines: Array = []
var _choice_batches: Array = []
var _thinking_states: Array = []


func _make_runner() -> StubRunner:
	var runner := StubRunner.new()
	add_child_autofree(runner)
	_lines = []
	_choice_batches = []
	_thinking_states = []
	runner.line_shown.connect(func(speaker: String, text: String) -> void:
		_lines.append({"speaker": speaker, "text": text}))
	runner.choices_shown.connect(func(choices: Array) -> void:
		_choice_batches.append(choices))
	runner.thinking_changed.connect(func(thinking: bool) -> void:
		_thinking_states.append(thinking))
	return runner


func _dialogue_with(nodes: Dictionary) -> Dictionary:
	return {"id": "test_buffer", "npc": "test_npc", "entry": "gen", "nodes": nodes}


func test_buffered_node_with_choices_speaks_and_offers_instantly() -> void:
	var runner := _make_runner()
	runner.start(_dialogue_with({
		"gen": {"kind": "generated", "prompt_hint": "say something",
			"buffer_line": "Let me think how to put that.",
			"choices": [{"text": "Take your time.", "goto": "end"}]},
	}), null)
	assert_eq(runner.requested, 1, "the mind was asked")
	assert_eq(_lines.size(), 1, "the buffer landed immediately")
	assert_eq(_lines[0].text, "Let me think how to put that.")
	assert_eq(_choice_batches.size(), 1, "choices are up — no stall")


func test_late_line_lands_as_followup_without_reoffering() -> void:
	var runner := _make_runner()
	runner.start(_dialogue_with({
		"gen": {"kind": "generated", "prompt_hint": "x", "buffer_line": "Hm.",
			"choices": [{"text": "ok", "goto": "end"}]},
	}), null)
	runner._on_soul_spoke("Here is the considered answer.")
	assert_eq(_lines.size(), 2, "the mind's line followed the buffer")
	assert_eq(_lines[1].text, "Here is the considered answer.")
	assert_eq(_choice_batches.size(), 1, "choices were not re-offered")


func test_moving_on_drops_the_late_line() -> void:
	var runner := _make_runner()
	var ended := [false]
	runner.ended.connect(func() -> void: ended[0] = true)
	runner.start(_dialogue_with({
		"gen": {"kind": "generated", "prompt_hint": "x", "buffer_line": "Hm.",
			"choices": [{"text": "Never mind.", "goto": "end"}]},
	}), null)
	runner.choose(0)
	var lines_after_choice := _lines.size()
	runner._on_soul_spoke("A line from a moment that already passed.")
	assert_eq(_lines.size(), lines_after_choice, "the stale line was dropped")
	assert_true(ended[0], "conversation closed cleanly")


func test_choiceless_buffer_bridges_to_the_real_line() -> void:
	var runner := _make_runner()
	runner.start(_dialogue_with({
		"gen": {"kind": "generated", "prompt_hint": "x",
			"buffer_line": "One moment.", "goto": "after"},
		"after": {"kind": "authored", "text": "And here we are.", "goto": "end"},
	}), null)
	assert_eq(_lines.size(), 1, "buffer holds the beat")
	assert_true(_thinking_states.size() >= 1 and _thinking_states[0] == true,
		"the thinking indicator is on while bridging")
	runner._on_soul_spoke("The real answer.")
	assert_eq(_lines.size(), 3, "real line then the next authored node")
	assert_eq(_lines[1].text, "The real answer.")
	assert_eq(_lines[2].text, "And here we are.")
	assert_false(_thinking_states[_thinking_states.size() - 1], "indicator off")


func test_unbuffered_node_shows_thinking_indicator() -> void:
	var runner := _make_runner()
	runner.start(_dialogue_with({
		"gen": {"kind": "generated", "prompt_hint": "x", "text": "fallback", "goto": "end"},
	}), null)
	assert_eq(_lines.size(), 0, "nothing spoken yet — the mind is working")
	assert_true(_thinking_states.size() >= 1 and _thinking_states[0] == true)
	runner._on_soul_spoke("Done thinking.")
	assert_eq(_lines[0].text, "Done thinking.")


func test_offline_generated_node_falls_back_instantly() -> void:
	# A plain DialogueRunner with no soul and no daemon: the buffer is
	# irrelevant, the authored fallback text carries the beat.
	var runner := DialogueRunner.new()
	add_child_autofree(runner)
	var lines: Array = []
	runner.line_shown.connect(func(_s: String, text: String) -> void: lines.append(text))
	runner.start(_dialogue_with({
		"gen": {"kind": "generated", "prompt_hint": "x", "buffer_line": "Hm.",
			"text": "The offline line.", "goto": "end"},
	}), null)
	assert_eq(lines.size(), 1)
	assert_eq(lines[0], "The offline line.")


func test_speaker_names_voice_nodes_through_bystanders() -> void:
	var runner := _make_runner()
	var dialogue := _dialogue_with({
		"gen": {"kind": "authored", "text": "Oi!", "goto": "end"},
	})
	dialogue["speaker_names"] = {"gen": "Heckler"}
	runner.start(dialogue, null)
	assert_eq(_lines[0].speaker, "Heckler")
