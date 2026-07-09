extends Node2D
## Ring 0 — CharacterSprite: the one way a person is drawn in world space.
##
## Renders a character from a 4x4 sprite sheet (assets convention:
## `assets/npcs/<id>_sheet.png`, or `assets/player/character_sheet.png` for
## the player) — rows down/up/left/right, four walk frames per row, 24x32
## per frame, drawn at 2x. `set_motion()` every frame is the whole API:
## direction picks the row, movement drives the frame clock, idle breathes
## on a slow two-frame cycle.
##
## No sheet shipped? Falls back to the StandIn painter, so a mod that only
## authors data still gets a readable figure. Artists replace the PNG,
## nothing else moves.

class_name CharacterSprite

const FRAME := Vector2(24, 32)
const DRAW_SCALE := 2.0
const WALK_FPS := 7.0
const IDLE_FPS := 1.6

const ROWS := {"down": 0, "up": 1, "left": 2, "right": 3}

var _sheet: Texture2D = null
var _fallback_color := Color(0.7, 0.7, 0.75)
var _seed_id := ""
var _row := 0
var _clock := 0.0
var _moving := false
var _label := ""
var _floating := false
var _sway := 0.0


## kind/id are AssetLibrary coordinates: ("npcs", npc id) for characters,
## ("player", "character") for the player avatar.
func setup(kind: String, id: String, fallback_color: Color, label := "") -> void:
	_seed_id = id
	_fallback_color = fallback_color
	_label = label
	var sheet_id := id + "_sheet"
	_sheet = AssetLibrary.texture(kind, sheet_id)


## Zero-G presentation: no ground contact, a slow sway instead of a walk
## cycle. Mag-locked characters (locomotion "magnetic") keep walking.
func set_float_mode(enabled: bool) -> void:
	_floating = enabled
	if not enabled:
		rotation = 0.0


func is_floating() -> bool:
	return _floating


func set_motion(direction: Vector2, moving: bool) -> void:
	_moving = moving
	if direction.length() > 0.1:
		if absf(direction.x) >= absf(direction.y):
			_row = ROWS.right if direction.x > 0.0 else ROWS.left
		else:
			_row = ROWS.down if direction.y > 0.0 else ROWS.up


## The sheet row currently facing (contract-testable).
func facing_row() -> int:
	return _row


func has_sheet() -> bool:
	return _sheet != null


func _process(delta: float) -> void:
	if _floating:
		# Adrift: the cycle barely turns, the body sways on its axis.
		_sway += delta
		_clock += delta * (WALK_FPS * 0.35 if _moving else IDLE_FPS * 0.5)
		rotation = sin(_sway * 1.7) * 0.09
	else:
		_clock += delta * (WALK_FPS if _moving else IDLE_FPS)
	queue_redraw()


func _draw() -> void:
	var size := FRAME * DRAW_SCALE
	var lift := (sin(_sway * 1.1) * 3.0 - 4.0) if _floating else 0.0
	var dest := Rect2(-size.x * 0.5, -size.y + 8 + lift, size.x, size.y)
	if _sheet != null:
		var frame := int(_clock) % 4 if _moving else (0 if int(_clock) % 2 == 0 else 2)
		var src := Rect2(Vector2(frame * FRAME.x, _row * FRAME.y), FRAME)
		if not _floating:
			# Ground shadow: figures sit ON the floor (nobody floats a shadow).
			draw_circle(Vector2(0, 6), 10, Color(0, 0, 0, 0.25))
		draw_texture_rect_region(_sheet, dest, src)
	else:
		if not _floating:
			draw_circle(Vector2(0, 6), 10, Color(0, 0, 0, 0.25))
		StandIn.paint_character(self, Rect2(-14, -46, 28, 50), _fallback_color, _seed_id)
	if _label != "":
		draw_string(ThemeDB.fallback_font, Vector2(-40, 20), _label,
			HORIZONTAL_ALIGNMENT_CENTER, 80, 12, Color(0.94, 0.94, 0.97, 0.9))
