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
	# Both droids are mag-locked, but their chassis differ: one walks
	# zero-G at full clip and crawls under gravity, the other mag-walks
	# slowly everywhere weightless. Organics drift.
	var fast_zero_g := ""
	var slow_zero_g := ""
	var walker := ""
	for npc_id in DataRegistry.ids("npcs"):
		var npc := DataRegistry.get_entity("npcs", npc_id)
		var locomotion: Dictionary = npc.get("locomotion", {})
		if str(locomotion.get("zero_g", "")) == "magnetic":
			if float(locomotion.get("zero_g_speed_mult", 1.0)) >= 0.99:
				fast_zero_g = npc_id
			else:
				slow_zero_g = npc_id
		elif npc.get("aboard", false) and walker == "":
			walker = npc_id
	assert_ne(fast_zero_g, "", "one chassis owns the upper deck")
	assert_ne(slow_zero_g, "", "one chassis mag-walks it slowly")
	assert_true(_interior._is_magnetic(fast_zero_g))
	assert_true(_interior._is_magnetic(slow_zero_g))
	assert_false(_interior._is_magnetic(walker), "organics drift unless equipped")
	assert_lt(_interior._gravity_speed_mult(fast_zero_g), 0.7,
		"the deck chassis crawls under gravity")
	assert_lt(_interior._zero_g_speed_mult(slow_zero_g), 0.7,
		"the nav chassis mag-walks zero-G at half speed")


func test_flight_suit_flag_makes_the_player_magnetic() -> void:
	var walker := ""
	for npc_id in DataRegistry.ids("npcs"):
		var npc := DataRegistry.get_entity("npcs", npc_id)
		if npc.get("aboard", false) \
				and str(npc.get("locomotion", {}).get("zero_g", "drift")) == "drift":
			walker = npc_id
			break
	assert_ne(walker, "", "an organic crew member exists")
	GameState.set_player_character(walker)
	assert_false(_interior._is_magnetic(walker), "bare boots drift")
	GameState.set_flag("flight_suit_on")
	assert_true(_interior._is_magnetic(walker), "the flight suit's mag-soles bite")
	GameState.clear_flag("flight_suit_on")
	assert_false(_interior._is_magnetic(walker), "racking the suit hands zero-G back")


func test_ladder_sprite_is_stamped_into_the_rooms() -> void:
	var stamped := 0
	for room: Dictionary in _interior._rooms:
		for prop: Dictionary in room.props:
			if prop.get("sprite", "") == "ladder":
				stamped += 1
	assert_eq(stamped, 2, "one visible ladder end per deck")


func test_rescuer_candidate_is_the_fastest_magnetic_crew() -> void:
	var rescuer := _interior._rescuer_candidate()
	assert_false(rescuer.is_empty(), "someone aboard can walk out and get you")
	var locomotion: Dictionary = DataRegistry.get_entity("npcs", rescuer.id).get("locomotion", {})
	assert_eq(str(locomotion.get("zero_g", "")), "magnetic")
	assert_almost_eq(float(locomotion.get("zero_g_speed_mult", 1.0)), 1.0, 0.01,
		"the fast chassis gets the job")


func test_flight_suit_locker_is_a_station_in_the_hull() -> void:
	var found := false
	for s: Dictionary in _interior._stations:
		if s.id == "flight_suit":
			found = true
			assert_eq(str(s.deck), "lower", "the suit hangs below decks — that's the lesson")
	assert_true(found, "the hull ships a flight suit locker")


func test_repair_crew_exists_in_data() -> void:
	var repairers := 0
	for npc_id in DataRegistry.ids("npcs"):
		if DataRegistry.get_entity("npcs", npc_id).get("repairs", false):
			repairers += 1
	assert_gt(repairers, 0, "someone aboard runs damage control on their own")
