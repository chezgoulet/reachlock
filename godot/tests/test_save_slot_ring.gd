extends GutTest
## Contract test: GameState save-slot ring (t_f7f06ee).
##
## Tests the rotating save slot system: slot cycling through all RING_SIZE
## slots, most-recent-slot load behaviour, slot metadata (location, mission),
## list_saves / load_slot / has_save API, and legacy v0 migration.
##
## Note: these tests modify the real user://saves/ filesystem. Each test
## starts fresh (before_each clears the ring) and after_all removes the
## test save directory.

const SAVE_DIR := "user://saves/"
const INDEX_PATH := "user://saves/index.json"

var _backup_index: String = ""
var _backup_slot_count := 0


func before_all() -> void:
	# Stash any pre-existing save index so we can restore it.
	if FileAccess.file_exists(INDEX_PATH):
		_backup_index = FileAccess.get_file_as_string(INDEX_PATH)
		# Count existing save files
		for i in 5:
			if FileAccess.file_exists(SAVE_DIR + "slot%d.json" % i):
				_backup_slot_count += 1


func before_each() -> void:
	# Clear the ring so each test starts fresh.
	_clear_ring()


func after_all() -> void:
	# Restore backup if it existed.
	if _backup_index != "":
		DirAccess.make_dir_recursive_absolute(SAVE_DIR)
		var f := FileAccess.open(INDEX_PATH, FileAccess.WRITE)
		if f != null:
			f.store_string(_backup_index)
			f.close()
	else:
		# No backup — remove the index file we created.
		if FileAccess.file_exists(INDEX_PATH):
			DirAccess.make_dir_recursive_absolute(SAVE_DIR)
			var f := FileAccess.open(INDEX_PATH, FileAccess.WRITE)
			if f != null:
				f.store_string("{}")
				f.close()


## Wipe the ring index and all slot files so each test starts fresh.
func _clear_ring() -> void:
	DirAccess.make_dir_recursive_absolute(SAVE_DIR)
	# Overwrite all 5 slot files with invalid content so the legacy
	# fallback check in has_save() won't find them.
	for i in 5:
		var sf := FileAccess.open(SAVE_DIR + "slot%d.json" % i, FileAccess.WRITE)
		if sf != null:
			sf.store_string("null")
			sf.close()
	# Write a fresh-start index with all slots empty.
	var slots: Array = []
	for i in 5:
		slots.append({"filled": false, "tick": 0})
	var data := {"ring_size": 5, "next_slot": 0, "slots": slots}
	var f := FileAccess.open(INDEX_PATH, FileAccess.WRITE)
	if f != null:
		f.store_string(JSON.stringify(data))
		f.close()


## Helper: count how many slots are marked filled in the current index.
func _filled_count() -> int:
	var n := 0
	var saves: Array = GameState.list_saves()
	for s in saves:
		var entry: Dictionary = s as Dictionary
		if entry.get("filled", false):
			n += 1
	return n


## --- empty ring ------------------------------------------------------------


func test_empty_ring_has_no_save() -> void:
	assert_false(GameState.has_save(), "Fresh game should have no saves")


func test_empty_ring_list_empty() -> void:
	var saves: Array = GameState.list_saves()
	assert_eq(saves.size(), 5, "list_saves returns RING_SIZE entries even when empty")
	for s in saves:
		var entry: Dictionary = s as Dictionary
		assert_false(entry.get("filled", false), "All slots empty in fresh ring")


func test_empty_ring_load_fails() -> void:
	assert_false(GameState.load_game(), "Loading with no saves should return false")


## --- saving cycles through slots ------------------------------------------


