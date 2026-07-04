extends Node2D
## On Board mode — your ship as a place (M8). Instantiates the framework
## ShipInterior scene for the player's hull; the crew CrewRoster says is
## aboard live at their stations, hear the same news you do, and talk.
## Engine code: every room name, crew member, and color comes from content.

const ShipInteriorScene := preload("res://scenes/framework/ship_interior.tscn")


func _ready() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)
	var interior: ShipInterior = ShipInteriorScene.instantiate()
	interior.set_anchors_preset(Control.PRESET_FULL_RECT)
	layer.add_child(interior)
	interior.launch_requested.connect(on_launch)
	interior.disembark_requested.connect(func() -> void:
		GameManager.request_mode(GameManager.Mode.LANDED))
	interior.configure(DataRegistry.get_entity("ships", GameState.player.ship.hull_id))


func on_launch() -> void:
	GameManager.request_mode(GameManager.Mode.SPACE_FLIGHT)
