extends Node
## Ring 0 — GravitySystem: the environment's gravity state.
##
## Every scene (ship interior, planet surface, station dock, spacewalk)
## queries this autoload to know how the player should move.
## Ships define their gravity via hull.gravity; locations and planets
## override it via their own gravity block.
##
## Zero-G from combat damage, engineering failures, or spacewalks is
## handled by writing to this autoload. The _Walker never checks the
## hull — it checks *this*.

signal gravity_changed(type: String, strength: float)
signal gravity_failure_warning(duration: float)

enum Gravity {
	NONE = 0,
	MAGNETIC_BOOTS = 1,
	CENTRIFUGAL = 2,
	ENERGY_PLATE = 3,
	GRAV_PLATE = 4,
}

const TYPE_NAMES := {
	Gravity.NONE: "none",
	Gravity.MAGNETIC_BOOTS: "magnetic_boots",
	Gravity.CENTRIFUGAL: "centrifugal",
	Gravity.ENERGY_PLATE: "energy_plate",
	Gravity.GRAV_PLATE: "grav_plate",
}

## Current gravity type. Set by the active scene.
var gravity_type: int = Gravity.ENERGY_PLATE

## 0.0 = zero-G, 1.0 = Earth standard.
var strength: float = 1.0

## True when gravity is stable. False during damage, startup, or failure.
var safe: bool = true

var _player_frozen: bool = false


func _ready() -> void:
	reset()


## Set gravity from a hull definition block. Called by ShipInterior when
## interior is configured.
func configure(config: Dictionary) -> void:
	var type_str: String = config.get("type", "energy_plate")
	var new_type := _parse_type(type_str)
	gravity_type = new_type
	strength = config.get("strength", 1.0)
	safe = config.get("safe", true)
	gravity_changed.emit(type_str, strength)


## Set gravity from a location / planet biome block.
func configure_location(config: Dictionary) -> void:
	configure(config)


## Set strength only (e.g., engineering power slider, spin-up/down).
func set_strength(new_strength: float) -> void:
	strength = clampf(new_strength, 0.0, 3.0)
	var type_str: String = TYPE_NAMES.get(gravity_type, "none")
	gravity_changed.emit(type_str, strength)


## Trigger a gravity failure (combat damage, malfunction).
func trigger_failure(warning_seconds: float = 3.0) -> void:
	if not safe:
		return
	safe = false
	gravity_failure_warning.emit(warning_seconds)
	# After the warning, cut gravity
	await get_tree().create_timer(warning_seconds).timeout
	strength = 0.0
	gravity_type = Gravity.NONE
	var type_str: String = TYPE_NAMES.get(gravity_type, "none")
	gravity_changed.emit(type_str, 0.0)


## Restore gravity after a failure.
func restore(config: Dictionary) -> void:
	safe = true
	configure(config)


## Reset to defaults.
func reset() -> void:
	gravity_type = Gravity.ENERGY_PLATE
	strength = 1.0
	safe = true


## Apply gravity to a movement vector. Every _Walker calls this each frame.
## Returns a modified delta (for zero-G drift) and a modified move vector.
func apply_movement(move: Vector2, delta: float, velocity: Vector2) -> Dictionary:
	if strength < 0.1:
		# Zero-G: velocity doesn't decay naturally. WASD applies
		# acceleration, but releasing keys doesn't stop you.
		var accel := 200.0
		var new_vel := velocity + move * accel * delta
		# Clamp max drift speed
		var max_speed := 150.0
		if new_vel.length() > max_speed:
			new_vel = new_vel.normalized() * max_speed
		return {"move": new_vel, "delta": delta}
	
	# Normal gravity: movement works as expected. Strength affects speed.
	var speed_mult := 1.0
	if strength < 0.4:
		# Low-G floaty
		speed_mult = 0.6
	elif strength > 1.5:
		# High-G heavy
		speed_mult = 0.35
	
	return {"move": move * speed_mult, "delta": delta, "velocity": Vector2.ZERO}


## Is this a type that allows jumping?
func can_jump() -> bool:
	if strength < 0.3:
		return false
	return gravity_type != Gravity.MAGNETIC_BOOTS


## Name for the HUD
func type_name() -> String:
	return TYPE_NAMES.get(gravity_type, "unknown")


func _parse_type(s: String) -> int:
	match s:
		"none": return Gravity.NONE
		"magnetic_boots": return Gravity.MAGNETIC_BOOTS
		"centrifugal": return Gravity.CENTRIFUGAL
		"energy_plate": return Gravity.ENERGY_PLATE
		"grav_plate": return Gravity.GRAV_PLATE
		_: return Gravity.ENERGY_PLATE
