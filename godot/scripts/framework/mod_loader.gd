extends RefCounted
## Ring 0 — mod discovery and loading (GAME-DESIGN.md §8).
##
## Everything specific arrives through here. Mods are directories under the
## mods root containing a manifest.json; each `provides` kind maps 1:1 to a
## subdirectory of entity JSON files. Load order is dependency order
## (topological); on id collisions the last-loaded mod wins, with a warning.
##
## The loader is deliberately lenient at runtime (log and continue) — strict
## validation is CI's job (scripts/validate_mod_data.py). A player with a
## half-broken mod should get a report, not a crash.

const MODS_ROOT := "res://mods"  # arch-allow: the loader owns the mods root

const REQUIRED_MANIFEST_KEYS: Array[String] = ["id", "name", "version", "provides"]


## Loads every mod and returns:
## {
##   manifests: {mod_id: Dictionary}, order: Array[String],
##   entities: {kind: {id: Dictionary}}, sources: {kind: {id: mod_id}},
##   dirs: {mod_id: dir_path}, start: Dictionary,
##   warnings: Array[String], errors: Array[String],
## }
##
## `dirs` is the loader's own map of each loaded mod's root directory. It is
## how the engine reaches a mod's *non-JSON* assets (sprite overrides, etc.)
## without ever naming a `res://mods/...` path in engine source: the path is
## handed over here, by the layer that owns the mods root.
func load_all() -> Dictionary:
	var out := {
		"manifests": {}, "order": [], "entities": {}, "sources": {},
		"dirs": {}, "start": {}, "warnings": [], "errors": [],
	}
	var dirs := _discover_manifests(out)
	out.dirs = dirs
	var order := _dependency_order(out)
	out.order = order
	_check_conflicts(out)
	for mod_id: String in order:
		_load_mod_entities(mod_id, dirs[mod_id], out)
	return out


## Finds every <MODS_ROOT>/<dir>/manifest.json. Returns {mod_id: dir_path}.
func _discover_manifests(out: Dictionary) -> Dictionary:
	var dirs := {}
	var root := DirAccess.open(MODS_ROOT)
	if root == null:
		out.warnings.append("no mods directory at %s — engine boots empty" % MODS_ROOT)
		return dirs
	root.list_dir_begin()
	var entry := root.get_next()
	while entry != "":
		if root.current_is_dir() and not entry.begins_with("."):
			var mod_dir := MODS_ROOT.path_join(entry)
			var manifest := _read_json(mod_dir.path_join("manifest.json"), out)
			if not manifest.is_empty():
				var problem := _manifest_problem(manifest)
				if problem != "":
					out.errors.append("%s/manifest.json: %s — mod skipped" % [entry, problem])
				elif out.manifests.has(manifest.id):
					out.errors.append("duplicate mod id '%s' (%s) — first one wins" % [manifest.id, entry])
				else:
					out.manifests[manifest.id] = manifest
					dirs[manifest.id] = mod_dir
		entry = root.get_next()
	root.list_dir_end()
	return dirs


func _manifest_problem(manifest: Dictionary) -> String:
	for key in REQUIRED_MANIFEST_KEYS:
		if not manifest.has(key):
			return "manifest missing required key '%s'" % key
	if not manifest.provides is Dictionary:
		return "manifest `provides` must be an object"
	return ""


## Kahn's algorithm over declared dependencies. Missing deps are reported and
## the dependent mod is skipped; cycles are reported and every mod in the
## cycle is skipped (GAME-DESIGN.md §8: circular dependencies are rejected).
func _dependency_order(out: Dictionary) -> Array[String]:
	var pending: Dictionary = {}  # mod_id -> Array of unmet dep ids
	for mod_id: String in out.manifests:
		var deps: Array[String] = []
		for dep in out.manifests[mod_id].get("dependencies", []):
			if out.manifests.has(dep):
				deps.append(dep)
			else:
				out.errors.append("mod '%s' depends on missing mod '%s' — skipped" % [mod_id, dep])
				deps.append(dep)  # unmet forever; keeps the mod out of the order
		pending[mod_id] = deps

	var order: Array[String] = []
	var progressed := true
	while progressed:
		progressed = false
		for mod_id: String in pending.keys():
			var unmet: Array[String] = pending[mod_id]
			var still_unmet := unmet.filter(func(d: String) -> bool: return not order.has(d))
			if still_unmet.is_empty():
				order.append(mod_id)
				pending.erase(mod_id)
				progressed = true
	for mod_id: String in pending:
		if pending[mod_id].all(func(d: String) -> bool: return out.manifests.has(d)):
			out.errors.append("mod '%s' is in a dependency cycle — skipped" % mod_id)
	return order


func _check_conflicts(out: Dictionary) -> void:
	for mod_id: String in out.order:
		for rival in out.manifests[mod_id].get("conflicts", []):
			if out.manifests.has(rival):
				out.warnings.append("mod '%s' declares a conflict with loaded mod '%s'" % [mod_id, rival])


func _load_mod_entities(mod_id: String, mod_dir: String, out: Dictionary) -> void:
	var manifest: Dictionary = out.manifests[mod_id]
	var provides: Dictionary = manifest.provides
	for kind: String in provides:
		var kind_dir := mod_dir.path_join(kind)
		if not out.entities.has(kind):
			out.entities[kind] = {}
			out.sources[kind] = {}
		var declared: Array = provides[kind]
		for file_name in _json_files(kind_dir):
			var data := _read_json(kind_dir.path_join(file_name), out)
			if data.is_empty():
				continue
			var id: String = data.get("id", "")
			if id == "":
				out.errors.append("%s/%s/%s: entity has no id — skipped" % [mod_id, kind, file_name])
				continue
			if not declared.has(id):
				out.warnings.append("%s provides.%s does not declare '%s' — loaded anyway" % [mod_id, kind, id])
			if out.entities[kind].has(id):
				out.warnings.append("%s.%s: mod '%s' overrides '%s'" % [kind, id, mod_id, out.sources[kind][id]])
			out.entities[kind][id] = data
			out.sources[kind][id] = mod_id
	if manifest.get("start") is Dictionary:
		out.start = manifest.start  # last-loaded mod with a start block wins


func _json_files(dir_path: String) -> Array[String]:
	var files: Array[String] = []
	var dir := DirAccess.open(dir_path)
	if dir == null:
		return files
	for file_name in dir.get_files():
		# Exported builds see "<name>.json" only if export filters include
		# *.json; in-editor this is transparent.
		if file_name.ends_with(".json"):
			files.append(file_name)
	files.sort()
	return files


func _read_json(path: String, out: Dictionary) -> Dictionary:
	if not FileAccess.file_exists(path):
		return {}
	var text := FileAccess.get_file_as_string(path)
	var parsed: Variant = JSON.parse_string(text)
	if parsed is Dictionary:
		return parsed
	out.errors.append("%s: not valid JSON (or not an object) — skipped" % path)
	return {}
