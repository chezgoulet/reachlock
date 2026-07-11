extends GutTest
## Contract tests: interior ship damage (Sprint 3) — combat writes entries
## into GameState.player.ship.damage, they degrade flight until repaired,
## engineering trims the penalty, and the gunnery calibration is a one-shot.


func before_each() -> void:
	GameState.player["character"] = ""
	GameState.player["upgrades"] = []
	GameState.player.ship["damage"] = []
	GameState.player.ship["damage_seq"] = 0
	GameState.player.ship["weapons_calibrated"] = false


func after_each() -> void:
	before_each()


func test_damage_entries_get_unique_ids() -> void:
	var a := GameState.add_ship_damage("galley", "fire", [300.0, 250.0])
	var b := GameState.add_ship_damage("engineering", "conduit", [200.0, 420.0])
	assert_ne(int(a.id), int(b.id))
	assert_eq(GameState.ship_damage().size(), 2)


func test_repair_removes_the_entry() -> void:
	var entry := GameState.add_ship_damage("galley", "fire", [300.0, 250.0])
	GameState.repair_ship_damage(int(entry.id))
	assert_eq(GameState.ship_damage().size(), 0)


func test_pristine_ship_flies_at_spec() -> void:
	var penalty := GameState.flight_damage_penalty()
	assert_almost_eq(float(penalty.speed_mult), 1.0, 0.001)
	assert_almost_eq(float(penalty.cooldown_mult), 1.0, 0.001)
	assert_almost_eq(float(penalty.vulnerability), 1.0, 0.001)


func test_damage_bleeds_speed_and_guns() -> void:
	GameState.add_ship_damage("galley", "fire", [300.0, 250.0], 1.0)
	GameState.add_ship_damage("bridge", "conduit", [200.0, 80.0], 1.0)
	var penalty := GameState.flight_damage_penalty()
	assert_lt(float(penalty.speed_mult), 1.0, "a wounded ship is slower")
	assert_gt(float(penalty.cooldown_mult), 1.0, "a wounded ship shoots slower")
	assert_gt(float(penalty.vulnerability), 1.0, "a wounded ship takes hits worse")


func test_a_good_engineer_trims_the_bleeding() -> void:
	GameState.add_ship_damage("galley", "fire", [300.0, 250.0], 1.0)
	GameState.add_ship_damage("bridge", "conduit", [200.0, 80.0], 1.0)
	var stock := float(GameState.flight_damage_penalty().speed_mult)
	# Find a playable engineer (engineering stat above the even keel).
	var engineer := ""
	for npc_id in DataRegistry.ids("npcs"):
		var playable: Dictionary = DataRegistry.get_entity("npcs", npc_id).get("playable", {})
		if int(playable.get("stats", {}).get("engineering", 0)) >= 4:
			engineer = npc_id
			break
	assert_ne(engineer, "", "the demo content ships an engineer")
	GameState.set_player_character(engineer)
	assert_gt(float(GameState.flight_damage_penalty().speed_mult), stock,
		"the same damage costs less under a real engineer")


func test_weapons_calibration_is_spent_by_one_flight() -> void:
	GameState.set_weapons_calibrated(true)
	assert_true(GameState.consume_weapons_calibration(), "the first flight gets the edge")
	assert_false(GameState.consume_weapons_calibration(), "and spends it")


func test_pre_sprint_saves_default_clean() -> void:
	# A save from before the damage block existed: the accessor lazily
	# defaults it instead of crashing (same policy as power/current_space).
	GameState.player.ship.erase("damage")
	assert_eq(GameState.ship_damage().size(), 0)
	assert_true(GameState.player.ship.has("damage"), "the default persists in place")
