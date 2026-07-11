extends GutTest
## Contract tests: the DialoguePanel (Sprint 3) — the isolated conversation
## surface. Typewriter reveals text over time (never all at once), lines
## queue in order, fast-forward completes the beat, the mind-status lamp
## tells the player what kind of wait a conversation is, and narration
## cards stand in for scenes the player IS the speaker of.

const DialoguePanelScript := preload("res://scripts/framework/dialogue_panel.gd")


var _panel: DialoguePanel


func before_each() -> void:
	_panel = DialoguePanelScript.new()
	add_child_autofree(_panel)


func test_closed_by_default() -> void:
	assert_false(_panel.is_open())
	assert_false(_panel.visible)


func test_lines_type_out_not_all_at_once() -> void:
	_panel.open("Tester", "scripted")
	_panel.show_line("Tester", "A line long enough that one frame cannot possibly finish it.")
	assert_true(_panel.is_typing(), "the SNES cadence: characters land one by one")


func test_fast_forward_completes_the_line() -> void:
	_panel.open("Tester", "scripted")
	_panel.show_line("Tester", "A line long enough that one frame cannot possibly finish it.")
	_panel.fast_forward()
	assert_false(_panel.is_typing())


func test_lamp_reports_the_kind_of_wait() -> void:
	_panel.open("Tester", "linked")
	assert_eq(_panel.lamp_state(), "linked", "a live mind is announced up front")
	_panel.set_thinking(true)
	assert_eq(_panel.lamp_state(), "composing", "and pulses while it works")
	_panel.set_thinking(false)
	assert_eq(_panel.lamp_state(), "linked")


func test_offline_lamp() -> void:
	_panel.open("Tester", "offline")
	assert_eq(_panel.lamp_state(), "offline")


func test_close_clears_the_surface() -> void:
	_panel.open("Tester", "linked")
	_panel.show_line("Tester", "Words.")
	_panel.close()
	assert_false(_panel.is_open())
	assert_false(_panel.visible)


func test_narration_card_opens_and_reports_done() -> void:
	var done := [false]
	_panel.narration_done.connect(func() -> void: done[0] = true)
	_panel.show_narration("You", "You do the thing yourself, because you are the thing.")
	assert_true(_panel.is_open())
	assert_true(_panel.visible)
	# The card types out; finishing it shows the continue prompt, and the
	# host is only released when the player dismisses it (input path) —
	# here we only contract that showing it does NOT auto-finish.
	assert_false(done[0])


func test_bark_shows_without_a_conversation() -> void:
	_panel.bark("Boris", "Logged.")
	assert_true(_panel.visible, "transient barks use the same surface")
	assert_false(_panel.is_open(), "but do not claim the conversation")
