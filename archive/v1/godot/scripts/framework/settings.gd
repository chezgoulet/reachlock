extends Node
## Ring 0 — Settings: player preferences that live OUTSIDE the save
## (user://settings.json). Volumes, fullscreen, typewriter cadence — the
## things about the machine and the player, not the playthrough. Applied
## at boot before any scene loads; every write persists immediately.
##
## The keybind card (title/pause menus) renders from GameManager's
## binding table — display-only this sprint; remapping is a later pass.

const SETTINGS_PATH := "user://settings.json"

signal changed

var _values := {
	"master_volume": 1.0,   # 0..1 linear
	"fullscreen": false,
	"typewriter_cps": 55.0, # DialoguePanel cadence; 0 = instant
}


func _ready() -> void:
	_load()
	_apply()


func get_value(key: String) -> Variant:
	return _values.get(key)


func set_value(key: String, value: Variant) -> void:
	if not _values.has(key) or _values[key] == value:
		return
	_values[key] = value
	_apply()
	_save()
	changed.emit()


func _apply() -> void:
	var linear := clampf(float(_values.master_volume), 0.0, 1.0)
	AudioServer.set_bus_volume_db(0, linear_to_db(maxf(linear, 0.0001)))
	AudioServer.set_bus_mute(0, linear <= 0.0)
	if DisplayServer.get_name() != "headless":
		DisplayServer.window_set_mode(
			DisplayServer.WINDOW_MODE_FULLSCREEN if bool(_values.fullscreen)
			else DisplayServer.WINDOW_MODE_WINDOWED)


func _load() -> void:
	if not FileAccess.file_exists(SETTINGS_PATH):
		return
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(SETTINGS_PATH))
	if parsed is Dictionary:
		for key: String in _values:
			if (parsed as Dictionary).has(key):
				_values[key] = parsed[key]


func _save() -> void:
	var file := FileAccess.open(SETTINGS_PATH, FileAccess.WRITE)
	if file == null:
		push_warning("settings: cannot write %s" % SETTINGS_PATH)
		return
	file.store_string(JSON.stringify(_values, "  "))
	file.close()
