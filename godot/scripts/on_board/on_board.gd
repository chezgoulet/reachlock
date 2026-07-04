extends Node2D
## On Board mode — your walkable ship interior (Sprint 03, P10).
## Instantiates the ShipInterior scene for the player's hull; crew from
## CrewRoster live at their stations and talk.

const ShipInteriorScene := preload("res://scenes/framework/ship_interior.tscn")


func _ready() -> void:
	var interior: ShipInterior = ShipInteriorScene.instantiate()
	interior.launch_requested.connect(on_launch)
	interior.disembark_requested.connect(func() -> void:
		GameManager.request_mode(GameManager.Mode.LANDED))
	add_child(interior)
	interior.configure(DataRegistry.get_entity("ships", GameState.player.ship.hull_id))


func on_launch() -> void:
	GameManager.request_mode(GameManager.Mode.SPACE_FLIGHT)
