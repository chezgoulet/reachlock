extends GutTest
## Contract tests: the playable-crew layer (Sprint 3) — GameState.player_stat
## reads the chosen character's playable block, upgrades ride on top via
## stat_<name> effects, and the character-select roster is driven by data.

const CharacterSelectScript := preload("res://scripts/framework/character_select.gd")


func before_each() -> void:
	GameState.player["character"] = ""
	GameState.player["upgrades"] = []


func after_each() -> void:
	GameState.player["character"] = ""
	GameState.player["upgrades"] = []


func test_unnamed_captain_reads_even_stats() -> void:
	assert_eq(GameState.player_stat("piloting"), 2)
	assert_eq(GameState.player_stat("grit"), 2)


func test_chosen_character_stats_come_from_playable_data() -> void:
	# Data-driven: find any playable crew member with an uneven spread.
	var found := ""
	for npc_id in DataRegistry.ids("npcs"):
		var playable: Dictionary = DataRegistry.get_entity("npcs", npc_id).get("playable", {})
		if not playable.get("stats", {}).is_empty():
			found = npc_id
			break
	assert_ne(found, "", "the demo content ships playable crew")
	GameState.set_player_character(found)
	var stats: Dictionary = DataRegistry.get_entity("npcs", found).get("playable", {}).get("stats", {})
	for stat: String in stats:
		assert_eq(GameState.player_stat(stat), int(stats[stat]),
			"stat %s reads the playable block" % stat)


func test_upgrade_stat_riders_add_on_top() -> void:
	var base := GameState.player_stat("piloting")
	# Find an upgrade carrying a stat_piloting effect (upgrade contract).
	var found := ""
	for upgrade_id in DataRegistry.ids("upgrades"):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		if float(effects.get("stat_piloting", 0.0)) > 0.0:
			found = upgrade_id
			break
	assert_ne(found, "", "the demo content ships a piloting trainer")
	GameState.player.upgrades.append(found)
	assert_gt(GameState.player_stat("piloting"), base, "the sim module trains reflexes")


func test_character_rides_the_trigger_dsl_context() -> void:
	GameState.set_player_character("test_character_id")
	assert_eq(str(GameState.context().player.character), "test_character_id")


func test_character_select_roster_is_playable_crew() -> void:
	var select: Control = CharacterSelectScript.new()
	add_child_autofree(select)
	var expected := 0
	var hull_id: String = GameState.player.ship.hull_id
	for npc_id in DataRegistry.ids("npcs"):
		var npc := DataRegistry.get_entity("npcs", npc_id)
		if npc.has("playable") and npc.get("ship", "") == hull_id and npc.get("aboard", false):
			expected += 1
	assert_eq(select.roster_size(), expected, "every playable crew member is a seat")
	assert_gt(select.roster_size(), 0)
	assert_ne(select.selected_id(), "", "something is selected from the start")


func test_magnetic_boolean_upgrade_effect() -> void:
	assert_false(GameState.upgrade_effect_bool("magnetic_soles"))
	var found := ""
	for upgrade_id in DataRegistry.ids("upgrades"):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		if bool(effects.get("magnetic_soles", false)):
			found = upgrade_id
			break
	assert_ne(found, "", "the demo content ships mag boots")
	GameState.player.upgrades.append(found)
	assert_true(GameState.upgrade_effect_bool("magnetic_soles"))
