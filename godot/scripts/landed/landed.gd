extends Node2D

var location_id: String = ""

func _ready() -> void:
	pass

func _process(_delta: float) -> void:
	pass

func on_ship_entered() -> void:
	GameManager.request_mode(GameManager.Mode.ON_BOARD)

func on_undock() -> void:
	GameManager.request_mode(GameManager.Mode.SPACE_FLIGHT)
