extends GutTest
## Contract test: voice through the DialoguePanel (EAR-PROTOCOL.md, "what
## the host does with a transcript"). No mic, no daemon — transcripts are
## delivered straight to the handler, because voice is an input method and
## the panel cannot tell the difference. That's the property under test.

var _picked: Array = []
var _free: Array = []


func _make_panel() -> DialoguePanel:
	var panel := DialoguePanel.new()
	add_child_autofree(panel)
	_picked = []
	_free = []
	panel.choice_picked.connect(func(index: int) -> void: _picked.append(index))
	panel.free_speech.connect(func(text: String) -> void: _free.append(text))
	return panel


func _open_with_choices(panel: DialoguePanel) -> void:
	panel.open("Grissom", "linked")
	panel.show_choices([
		{"index": 0, "text": "I'd do it again — she's crew."},
		{"index": 1, "text": "It wasn't my business."},
		{"index": 2, "text": "Buy him a drink."},
	])


func test_matching_transcript_fires_the_choice_exactly_as_clicked() -> void:
	var panel := _make_panel()
	_open_with_choices(panel)
	panel._on_transcript("I'd do it again. She's crew.", 0.9)
	assert_eq(_picked, [0], "the matched choice fired with its runner index")
	assert_eq(_free.size(), 0, "no free-speech leak on a match")


func test_unmatched_transcript_becomes_free_speech_and_choices_stay_up() -> void:
	var panel := _make_panel()
	_open_with_choices(panel)
	panel._on_transcript("what happened to the cargo we lost near the belt", 0.9)
	assert_eq(_picked.size(), 0, "nothing fired")
	assert_eq(_free, ["what happened to the cargo we lost near the belt"])


func test_silence_is_not_input() -> void:
	var panel := _make_panel()
	_open_with_choices(panel)
	panel._on_transcript("   ", 0.0)
	assert_eq(_picked.size(), 0)
	assert_eq(_free.size(), 0)


func test_closed_panel_ignores_late_transcripts() -> void:
	var panel := _make_panel()
	_open_with_choices(panel)
	panel.close()
	panel._on_transcript("buy him a drink", 0.9)
	assert_eq(_picked.size(), 0)
	assert_eq(_free.size(), 0)


func test_ambiguous_transcript_is_free_speech_not_a_guess() -> void:
	var panel := _make_panel()
	panel.open("Picket", "linked")
	panel.show_choices([
		{"index": 0, "text": "Take the deal."},
		{"index": 1, "text": "Walk away."},
	])
	panel._on_transcript("take the deal and walk away", 0.9)
	assert_eq(_picked.size(), 0, "ambiguity never guesses")
	assert_eq(_free.size(), 1)


func test_listening_outranks_composing_on_the_lamp() -> void:
	var panel := _make_panel()
	panel.open("Grissom", "linked")
	panel.set_thinking(true)
	panel._on_listening_changed(true)
	assert_eq(panel.lamp_state(), "listening")
	panel._on_listening_changed(false)
	assert_eq(panel.lamp_state(), "composing")
