extends GutTest
func test_minimal() -> void:
	assert_true(true, "basic test works")

func test_call_list_saves() -> void:
	var saves: Array = GameState.list_saves()
	assert_eq(saves.size(), 5, "list_saves returns 5 entries from the compiler")

func test_call_save() -> void:
	var ok: bool = GameState.save_game()
	assert_true(ok, "save works")

func test_has_save_after_save() -> void:
	GameState.save_game()
	assert_true(GameState.has_save(), "has_save returns true after saving")

func test_load_slot_exists() -> void:
	GameState.save_game()
	var ok: bool = GameState.load_slot(0)
	assert_true(ok, "load_slot(0) returns true after saving")
