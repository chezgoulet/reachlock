extends Node
## Ring 0 — the sprite-override loader convention (Sprint 02 art pass).
##
## Every visual in the game is a framework default that a mod can override by
## dropping a file in its own asset directory. This is the resolver for that
## rule: given a (kind, id) — the same coordinates the DataRegistry uses —
## AssetLibrary looks for `<mod>/assets/<kind>/<id>.png` across the loaded
## mods and returns the winning texture, or null when no mod ships one (the
## caller then draws its stand-in default).
##
## Override semantics match the data loader: the LAST mod in load order that
## carries the file wins, so a mod layered on top of REACHLOCK re-skins a
## character by shipping `assets/npcs/tib.png` and nothing else.
##
## The engine never names a `res://mods/...` path: the mod's root directory is
## handed over by the loader (DataRegistry.mod_dir), and the asset path is
## built from it at runtime. Images are loaded through Image so an override
## works whether or not the file went through Godot's import pipeline.

const ASSET_DIR := "assets"
const EXTENSIONS: Array[String] = [".png", ".webp", ".jpg"]

var _cache: Dictionary = {}   # "kind/id" -> Texture2D or null (miss is cached too)


## The override texture for a content entity, or null if no loaded mod ships
## one. `kind` is a DataRegistry kind ("npcs", "locations", "ships", ...);
## `id` is the entity id. Callers treat null as "draw the stand-in default".
func texture(kind: String, id: String) -> Texture2D:
	var key := kind + "/" + id
	if _cache.has(key):
		return _cache[key]
	var found: Texture2D = _resolve(kind, id)
	_cache[key] = found
	return found


## True when a mod ships an override for this entity (cheap: reuses the cache).
func has_override(kind: String, id: String) -> bool:
	return texture(kind, id) != null


func _resolve(kind: String, id: String) -> Texture2D:
	# Last mod in load order wins, mirroring entity-override precedence.
	var order := DataRegistry.load_order()
	order.reverse()
	for mod_id: String in order:
		var root := DataRegistry.mod_dir(mod_id)
		if root == "":
			continue
		var base := root.path_join(ASSET_DIR).path_join(kind).path_join(id)
		for ext: String in EXTENSIONS:
			var path := base + ext
			if FileAccess.file_exists(path):
				var tex := _load_texture(path)
				if tex != null:
					return tex
	return null


func _load_texture(path: String) -> Texture2D:
	# Decode from bytes rather than Image.load(): no "loaded resource as
	# image file" engine warning, and it works identically for loose files
	# and PCK-packed exports.
	var bytes := FileAccess.get_file_as_bytes(path)
	if bytes.is_empty():
		push_warning("assets: could not read override image %s" % path)
		return null
	var image := Image.new()
	var err := ERR_FILE_UNRECOGNIZED
	match path.get_extension().to_lower():
		"png":
			err = image.load_png_from_buffer(bytes)
		"webp":
			err = image.load_webp_from_buffer(bytes)
		"jpg", "jpeg":
			err = image.load_jpg_from_buffer(bytes)
	if err != OK:
		push_warning("assets: could not decode override image %s" % path)
		return null
	return ImageTexture.create_from_image(image)
