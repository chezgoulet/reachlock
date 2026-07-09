extends GutTest
## Contract test: InteriorWorld — the shared walkable-interior model.
## Walls are solid, doors are passable, props obey trigger-DSL conditions.

const ROOM_A := Rect2(0, 0, 200, 200)
const ROOM_B := Rect2(240, 0, 200, 200)  # 40px hull gap between them


func _world_with_door() -> InteriorWorld:
	var world := InteriorWorld.new()
	add_child_autofree(world)
	world.setup([
		{"id": "a", "name": "A", "kind": "corridor", "rect": ROOM_A,
			"color": Color.GRAY, "props": [],
			"doors": [{"to": "b", "side": "right", "offset": 0.5, "width": 40.0}]},
		{"id": "b", "name": "B", "kind": "bar", "rect": ROOM_B,
			"color": Color.GRAY, "props": [], "doors": []},
	])
	return world


func test_room_interiors_are_walkable() -> void:
	var world := _world_with_door()
	assert_true(world.is_walkable(Vector2(100, 100)), "middle of room A")
	assert_true(world.is_walkable(Vector2(340, 100)), "middle of room B")


func test_outside_and_hull_gap_are_not_walkable() -> void:
	var world := _world_with_door()
	assert_false(world.is_walkable(Vector2(600, 600)), "outside everything")
	assert_false(world.is_walkable(Vector2(220, 20)), "the hull gap, away from the door")


func test_door_bridge_is_walkable() -> void:
	var world := _world_with_door()
	# Door on A's right wall at offset 0.5 → y = 100; the bridge spans the gap.
	assert_true(world.is_walkable(Vector2(220, 100)), "the doorway between the rooms")


func test_prop_conditions_follow_story_state() -> void:
	GameState.clear_flag("test_wrecked")
	var world := InteriorWorld.new()
	add_child_autofree(world)
	world.setup([
		{"id": "a", "name": "A", "kind": "bar", "rect": ROOM_A, "color": Color.GRAY,
			"doors": [], "props": [
				{"sprite": "table_round", "x": 50, "y": 50,
					"condition": "not (\"test_wrecked\" in player.flags)"},
				{"sprite": "table_broken", "x": 50, "y": 50,
					"condition": "\"test_wrecked\" in player.flags"},
				{"sprite": "bar_counter", "x": 100, "y": 30},
			]},
	])
	var names := func() -> Array:
		return world._visible_props.map(func(p: Dictionary) -> String: return p.sprite)
	assert_has(names.call(), "table_round", "intact bar before the fight")
	assert_does_not_have(names.call(), "table_broken")
	GameState.set_flag("test_wrecked")
	assert_has(names.call(), "table_broken", "wreckage after the flag flips")
	assert_does_not_have(names.call(), "table_round")
	assert_has(names.call(), "bar_counter", "unconditional props persist")
	GameState.clear_flag("test_wrecked")
