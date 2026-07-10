extends GutTest
## Contract test: the weave loom must agree with the reference resolver
## (scripts/check_weave_contract.py) on every golden and adversarial
## fixture. The adversarial ones are the point — a proposal that exceeds
## its allowlist must be provably neutered, here and in CI, forever.

const FIXTURES_DIR := "res://framework/weave/fixtures"


func _fixture_paths() -> Array:
	var paths: Array = []
	for file: String in DirAccess.get_files_at(FIXTURES_DIR):
		if file.ends_with(".json"):
			paths.append(FIXTURES_DIR + "/" + file)
	paths.sort()
	return paths


func _load_json(path: String) -> Variant:
	return JSON.parse_string(FileAccess.get_file_as_string(path))


## Numeric-tolerant deep equality: JSON gives floats, the loom may carry
## ints — 3 and 3.0 are the same fact.
func _deep_eq(a: Variant, b: Variant) -> bool:
	if a is Dictionary and b is Dictionary:
		if a.keys().size() != b.keys().size():
			return false
		for key: Variant in a:
			if not b.has(key) or not _deep_eq(a[key], b[key]):
				return false
		return true
	if a is Array and b is Array:
		if a.size() != b.size():
			return false
		for i in a.size():
			if not _deep_eq(a[i], b[i]):
				return false
		return true
	if (a is int or a is float) and (b is int or b is float):
		return is_equal_approx(float(a), float(b))
	return a == b


func test_fixtures_exist() -> void:
	assert_gt(_fixture_paths().size(), 8, "the contract fixture suite is present")


func test_every_fixture_resolves_like_the_reference() -> void:
	for path: String in _fixture_paths():
		var fixture: Dictionary = _load_json(path)
		var resolved := WeaveLoom.resolve(fixture.get("node", {}), fixture.get("proposal"))
		var expected: Variant = fixture.get("resolved")
		if expected == null:
			assert_true(resolved.is_empty(),
				"%s: a malformed proposal is discarded whole" % path.get_file())
		else:
			assert_true(_deep_eq(resolved, expected),
				"%s: resolution must match the reference\n  expected: %s\n  got:      %s" % [
					path.get_file(), JSON.stringify(expected), JSON.stringify(resolved)])


func test_resolution_is_a_fixed_point() -> void:
	# Re-resolving an already-clamped resolution must change nothing —
	# the allowlist has no second opinion.
	for path: String in _fixture_paths():
		var fixture: Dictionary = _load_json(path)
		var resolved := WeaveLoom.resolve(fixture.get("node", {}), fixture.get("proposal"))
		if resolved.is_empty():
			continue
		var replay := {"line": resolved.line, "mutations": resolved.mutations, "choices": []}
		for choice: Dictionary in resolved.choices:
			replay.choices.append({"text": choice.text, "mutations": choice.mutations})
		var again := WeaveLoom.resolve(fixture.get("node", {}), replay)
		assert_true(_deep_eq(again, resolved),
			"%s: resolving a resolution is the identity" % path.get_file())


func test_the_headline_adversarial_case_in_prose() -> void:
	# The contract's one-sentence promise: 3 because the author said so,
	# never 40 because a model felt like it.
	var node := {"kind": "woven", "return_to": "end", "may": {"grants": [
		{"op": "adjust_faction", "factions": ["reach_compact"], "axes": ["trust"], "max_amount": 3},
	]}}
	var resolved := WeaveLoom.resolve(node, {"line": "x", "mutations": [
		{"op": "adjust_faction", "faction": "reach_compact", "axis": "trust", "amount": 40},
	]})
	assert_eq(float(resolved.mutations[0].amount), 3.0, "40 clamps to the granted 3")
