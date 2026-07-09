extends GutTest
## Contract tests: the two-deck ship (Sprint 3) — decks parse from hull data,
## each deck carries its own gravity, a conduit fault cuts a deck's plates,
## the ladder connects the decks, and locomotion rules come from npc data
## (a mag-locked chassis walks zero-G; a heavy one crawls under gravity).

const ShipInteriorScene := preload("res://scenes/framework/ship_interior.tscn")


var _interior: ShipInterior


func before_each() -> void:
	GameState.player["character"] = ""
	GameState.player["upgrades"] = []
	GameState.player.ship["damage"] = []
	GameState.player.ship["damage_seq"] = 0
	_interior = ShipInteriorScene.instantiate()
	add_child_autofree(_interior)
	_interior.configure(DataRegistry.get_entity("ships", GameState.player.ship.hull_id))


func after_each() -> void:
	GameState.player["character"] = ""
	GameState.player.ship["damage"] = []


func test_player_wakes_on_the_grav_deck() -> void:
	assert_eq(_interior.current_deck(), "lower",
		"the cryopod — and the player's first steps — are on the lower deck")


func test_decks_carry_their_own_gravity() -> void:
	assert_almost_eq(_interior.deck_gravity_strength("lower"), 1.0, 0.01,
		"grav plates hold the lower deck down")
	assert_almost_eq(_interior.deck_gravity_strength("upper"), 0.0, 0.01,
		"the upper deck runs cold and weightless")


func test_a_conduit_fault_cuts_the_plates() -> void:
	# Find a lower-deck room straight from the hull data.
	var hull := DataRegistry.get_entity("ships", GameState.player.ship.hull_id)
	var lower_room := ""
	for room: Dictionary in hull.get("rooms", []):
		if room.get("deck", "") == "lower":
			lower_room = room.get("id", "")
			break
	assert_ne(lower_room, "", "the hull has lower-deck rooms")
	GameState.add_ship_damage(lower_room, "conduit", [250.0, 250.0])
	assert_almost_eq(_interior.deck_gravity_strength("lower"), 0.0, 0.01,
		"an arcing conduit drops the deck's gravity until fixed")
	GameState.repair_ship_damage(int(GameState.ship_damage()[0].id))
	assert_almost_eq(_interior.deck_gravity_strength("lower"), 1.0, 0.01,
		"repair brings the plates back")


func test_ladder_connects_the_decks() -> void:
	var found := false
	for it: Dictionary in _interior._interactables:
		if it.kind == "ladder":
			found = true
	assert_true(found, "the main ladder is an interactable on the player's deck")


func test_locomotion_rules_come_from_npc_data() -> void:
	# Find the mag-locked crew member (locomotion.zero_g: magnetic).
	var magnetic := ""
	var walker := ""
	for npc_id in DataRegistry.ids("npcs"):
		var locomotion: Dictionary = DataRegistry.get_entity("npcs", npc_id).get("locomotion", {})
		if str(locomotion.get("zero_g", "")) == "magnetic":
			magnetic = npc_id
		elif DataRegistry.get_entity("npcs", npc_id).get("aboard", false) and walker == "":
			walker = npc_id
	assert_ne(magnetic, "", "the demo content ships a mag-locked droid")
	assert_true(_interior._is_magnetic(magnetic))
	assert_false(_interior._is_magnetic(walker), "organics drift unless they buy boots")
	assert_lt(_interior._gravity_speed_mult(magnetic), 0.7,
		"the heavy chassis crawls under gravity")


func test_repair_crew_exists_in_data() -> void:
	var repairers := 0
	for npc_id in DataRegistry.ids("npcs"):
		if DataRegistry.get_entity("npcs", npc_id).get("repairs", false):
			repairers += 1
	assert_gt(repairers, 0, "someone aboard runs damage control on their own")