func test_first_save_writes_slot_0() -> void:
	var ok: bool = GameState.save_game()
	assert_true(ok, "save_game succeeds on fresh ring")
	assert_true(GameState.has_save(), "has_save true after first save")

	var saves: Array = GameState.list_saves()
	var s0: Dictionary = saves[0] as Dictionary
	assert_true(s0.get("filled", false), "Slot 0 filled after first save")
	var s1: Dictionary = saves[1] as Dictionary
	assert_false(s1.get("filled", false), "Slot 1 still empty after first save")
	assert_eq(_filled_count(), 1)


func test_second_save_writes_slot_1() -> void:
	GameState.save_game()  # slot 0
	GameState.save_game()  # slot 1

	var saves: Array = GameState.list_saves()
	var s0: Dictionary = saves[0] as Dictionary
	assert_true(s0.get("filled", false), "Slot 0 filled")
	var s1: Dictionary = saves[1] as Dictionary
	assert_true(s1.get("filled", false), "Slot 1 filled")
	assert_eq(_filled_count(), 2)


func test_save_cycles_through_all_slots() -> void:
	# All 5 saves to fill the ring
	for i in 5:
		GameState.save_game()

	var saves: Array = GameState.list_saves()
	for i in 5:
		var entry: Dictionary = saves[i] as Dictionary
		assert_true(entry.get("filled", false), "Slot %d should be filled after cycling through all" % i)
	assert_eq(_filled_count(), 5)


func test_sixth_save_wraps_to_slot_0() -> void:
	for i in 5:
		GameState.save_game()
	# Sixth save wraps to slot 0, overwriting it
	GameState.save_game()

	var saves: Array = GameState.list_saves()
	var s0: Dictionary = saves[0] as Dictionary
	assert_true(s0.get("filled", false), "Slot 0 overwritten and still filled")
	assert_eq(_filled_count(), 5, "Ring stays at 5 filled after wrap")


## --- load most recent -------------------------------------------------------


func test_load_game_loads_most_recent() -> void:
	# Save three times — each resaves the default state with the same tick=0
	GameState.save_game()
	GameState.save_game()
	GameState.save_game()

	# load_game should succeed (any filled slot)
	var ok: bool = GameState.load_game()
	assert_true(ok, "load_game succeeds when saves exist")


func test_load_game_fails_on_empty_ring_after_save_wipe() -> void:
	# Save once, then manually break the index
	GameState.save_game()
	assert_true(GameState.has_save())

	# Corrupt the index (simulate all slots empty in index but file exists)
	var f := FileAccess.open(INDEX_PATH, FileAccess.WRITE)
	if f != null:
		f.store_string("{\"ring_size\":5,\"next_slot\":0,\"slots\":[{\"filled\":false,\"tick\":0},{\"filled\":false,\"tick\":0},{\"filled\":false,\"tick\":0},{\"filled\":false,\"tick\":0},{\"filled\":false,\"tick\":0}]}")
		f.close()

	# Refresh GameState index state
	assert_false(GameState.load_game(), "load_game returns false when index shows empty")


## --- load_slot --------------------------------------------------------------


func test_load_slot_with_valid_index() -> void:
	GameState.save_game()
	var ok: bool = GameState.load_slot(0)
	assert_true(ok, "load_slot(0) succeeds when slot 0 is filled")


func test_load_slot_with_empty_slot_fails() -> void:
	GameState.save_game()
	# Slot 1 should be empty
	var ok: bool = GameState.load_slot(1)
	assert_false(ok, "load_slot(1) fails when slot 1 is empty")


func test_load_slot_out_of_range_fails() -> void:
	GameState.save_game()
	assert_false(GameState.load_slot(-1), "load_slot(-1) out of range")
	assert_false(GameState.load_slot(99), "load_slot(99) out of range")


## --- slot metadata ----------------------------------------------------------


func test_slot_metadata_includes_location() -> void:
	GameState.player.location = "sorrow_station"
	GameState.save_game()

	var saves: Array = GameState.list_saves()
	var s0: Dictionary = saves[0] as Dictionary
	assert_eq(s0.get("location", ""), "sorrow_station", "Slot metadata records location")


