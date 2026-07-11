extends Control
## Ring 0 — the stand-in character visual (Sprint 02 art pass).
##
## One consistent visual language for a person on screen until real sprite art
## lands: flat 2D, solid colors, one darker side for minimal shading — the
## late-90s SNES-RPG look (LttP, FF, Stardew). Each character reads by a
## distinctive silhouette plus one primary color (the npc `color` field);
## the silhouette varies deterministically by id so two blue-coated characters
## still don't look identical.
##
## Override path: if a mod ships `assets/npcs/<id>.png`, AssetLibrary returns
## it and StandIn draws that instead — same rule for every framework visual.
##
## The painter is exposed as a static function so world-space scenes
## (PlanetScene's surface figures, the player avatar) draw the exact same
## vocabulary from a plain CanvasItem, not just this Control.

class_name StandIn

const DEFAULT_SIZE := Vector2(46, 74)

var _kind: String = "npcs"
var _id: String = ""
var _color: Color = Color(0.6, 0.6, 0.65)
var _override: Texture2D = null


func _ready() -> void:
	if custom_minimum_size == Vector2.ZERO:
		custom_minimum_size = DEFAULT_SIZE


## Point the figure at a content entity. `color` is the character's primary
## color (npc `color`, with a deterministic fallback); `id` seeds both the
## silhouette variation and the sprite-override lookup.
func configure(id: String, color: Color, kind := "npcs") -> void:
	_id = id
	_color = color
	_kind = kind
	_override = AssetLibrary.texture(kind, id)
	queue_redraw()


func _draw() -> void:
	var rect := Rect2(Vector2.ZERO, size)
	if _override != null:
		draw_texture_rect(_override, rect, false)
		return
	paint_character(self, rect, _color, _id)


## --- shared vocabulary (static: any CanvasItem can draw a stand-in) ---------


## Draw a flat stand-in character into `rect` on canvas item `ci`. Solid fills,
## a single darker side for shading, a silhouette that shifts by `seed_id`
## (build, headgear) so characters differ beyond color alone.
static func paint_character(ci: CanvasItem, rect: Rect2, color: Color, seed_id: String) -> void:
	var w := rect.size.x
	var h := rect.size.y
	var ox := rect.position.x
	var oy := rect.position.y
	var rng := _rng_for(seed_id)
	var shade := color.darkened(0.35)
	var light := color.lightened(0.18)
	var skin := Color(0.86, 0.72, 0.6).lerp(color, 0.12)
	var outline := color.darkened(0.72)

	# Ground shadow — grounds the figure in the scene.
	ci.draw_circle(Vector2(ox + w * 0.5, oy + h * 0.94), w * 0.34, Color(0, 0, 0, 0.22))

	# Torso: a rounded coat block, primary color, one darker half for shading.
	var body_top := oy + h * 0.42
	var body := Rect2(ox + w * 0.20, body_top, w * 0.60, h * 0.46)
	ci.draw_rect(body, color)
	ci.draw_rect(Rect2(body.position.x + body.size.x * 0.5, body.position.y,
		body.size.x * 0.5, body.size.y), shade)
	# Shoulders — a wider band reads as a coat; width varies by build.
	var build: float = 0.5 + rng.randf() * 0.5
	var shoulder_w := w * (0.62 + build * 0.14)
	ci.draw_rect(Rect2(ox + (w - shoulder_w) * 0.5, body_top - h * 0.02,
		shoulder_w, h * 0.10), color)
	ci.draw_rect(Rect2(ox + w * 0.5, body_top - h * 0.02,
		shoulder_w * 0.5, h * 0.10), shade)

	# Head.
	var head_c := Vector2(ox + w * 0.5, oy + h * 0.30)
	var head_r := w * 0.20
	ci.draw_circle(head_c, head_r, skin)
	ci.draw_circle(head_c + Vector2(head_r * 0.32, 0), head_r * 0.68, skin.darkened(0.14))
	# Headgear (hat/hood/none) picked by id — a cheap silhouette differentiator.
	match rng.randi() % 3:
		0:  # brimmed hat
			ci.draw_rect(Rect2(head_c.x - head_r * 1.35, head_c.y - head_r * 0.55,
				head_r * 2.7, head_r * 0.32), light)
			ci.draw_rect(Rect2(head_c.x - head_r * 0.8, head_c.y - head_r * 1.25,
				head_r * 1.6, head_r * 0.8), color)
		1:  # hood/collar up
			ci.draw_colored_polygon(PackedVector2Array([
				Vector2(head_c.x - head_r * 1.1, head_c.y + head_r * 0.9),
				Vector2(head_c.x + head_r * 1.1, head_c.y + head_r * 0.9),
				Vector2(head_c.x, head_c.y - head_r * 0.4)]), shade)
			ci.draw_circle(head_c, head_r, skin)
		_:  # bare — a hair cap
			ci.draw_arc(head_c, head_r * 1.02, PI, TAU, 10, light, head_r * 0.5)

	# A thin outline pass to firm up the read at small sizes.
	ci.draw_rect(body, outline, false, 1.5)


## The character's primary color: the authored `color`, or a deterministic
## per-id hue when content supplies none. Centralized so every scene colors a
## character the same way (ShipInterior, StationDock, PlanetScene).
static func character_color(npc: Dictionary, id: String) -> Color:
	var authored: String = str(npc.get("color", ""))
	if authored != "":
		return Color.from_string(authored, fallback_color(id))
	return fallback_color(id)


static func fallback_color(id: String) -> Color:
	var hue := float(hash(id) % 360) / 360.0
	return Color.from_hsv(hue, 0.55, 0.85)


static func _rng_for(seed_id: String) -> RandomNumberGenerator:
	var rng := RandomNumberGenerator.new()
	rng.seed = hash(seed_id)
	return rng
