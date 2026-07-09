extends GutTest
## Contract test: CharacterSprite — the one way a person is drawn.
## Direction picks the sheet row; missing art falls back without crashing.


func test_direction_picks_sheet_row() -> void:
	var sprite := CharacterSprite.new()
	add_child_autofree(sprite)
	sprite.setup("npcs", "test_nobody", Color.GRAY)
	sprite.set_motion(Vector2.RIGHT, true)
	assert_eq(sprite.facing_row(), CharacterSprite.ROWS.right)
	sprite.set_motion(Vector2.UP, true)
	assert_eq(sprite.facing_row(), CharacterSprite.ROWS.up)
	sprite.set_motion(Vector2(0.3, 0.9), true)
	assert_eq(sprite.facing_row(), CharacterSprite.ROWS.down, "dominant axis wins")
	sprite.set_motion(Vector2.ZERO, false)
	assert_eq(sprite.facing_row(), CharacterSprite.ROWS.down, "stopping keeps the last facing")


func test_missing_sheet_falls_back_quietly() -> void:
	var sprite := CharacterSprite.new()
	add_child_autofree(sprite)
	sprite.setup("npcs", "definitely_not_a_character", Color.GRAY, "Nobody")
	assert_false(sprite.has_sheet(), "no art shipped for this id")
	sprite.set_motion(Vector2.LEFT, true)  # must not crash without a sheet
	assert_eq(sprite.facing_row(), CharacterSprite.ROWS.left)


func test_crew_sheets_shipped_for_every_aboard_character() -> void:
	# The art pass promise: every crew member the roster can put on screen
	# has a walk-cycle sheet (artists replace the PNG; the file must exist).
	for crew_id: String in CrewRoster.aboard():
		var sprite := CharacterSprite.new()
		add_child_autofree(sprite)
		sprite.setup("npcs", crew_id, Color.GRAY)
		assert_true(sprite.has_sheet(), "%s has a sprite sheet" % crew_id)