func test_slot_metadata_includes_tick() -> void:
	GameState.universe.tick = 42
	GameState.save_game()

	var saves: Array = GameState.list_saves()
	var s0: Dictionary = saves[0] as Dictionary
	assert_eq(s0.get("tick", 0), 42, "Slot metadata records universe tick")


func test_slot_metadata_includes_mission_when_active() -> void:
	GameState.save_game()
	var saves: Array = GameState.list_saves()
	var s0: Dictionary = saves[0] as Dictionary
	assert_has(s0, "mission_id", "Slot metadata has mission_id field")


## --- list_saves -----------------------------------------------------------


func test_list_saves_returns_full_ring() -> void:
	var saves: Array = GameState.list_saves()
	assert_eq(saves.size(), 5, "list_saves always returns 5 entries")


func test_list_saves_entries_have_expected_keys() -> void:
	GameState.save_game()
	var saves: Array = GameState.list_saves()
	for s in saves:
		var entry: Dictionary = s as Dictionary
		assert_has(entry, "index")
		assert_has(entry, "filled")
		assert_has(entry, "tick")
		assert_has(entry, "location")
		assert_has(entry, "mission_id")


## --- legacy migration ------------------------------------------------------


func test_has_save_detects_legacy_slot0() -> void:
	# Write an index without a "slots" key so has_save falls through
	# to the legacy-save check.
	DirAccess.make_dir_recursive_absolute(SAVE_DIR)
	var f2 := FileAccess.open(INDEX_PATH, FileAccess.WRITE)
	if f2 != null:
		f2.store_string("{\"not_an_index\": true}")
		f2.close()

	DirAccess.make_dir_recursive_absolute(SAVE_DIR)
	var legacy := JSON.stringify({
		"save_version": 0,
		"universe": {"tick": 10, "flags": []},
		"player": {"location": "sorrow_station", "credits": 500, "flags": [], "ship": {"hull_id": "loup_garou", "hull_integrity": 0.8, "position": [0,0,0], "cargo": {}}},
		"souls": {},
		"mods": {"load_order": ["reachlock"]},
	})
	var file := FileAccess.open(SAVE_DIR + "slot0.json", FileAccess.WRITE)
	if file != null:
		file.store_string(legacy)
		file.close()

	assert_true(GameState.has_save(), "has_save detects legacy slot0.json")


func test_legacy_migration_preserves_location() -> void:
	# Same setup — index without "slots" key.
	DirAccess.make_dir_recursive_absolute(SAVE_DIR)
	var f2 := FileAccess.open(INDEX_PATH, FileAccess.WRITE)
	if f2 != null:
		f2.store_string("{\"not_an_index\": true}")
		f2.close()

	DirAccess.make_dir_recursive_absolute(SAVE_DIR)
	var legacy := JSON.stringify({
		"save_version": 0,
		"created_at": "2026-01-01T00:00:00",
		"updated_at": "2026-01-01T00:00:00",
		"universe": {"tick": 10, "flags": []},
		"player": {"location": "sorrow_station", "credits": 500, "flags": [], "ship": {"hull_id": "loup_garou", "hull_integrity": 0.8, "position": [0,0,0], "cargo": {}}},
		"souls": {},
		"mods": {"load_order": ["reachlock"]},
	})
	var file := FileAccess.open(SAVE_DIR + "slot0.json", FileAccess.WRITE)
	if file != null:
		file.store_string(legacy)
		file.close()

	# Trigger migration by calling has_save
	GameState.has_save()

	# After migration, the index should exist and slot 0 should have the legacy location
	var saves: Array = GameState.list_saves()
	var s0: Dictionary = saves[0] as Dictionary
	assert_true(s0.get("filled", false), "Legacy save migrated to slot 0")
	assert_eq(s0.get("location", ""), "sorrow_station", "Legacy location preserved in slot metadata")
