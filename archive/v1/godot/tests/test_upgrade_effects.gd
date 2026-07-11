extends GutTest
## Contract tests: upgrades change the game (Sprint 3). Every effect key the
## engine reads has a live consumer; this suite pins the aggregation math
## and the savvy price break at the counter.

const UpgradeShopScript := preload("res://scripts/framework/upgrade_shop.gd")


func before_each() -> void:
	GameState.player["character"] = ""
	GameState.player["upgrades"] = []


func after_each() -> void:
	before_each()


func test_turn_mult_aggregates_multiplicatively() -> void:
	assert_almost_eq(GameState.upgrade_effect_product("turn_mult"), 1.0, 0.001)
	var found := ""
	for upgrade_id in DataRegistry.ids("upgrades"):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		if float(effects.get("turn_mult", 1.0)) > 1.0:
			found = upgrade_id
			break
	assert_ne(found, "", "the demo content ships a turn upgrade")
	GameState.player.upgrades.append(found)
	assert_gt(GameState.upgrade_effect_product("turn_mult"), 1.0)


func test_repair_speed_mult_has_a_seller() -> void:
	var found := ""
	for upgrade_id in DataRegistry.ids("upgrades"):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		if float(effects.get("repair_speed_mult", 1.0)) > 1.0:
			found = upgrade_id
			break
	assert_ne(found, "", "the demo content ships a welder")
	GameState.player.upgrades.append(found)
	assert_gt(GameState.upgrade_effect_product("repair_speed_mult"), 1.0)


func test_auto_suppress_is_a_boolean_effect() -> void:
	assert_false(GameState.upgrade_effect_bool("auto_suppress"))
	var found := ""
	for upgrade_id in DataRegistry.ids("upgrades"):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		if bool(effects.get("auto_suppress", false)):
			found = upgrade_id
			break
	assert_ne(found, "", "the demo content ships a suppression net")
	GameState.player.upgrades.append(found)
	assert_true(GameState.upgrade_effect_bool("auto_suppress"))


func test_savvy_earns_a_price_break_at_the_counter() -> void:
	var shop: UpgradeShop = UpgradeShopScript.new()
	add_child_autofree(shop)
	shop.configure()
	var any_upgrade := DataRegistry.get_entity("upgrades", DataRegistry.ids("upgrades")[0])
	var sticker := shop._price(any_upgrade)
	# Find a high-savvy playable character.
	var haggler := ""
	for npc_id in DataRegistry.ids("npcs"):
		var playable: Dictionary = DataRegistry.get_entity("npcs", npc_id).get("playable", {})
		if int(playable.get("stats", {}).get("savvy", 0)) >= 4:
			haggler = npc_id
			break
	assert_ne(haggler, "", "the demo content ships a haggler")
	GameState.set_player_character(haggler)
	assert_lt(shop._price(any_upgrade), sticker, "savvy talks the price down")
