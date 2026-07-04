extends Node
## Ring 0 — AudioManager: lightweight sound effect dispatcher.
##
## Preloads common Kenney SFX at startup. Provides named play_* methods
## so scenes don't need to know file paths. Falls back silently if a
## sound file isn't available — no crashes, no warnings.

## Path to the Kenney audio packs within the mod's asset directory.
# arch-allow: audio files are mod content loaded by the engine at runtime
const AUDIO_ROOT := "res://mods/reachlock/assets/audio/"  # arch-allow: content path

var _sfx := {}  # name -> AudioStream


func _ready() -> void:
	_load_sfx("engine_medium", "kenney_sci-fi-sounds/Audio/spaceEngine_001.ogg")
	_load_sfx("engine_large", "kenney_sci-fi-sounds/Audio/spaceEngineLarge_001.ogg")
	_load_sfx("engine_low", "kenney_sci-fi-sounds/Audio/spaceEngineLow_001.ogg")
	_load_sfx("engine_circular", "kenney_sci-fi-sounds/Audio/engineCircular_000.ogg")
	_load_sfx("thruster", "kenney_sci-fi-sounds/Audio/thrusterFire_000.ogg")
	_load_sfx("laser_large", "kenney_sci-fi-sounds/Audio/laserLarge_000.ogg")
	_load_sfx("laser_small", "kenney_sci-fi-sounds/Audio/laserSmall_000.ogg")
	_load_sfx("laser_retro", "kenney_sci-fi-sounds/Audio/laserRetro_000.ogg")
	_load_sfx("explosion", "kenney_sci-fi-sounds/Audio/explosionCrunch_000.ogg")
	_load_sfx("force_field", "kenney_sci-fi-sounds/Audio/forceField_000.ogg")
	_load_sfx("impact_metal", "kenney_sci-fi-sounds/Audio/impactMetal_001.ogg")
	_load_sfx("impact_glass", "kenney_sci-fi-sounds/Audio/impactGlass_heavy_002.ogg")
	_load_sfx("impact_plate", "kenney_sci-fi-sounds/Audio/impactPlate_heavy_001.ogg")
	_load_sfx("door_open", "kenney_sci-fi-sounds/Audio/doorOpen_001.ogg")
	_load_sfx("door_close", "kenney_sci-fi-sounds/Audio/doorClose_001.ogg")
	_load_sfx("computer_noise", "kenney_sci-fi-sounds/Audio/computerNoise_000.ogg")
	
	# UI sounds
	_load_sfx("ui_click", "kenney_ui-audio/Audio/click1.ogg")
	_load_sfx("ui_hover", "kenney_ui-audio/Audio/rollover1.ogg")
	_load_sfx("ui_switch", "kenney_ui-audio/Audio/switch1.ogg")
	
	# Digital sounds
	_load_sfx("alert", "kenney_digital-audio/Audio/zapTwoTone.ogg")
	_load_sfx("power_up", "kenney_digital-audio/Audio/powerUp1.ogg")
	_load_sfx("phase_jump", "kenney_digital-audio/Audio/phaseJump1.ogg")


func _load_sfx(name: String, rel_path: String) -> void:
	var full_path := AUDIO_ROOT + rel_path
	if ResourceLoader.exists(full_path):
		_sfx[name] = ResourceLoader.load(full_path)
	else:
		# Silently skip missing files
		pass


## Play a sound effect once, optionally at a given position.
func play(name: String, pitch_scale: float = 1.0, volume_db: float = 0.0) -> void:
	if not _sfx.has(name):
		return
	var asp := AudioStreamPlayer2D.new()
	asp.stream = _sfx[name]
	asp.pitch_scale = pitch_scale
	asp.volume_db = volume_db
	asp.finished.connect(asp.queue_free)
	add_child(asp)
	asp.play()


## Play a looping engine sound. Returns the player so the caller can adjust
## volume/pitch per frame.
func play_loop(name: String) -> AudioStreamPlayer2D:
	if not _sfx.has(name):
		return null
	var asp := AudioStreamPlayer2D.new()
	asp.stream = _sfx[name]
	asp.autoplay = true
	add_child(asp)
	asp.play()
	return asp


## Quick accessors for common sounds.
func ui_click() -> void: play("ui_click")
func ui_hover() -> void: play("ui_hover", 1.2)
func ui_switch() -> void: play("ui_switch")
func laser_fire() -> void: play("laser_large", randf_range(0.9, 1.1))
func explosion() -> void: play("explosion", randf_range(0.85, 1.15))
func impact() -> void: play("impact_metal", randf_range(0.9, 1.1))
func door_open() -> void: play("door_open")
func door_close() -> void: play("door_close")
func alert() -> void: play("alert")
func power_up() -> void: play("power_up")
func phase_jump() -> void: play("phase_jump", randf_range(0.95, 1.05))
