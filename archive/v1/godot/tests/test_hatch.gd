extends GutTest
## Contract tests for the hatch: settings apply outside the save, and the
## pause menu honors the Esc-vs-panel ordering (a conversation on screen
## owns Esc; the menu waits its turn).


func after_each() -> void:
	get_tree().paused = false
	Settings.set_value("typewriter_cps", 55.0)
	Settings.set_value("master_volume", 1.0)


func test_settings_apply_to_the_master_bus() -> void:
	Settings.set_value("master_volume", 0.5)
	assert_almost_eq(AudioServer.get_bus_volume_db(0), linear_to_db(0.5), 0.01)
	Settings.set_value("master_volume", 0.0)
	assert_true(AudioServer.is_bus_mute(0), "zero volume mutes instead of -inf")
	Settings.set_value("master_volume", 1.0)
	assert_false(AudioServer.is_bus_mute(0))


func test_settings_emit_change_once_per_real_change() -> void:
	var changes: Array = [0]
	var handler := func() -> void: changes[0] += 1
	Settings.changed.connect(handler)
	Settings.set_value("typewriter_cps", 80.0)
	Settings.set_value("typewriter_cps", 80.0)  # no-op: same value
	Settings.set_value("nonexistent_key", 1)    # no-op: unknown key
	Settings.changed.disconnect(handler)
	assert_eq(changes[0], 1)


func test_escape_defers_to_an_open_dialogue_panel() -> void:
	var pause := PauseMenu.new()
	add_child_autofree(pause)
	var panel := DialoguePanel.new()
	add_child_autofree(panel)
	panel.open("Grissom", "scripted")
	var esc := InputEventKey.new()
	esc.keycode = KEY_ESCAPE
	esc.pressed = true
	pause._unhandled_input(esc)
	assert_false(pause.is_paused(), "the panel owns the moment")
	panel.close()
	pause._unhandled_input(esc)
	assert_true(pause.is_paused(), "with the panel closed, Esc pauses")
	pause.toggle()
	assert_false(pause.is_paused())
	assert_false(get_tree().paused)
