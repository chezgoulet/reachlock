extends Node
## Ring 0 — the single window engine systems read content through
## (docs/ARCHITECTURE.md). Populated by the mod loader at boot. Engine code
## never touches a mod directory or names a content id; it asks the registry
## for whatever is loaded.

signal mods_loaded(order: Array[String])

const ModLoaderScript := preload("res://scripts/framework/mod_loader.gd")

var _entities := {}   # kind -> {id -> Dictionary}
var _sources := {}    # kind -> {id -> mod_id that provided the winning copy}
var _manifests := {}  # mod_id -> manifest Dictionary
var _dirs := {}       # mod_id -> root directory (for non-JSON assets)
var _order: Array[String] = []
var _start := {}


func _ready() -> void:
	var result: Dictionary = ModLoaderScript.new().load_all()
	_entities = result.entities
	_sources = result.sources
	_manifests = result.manifests
	_dirs = result.dirs
	_order.assign(result.order)
	_start = result.start
	for warning: String in result.warnings:
		push_warning("mods: %s" % warning)
	for error: String in result.errors:
		push_error("mods: %s" % error)
	print("mods: loaded %s (%d entities)" % ["+".join(_order) if not _order.is_empty() else "none", entity_count()])
	mods_loaded.emit(_order)


## The merged entity for `id`, or {} if nothing provides it. Callers treat the
## result as read-only authored data; runtime state lives elsewhere (saves).
func get_entity(kind: String, id: String) -> Dictionary:
	return (_entities.get(kind, {}) as Dictionary).get(id, {})


func has_entity(kind: String, id: String) -> bool:
	return (_entities.get(kind, {}) as Dictionary).has(id)


func ids(kind: String) -> Array[String]:
	var result: Array[String] = []
	result.assign((_entities.get(kind, {}) as Dictionary).keys())
	result.sort()
	return result


func kinds() -> Array[String]:
	var result: Array[String] = []
	result.assign(_entities.keys())
	result.sort()
	return result


## Which mod provided the loaded copy of an entity (after overrides).
func source_mod(kind: String, id: String) -> String:
	return (_sources.get(kind, {}) as Dictionary).get(id, "")


func manifest(mod_id: String) -> Dictionary:
	return _manifests.get(mod_id, {})


func load_order() -> Array[String]:
	return _order.duplicate()


## The root directory a mod loaded from ("" if unknown). The loader owns the
## mods root; this is how engine systems reach a mod's non-JSON assets (sprite
## overrides and the like) without naming a content path — see AssetLibrary.
func mod_dir(mod_id: String) -> String:
	return _dirs.get(mod_id, "")


## The `start` block from the last-loaded mod that declared one: where a new
## game begins (mode, player_ship, location). Empty if no content is loaded —
## engine callers must default sensibly.
func start_config() -> Dictionary:
	return _start


func entity_count() -> int:
	var total := 0
	for kind: String in _entities:
		total += (_entities[kind] as Dictionary).size()
	return total
