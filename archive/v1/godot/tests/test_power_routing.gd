extends GutTest
## Contract test: the engineering power grid (GameState.power_share /
## set_power_share) and its promise that the budget always renormalizes.


func before_each() -> void:
	GameState.player.ship["power"] = {"weapons": 0.33, "shields": 0.33, "engines": 0.34}


func _total() -> float:
	var power: Dictionary = GameState.player.ship.power
	var total := 0.0
	for key: String in power:
		total += float(power[key])
	return total


func test_default_split_is_even() -> void:
	assert_almost_eq(GameState.power_share("weapons"), 0.33, 0.001)
	assert_almost_eq(GameState.power_share("engines"), 0.34, 0.001)


func test_raising_one_channel_drains_the_others() -> void:
	GameState.set_power_share("engines", 0.6)
	assert_almost_eq(GameState.power_share("engines"), 0.6, 0.001)
	assert_almost_eq(_total(), 1.0, 0.005, "the budget stays 1.0")
	assert_lt(GameState.power_share("weapons"), 0.33, "weapons paid for it")


func test_channel_clamps_below_full_budget() -> void:
	GameState.set_power_share("weapons", 5.0)
	assert_almost_eq(GameState.power_share("weapons"), 0.9, 0.001, "0.9 cap — nothing runs dark")
	assert_almost_eq(_total(), 1.0, 0.005)


func test_allocation_lives_in_the_save_block() -> void:
	GameState.set_power_share("shields", 0.5)
	assert_almost_eq(float(GameState.player.ship.power.shields), 0.5, 0.001,
		"the grid writes to the persisted ship block, not transient state")
