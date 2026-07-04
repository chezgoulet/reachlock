extends Node2D
## Landed mode — the router for "you are at a place." It mounts the right
## framework scene for the location's kind and owns the mode transitions the
## framework scenes ask for (undock → space, board → on board). Every name on
## screen comes from content; this script names none.
##
## kind "planet"/"moon" → PlanetScene (walkable surface, P7); everything else
## (station/outpost/wreck) → StationDock (P4). A modder adds a place by writing
## a location.json — the router picks the scene from its `kind`.

const StationDockScene := preload("res://scenes/framework/station_dock.tscn")
const PlanetSceneScene := preload("res://scenes/framework/planet_scene.tscn")

## In-game minutes that pass while the ship runs its departure sequence — the
## batch time-skip the M6 contract requires on undock.
const UNDOCK_DEPARTURE_TICKS := 30

var _location: Dictionary = {}


func _ready() -> void:
	var location_id: String = GameState.player.location
	if location_id == "":
		location_id = DataRegistry.start_config().get("location", "")
		GameState.player.location = location_id
	_location = DataRegistry.get_entity("locations", location_id)

	var layer := CanvasLayer.new()
	add_child(layer)
	match _location.get("kind", "station"):
		"planet", "moon":
			var surface: PlanetScene = PlanetSceneScene.instantiate()
			surface.depart_requested.connect(_undock)
			surface.board_ship_requested.connect(_board_ship)
			layer.add_child(surface)
			surface.configure(_location)
		_:
			var dock: StationDock = StationDockScene.instantiate()
			dock.set_anchors_preset(Control.PRESET_FULL_RECT)
			dock.undock_requested.connect(_undock)
			dock.board_ship_requested.connect(_board_ship)
			layer.add_child(dock)
			dock.configure(_location)


func _undock() -> void:
	# Departure clearance takes in-game time; the universe moves through it in
	# one deterministic batch (M6: time passes while you fly).
	SimGateway.advance_batch(UNDOCK_DEPARTURE_TICKS)
	GameState.player.location = ""
	GameState.clear_flag("survived_ambush")  # each flight earns its own stories
	GameManager.request_mode(GameManager.Mode.SPACE_FLIGHT)


func _board_ship() -> void:
	GameManager.request_mode(GameManager.Mode.ON_BOARD)


## Legacy hooks kept for the debug mode switches / tests that call them.
func on_ship_entered() -> void:
	_board_ship()


func on_undock() -> void:
	_undock()
