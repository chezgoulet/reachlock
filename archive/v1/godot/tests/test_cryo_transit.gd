extends GutTest
## Contract test: CryoTransit (the self-jump ritual, GAME-DESIGN.md §6.3).
## The sequence must pod up the organics, hand the ship to the jump pilot,
## run the tunnel, and emit `finished` with the route it was given —
## entirely from data, with or without a mind daemon.

var _finished_routes: Array = []


func _on_finished(route: Dictionary) -> void:
	_finished_routes.append(route)


func test_sequence_runs_to_finished() -> void:
	_finished_routes = []
	var transit := CryoTransit.new()
	add_child_autofree(transit)
	transit.finished.connect(_on_finished)
	transit.begin({
		"to": "sorrow_station",
		"name": "test leg",
		"pilot_line": "Sleep.",
		"arrival_line": "Wake.",
		"transit_seconds": 1.0,
	})
	# Drive the phases with coarse manual ticks; the engine's own frame
	# ticks are negligible next to these.
	for i in 60:
		transit._process(0.5)
		if not _finished_routes.is_empty():
			break
	assert_eq(_finished_routes.size(), 1, "transit finishes exactly once")
	if not _finished_routes.is_empty():
		assert_eq(_finished_routes[0].get("to", ""), "sorrow_station")


func test_organics_sleep_and_a_synthetic_flies() -> void:
	# The crew data must yield at least one synthetic jump pilot and the
	# organics list the pods are built from — the demo's crossing depends
	# on both existing in content.
	var synthetics := 0
	var pilots := 0
	var organics := 0
	for crew_id: String in CrewRoster.aboard():
		var npc := DataRegistry.get_entity("npcs", crew_id)
		if npc.get("synthetic", false):
			synthetics += 1
			if npc.get("jump_pilot", false):
				pilots += 1
		else:
			organics += 1
	assert_gt(synthetics, 0, "someone must fly the crossing awake")
	assert_gt(pilots, 0, "a rated jump pilot is aboard")
	assert_gt(organics, 0, "someone must sleep through it")
