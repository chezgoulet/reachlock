extends GutTest
## Contract test: ShipOperation autoload (Sprint 03, P9).
##
## Tests the occupancy API, control API, effects API, and reset behavior.
## Every test runs against a fresh ShipOperation instance.

const STATION_IDS := ["pilot", "weapons", "engineering", "scanner", "cargo"]


func before_each() -> void:
	# Reset ShipOperation to a clean state for each test
	ShipOperation.reset()
	assert_eq(ShipOperation.is_active(), true)


func test_all_stations_start_empty() -> void:
	for sid: String in STATION_IDS:
		assert_eq(ShipOperation.is_occupied(sid), false,
			"Station '%s' should start empty" % sid)


func test_occupy_station() -> void:
	ShipOperation.occupy("pilot", "player")
	assert_eq(ShipOperation.is_occupied("pilot"), true)
	assert_eq(ShipOperation.occupied_by("pilot"), "player")


func test_vacate_station() -> void:
	ShipOperation.occupy("pilot", "player")
	ShipOperation.vacate("pilot")
	assert_eq(ShipOperation.is_occupied("pilot"), false)


func test_occupy_vacate_clears_controls() -> void:
	ShipOperation.occupy("pilot", "player")
	ShipOperation.set_control("pilot", "throttle", 1.0)
	ShipOperation.vacate("pilot")
	assert_eq(ShipOperation.get_control("pilot", "throttle", -999), 0.0,
		"Controls should reset to defaults on vacate")


func test_multiple_stations_independent() -> void:
	ShipOperation.occupy("pilot", "player")
	ShipOperation.occupy("weapons", "tib")
	assert_eq(ShipOperation.is_occupied("pilot"), true)
	assert_eq(ShipOperation.is_occupied("weapons"), true)
	assert_eq(ShipOperation.is_occupied("engineering"), false)


func test_occupy_vacates_previous() -> void:
	ShipOperation.occupy("pilot", "player")
	ShipOperation.occupy("pilot", "tib")  # re-occupy
	assert_eq(ShipOperation.occupied_by("pilot"), "tib",
		"Second occupy should replace first")


func test_set_control_valid() -> void:
	ShipOperation.occupy("pilot", "player")
	ShipOperation.set_control("pilot", "throttle", 0.75)
	assert_eq(ShipOperation.get_control("pilot", "throttle", 0.0), 0.75)


func test_set_control_invalid_axis_safe() -> void:
	ShipOperation.occupy("pilot", "player")
	# Should not error — should silently ignore
	ShipOperation.set_control("pilot", "nonexistent_axis", 1.0)
	assert_eq(ShipOperation.get_control("pilot", "throttle", 0.0), 0.0,
		"Ignoring unknown axis should leave controls unchanged")


func test_set_control_invalid_station_safe() -> void:
	# Unknown station ID should silently ignore
	ShipOperation.set_control("nonexistent_station", "throttle", 1.0)
	# Should not crash — tested by reaching this assertion
	assert_true(true)


func test_controls_default_values() -> void:
	var pilot: Dictionary = ShipOperation.controls.get("pilot", {})
	assert_eq(pilot.get("throttle", -999), 0.0)
	assert_eq(pilot.get("yaw", -999), 0.0)
	assert_eq(pilot.get("pitch", -999), 0.0)
	assert_eq(pilot.get("roll", -999), 0.0)
	assert_eq(pilot.get("boost", null), false)
	assert_eq(pilot.get("brake", null), false)


func test_effects_default_values() -> void:
	assert_eq(ShipOperation.get_effect("engine_glow", -1.0), 0.0)
	assert_eq(ShipOperation.get_effect("weapons_firing", null), false)


func test_set_effect() -> void:
	ShipOperation.set_effect("engine_glow", 0.8)
	assert_eq(ShipOperation.get_effect("engine_glow", 0.0), 0.8)


func test_reset_clears_everything() -> void:
	ShipOperation.occupy("pilot", "player")
	ShipOperation.set_control("pilot", "throttle", 1.0)
	ShipOperation.set_effect("engine_glow", 1.0)
	
	ShipOperation.reset()
	
	assert_eq(ShipOperation.is_occupied("pilot"), false)
	assert_eq(ShipOperation.get_control("pilot", "throttle", -999), 0.0)
	assert_eq(ShipOperation.get_effect("engine_glow", -1.0), 0.0,
		"After reset, effects should be at defaults")


func test_dual_occupancy() -> void:
	ShipOperation.occupy("pilot", "player")
	ShipOperation.occupy("weapons", "tib")
	
	assert_eq(ShipOperation.occupied_by("pilot"), "player")
	assert_eq(ShipOperation.occupied_by("weapons"), "tib")


func test_weapons_defaults() -> void:
	var weapons: Dictionary = ShipOperation.controls.get("weapons", {})
	assert_eq(weapons.get("fire", null), false)
	assert_eq(weapons.get("target_id", null), "")
	assert_eq(weapons.get("weapon_index", -1), 0)
