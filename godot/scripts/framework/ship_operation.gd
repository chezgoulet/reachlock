extends Node
## Ring 0 — ShipOperation: the single source of truth for the ship's
## real-time operational state (Sprint 03, P9).
##
## The 2D walkable interior WRITES to this autoload (player walks to
## pilot console → sets throttle). The 3D exterior flight view READS
## from it (engine glow brightens, ship pitches). Neither scene knows
## about the other — this is the contract between them.
##
## Also acts as the crew-AI bridge: when the player vacates a station,
## CrewRoster's aboard crew auto-fill empty stations at reduced
## effectiveness.
##
## RESET on every mode switch (GameManager calls reset() when entering
## SPACE_FLIGHT). Transient state — never saved.

signal station_occupied(station_id: String, crew_id: String)
signal station_vacated(station_id: String)
signal control_changed(station_id: String, axis: String, value: Variant)
signal fired(weapon_id: String, target: Vector3)

## The canonical station IDs. Extensible — a mod adds more via hull JSON.
enum Station {
	PILOT = 0,
	WEAPONS = 1,
	ENGINEERING = 2,
	SCANNER = 3,
	CARGO = 4,
}

const STATION_NAMES := {
	Station.PILOT: "pilot",
	Station.WEAPONS: "weapons",
	Station.ENGINEERING: "engineering",
	Station.SCANNER: "scanner",
	Station.CARGO: "cargo",
}

## Station → who occupies it (null if empty). crew_id is "player" or
## an NPC soul id like "some_crew_character".
var stations: Dictionary = {}

## Per-station control state. Every station writes its current input
## here; the 3D exterior reads it each frame.
var controls: Dictionary = {}

## Visual effects state — what the exterior ship model reads for
## animation. Updated by interior scene, read by exterior.
var effects: Dictionary = {}

## True after reset() is called. The 3D flight scene checks this:
## if false, it falls back to direct keyboard input.
var _initialized := false


func _ready() -> void:
	reset()


## Reset ALL state to defaults. Called on mode switch to SPACE_FLIGHT
## and in _ready(). Every new field must be added here.
func reset() -> void:
	reset_stations()
	reset_controls()
	reset_effects()
	_initialized = true


func reset_stations() -> void:
	stations = {}
	for key: int in STATION_NAMES:
		stations[STATION_NAMES[key]] = null


func reset_controls() -> void:
	controls = {
		"pilot": {
			"throttle": 0.0,  # -1.0 .. 1.0
			"yaw": 0.0,       # -1.0 .. 1.0
			"pitch": 0.0,     # -1.0 .. 1.0
			"roll": 0.0,      # -1.0 .. 1.0
			"boost": false,
			"brake": false,
		},
		"weapons": {
			"fire": false,
			"target_id": "",       # soul id or "" when no lock
			"target_position": Vector3.ZERO,
			"weapon_index": 0,     # selected weapon group
		},
		"engineering": {
			"power_weapons": 0.33,  # 0.0 .. 1.0 budget share
			"power_shields": 0.33,
			"power_engines": 0.34,
			"repair_target": "",    # system id or ""
		},
		"scanner": {
			"mode": "passive",  # passive | active | targeting
			"selected_contact": "",
		},
		"cargo": {
			"selected_good": "",
			"transfer_amount": 0,
		},
	}


func reset_effects() -> void:
	effects = {
		"engine_glow": 0.0,       # 0..1, proportional to throttle abs
		"weapons_firing": false,
		"shield_level": 0.0,      # 0..1
		"hull_damage": [],        # Array of Vector3 positions for decals
		"engine_trail": 0.0,      # 0..1
	}


## --- occupancy API -----------------------------------------------------------


## Player or crew occupies a station. Fires station_occupied signal.
func occupy(station_id: String, crew_id: String) -> void:
	# Vacate whoever was there before (should be null but guard anyway).
	if stations.get(station_id, null) != null:
		vacate(station_id)
	stations[station_id] = crew_id
	station_occupied.emit(station_id, crew_id)


## Player or crew leaves a station. Fires station_vacated signal.
func vacate(station_id: String) -> void:
	if stations.get(station_id, null) == null:
		return
	stations[station_id] = null
	station_vacated.emit(station_id)
	# Reset controls for that station so the 3D exterior doesn't see
	# stale input.
	if controls.has(station_id):
		var defaults := _defaults_for(station_id)
		for axis: String in defaults:
			controls[station_id][axis] = defaults[axis]
			control_changed.emit(station_id, axis, defaults[axis])


func occupied_by(station_id: String) -> String:
	return stations.get(station_id, "")


func is_occupied(station_id: String) -> bool:
	return stations.get(station_id, null) != null


## --- control API -------------------------------------------------------------


## Set a control axis on a station. Emits control_changed.
func set_control(station_id: String, axis: String, value: Variant) -> void:
	if not controls.has(station_id):
		return
	if not controls[station_id].has(axis):
		return
	controls[station_id][axis] = value
	control_changed.emit(station_id, axis, value)


## Get a control value. Returns the default if the station/axis doesn't
## exist (graceful degradation for mod-added stations with no controls).
func get_control(station_id: String, axis: String, default: Variant = null) -> Variant:
	var s: Dictionary = controls.get(station_id, {})
	return s.get(axis, default)


## --- effects API -------------------------------------------------------------


func set_effect(key: String, value: Variant) -> void:
	if effects.has(key):
		effects[key] = value


func get_effect(key: String, default: Variant = null) -> Variant:
	return effects.get(key, default)


## --- helpers -----------------------------------------------------------------


func is_active() -> bool:
	return _initialized


## Return the default control state for a station, for reset purposes.
func _defaults_for(station_id: String) -> Dictionary:
	match station_id:
		"pilot":
			return {"throttle": 0.0, "yaw": 0.0, "pitch": 0.0, "roll": 0.0, "boost": false, "brake": false}
		"weapons":
			return {"fire": false, "target_id": "", "target_position": Vector3.ZERO, "weapon_index": 0}
		"engineering":
			return {"power_weapons": 0.33, "power_shields": 0.33, "power_engines": 0.34, "repair_target": ""}
		"scanner":
			return {"mode": "passive", "selected_contact": ""}
		"cargo":
			return {"selected_good": "", "transfer_amount": 0}
		_:
			return {}
