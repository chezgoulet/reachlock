extends Node2D

var crew: Array[Node] = []

func _ready() -> void:
	pass

func _process(_delta: float) -> void:
	pass

func on_launch() -> void:
	GameManager.request_mode(GameManager.Mode.SPACE_FLIGHT)
