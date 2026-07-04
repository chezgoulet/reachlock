extends GutTest
## Contract test: GravitySystem autoload (Sprint 03).
##
## Tests gravity type configuration, movement modifiers, strength scaling,
## zero-G drift behavior, and failure/reset cycles.

func before_each() -> void:
	GravitySystem.reset()


func test_default_is_energy_plate_1g() -> void:
	assert_eq(GravitySystem.gravity_type, GravitySystem.Gravity.ENERGY_PLATE)
	assert_eq(GravitySystem.strength, 1.0)
	assert_eq(GravitySystem.safe, true)


func test_configure_zero_gravity() -> void:
	GravitySystem.configure({"type": "none", "strength": 0.0})
	assert_eq(GravitySystem.gravity_type, GravitySystem.Gravity.NONE)
	assert_eq(GravitySystem.strength, 0.0)


func test_configure_energy_plate() -> void:
	GravitySystem.configure({"type": "energy_plate", "strength": 1.0, "power_draw": 0.15})
	assert_eq(GravitySystem.gravity_type, GravitySystem.Gravity.ENERGY_PLATE)
	assert_eq(GravitySystem.strength, 1.0)


func test_configure_magnetic_boots() -> void:
	GravitySystem.configure({"type": "magnetic_boots", "strength": 0.8})
	assert_eq(GravitySystem.gravity_type, GravitySystem.Gravity.MAGNETIC_BOOTS)
	assert_eq(GravitySystem.can_jump(), false,
		"Magnetic boots should not allow jumping")


func test_configure_centrifugal() -> void:
	GravitySystem.configure({"type": "centrifugal", "strength": 0.6})
	assert_eq(GravitySystem.gravity_type, GravitySystem.Gravity.CENTRIFUGAL)


func test_configure_grav_plate() -> void:
	GravitySystem.configure({"type": "grav_plate", "strength": 1.2})
	assert_eq(GravitySystem.gravity_type, GravitySystem.Gravity.GRAV_PLATE)


func test_zero_gravity_movement() -> void:
	GravitySystem.configure({"type": "none", "strength": 0.0})
	var result := GravitySystem.apply_movement(Vector2(1, 0), 0.016, Vector2.ZERO)
	var move: Vector2 = result.get("move", Vector2.ZERO)
	# In zero-G, WASD applies acceleration; releasing keys keeps velocity
	assert_ne(move.x, 0.0, "Zero-G movement should produce thrust")
	assert_ne(move, Vector2(1, 0), "Zero-G movement should be different from standard input")


func test_standard_gravity_movement() -> void:
	# Default: energy_plate at 1.0G
	var result := GravitySystem.apply_movement(Vector2(1, 0), 0.016, Vector2.ZERO)
	var move: Vector2 = result.get("move", Vector2.ZERO)
	assert_ne(move, Vector2.ZERO, "Normal gravity should pass input through")


func test_low_gravity_speed_reduction() -> void:
	GravitySystem.configure({"type": "energy_plate", "strength": 0.25})
	var result := GravitySystem.apply_movement(Vector2(1, 0), 0.016, Vector2.ZERO)
	var move: Vector2 = result.get("move", Vector2.ZERO)
	assert_lt(move.length(), 1.0, "Low gravity should reduce movement speed")


func test_high_gravity_speed_reduction() -> void:
	GravitySystem.configure({"type": "grav_plate", "strength": 2.0})
	var result := GravitySystem.apply_movement(Vector2(1, 0), 0.016, Vector2.ZERO)
	var move: Vector2 = result.get("move", Vector2.ZERO)
	assert_lt(move.length(), 0.5, "High gravity should heavily reduce movement")


func test_strength_clamping() -> void:
	GravitySystem.set_strength(5.0)
	assert_eq(GravitySystem.strength, 3.0, "Strength should clamp to max 3.0")
	GravitySystem.set_strength(-1.0)
	assert_eq(GravitySystem.strength, 0.0, "Strength should clamp to min 0.0")


func test_type_name() -> void:
	GravitySystem.configure({"type": "none"})
	assert_eq(GravitySystem.type_name(), "none")
	GravitySystem.configure({"type": "energy_plate"})
	assert_eq(GravitySystem.type_name(), "energy_plate")
	GravitySystem.configure({"type": "centrifugal"})
	assert_eq(GravitySystem.type_name(), "centrifugal")


func test_configure_location() -> void:
	# configure_location is an alias for configure for environment use
	GravitySystem.configure_location({"type": "none", "strength": 0.0, "safe": false})
	assert_eq(GravitySystem.strength, 0.0)
	assert_eq(GravitySystem.safe, false)


func test_reset_restores_defaults() -> void:
	GravitySystem.configure({"type": "none", "strength": 0.0})
	GravitySystem.reset()
	assert_eq(GravitySystem.gravity_type, GravitySystem.Gravity.ENERGY_PLATE)
	assert_eq(GravitySystem.strength, 1.0)
	assert_eq(GravitySystem.safe, true)


func test_unknown_type_defaults_to_energy_plate() -> void:
	GravitySystem.configure({"type": "quantum_flux", "strength": 2.0})
	assert_eq(GravitySystem.gravity_type, GravitySystem.Gravity.ENERGY_PLATE,
		"Unknown gravity type should fall back to energy_plate")
