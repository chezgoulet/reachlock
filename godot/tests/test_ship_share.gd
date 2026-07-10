extends GutTest
## Contract test: Ship-Share seat-claiming and intent handling
## (SHIP-SHARE.md), driven with the golden fixture payloads and zero
## network hardware — the ENet layer is a thin pipe under this core, and
## this core is the contract.

const FIXTURES_DIR := "res://framework/protocol/share/fixtures"


func before_each() -> void:
	ShipShare.stop()
	ShipShare.deliveries.clear()
	ShipOperation.reset()
	GameState.player["character"] = ""


func after_all() -> void:
	ShipShare.stop()
	ShipShare.deliveries.clear()


func _fixture(name: String) -> Dictionary:
	var parsed: Dictionary = JSON.parse_string(
		FileAccess.get_file_as_string(FIXTURES_DIR + "/" + name))
	return parsed.get("message", {})


func _sent_to(peer: int) -> Array:
	var out: Array = []
	for delivery: Dictionary in ShipShare.deliveries:
		if int(delivery.peer) == peer:
			out.append(delivery.payload)
	return out


func _last_kind_to(peer: int, kind: String) -> Dictionary:
	var found := {}
	for payload: Dictionary in _sent_to(peer):
		if payload.kind == kind:
			found = payload
	return found


func _join(peer: int, player_name := "Boris") -> void:
	ShipShare.handle_intent(peer, {"kind": "hello", "body": {
		"share_version": ShipShare.SHARE_VERSION, "game_version": "test", "name": player_name}})


func test_the_golden_hello_fixture_is_welcomed() -> void:
	ShipShare.handle_intent(2, _fixture("01_hello.json"))
	var welcome := _last_kind_to(2, "welcome")
	assert_false(welcome.is_empty(), "a version-matched hello draws a welcome")
	assert_eq(int(welcome.body.share_version), ShipShare.SHARE_VERSION)
	assert_true(ShipShare.players.has(2), "the peer joined the roster")


func test_version_mismatch_is_refused_loudly() -> void:
	ShipShare.handle_intent(2, {"kind": "hello", "body": {
		"share_version": ShipShare.SHARE_VERSION + 1, "game_version": "future", "name": "Traveler"}})
	var refusal := _last_kind_to(2, "refuse")
	assert_false(refusal.is_empty(), "mismatch answers refuse, never silence")
	assert_eq(str(refusal.body.reason), "version_mismatch")
	assert_eq(int(refusal.body.host_version), ShipShare.SHARE_VERSION)
	assert_eq(int(refusal.body.yours), ShipShare.SHARE_VERSION + 1)
	assert_false(ShipShare.players.has(2), "a refused peer never joins")


func test_first_claim_wins_and_the_loser_hears_who_holds_it() -> void:
	_join(2)
	_join(3, "Vesna")
	ShipShare.handle_intent(2, _fixture("04_claim_seat.json"))  # tib
	assert_eq(int(ShipShare.seats.get("tib", -1)), 2, "first claim landed")
	assert_true(ShipShare.is_claimed("tib"), "claimed crew stop being NPCs")
	ShipShare.handle_intent(3, _fixture("04_claim_seat.json"))
	var denied := _last_kind_to(3, "seat_denied")
	assert_false(denied.is_empty(), "the race loser is told")
	assert_eq(int(denied.body.held_by), 2)
	assert_eq(int(ShipShare.seats.get("tib", -1)), 2, "the seat did not move")


func test_reseating_releases_the_previous_body() -> void:
	_join(2)
	ShipShare.handle_intent(2, {"kind": "claim_seat", "body": {"npc_id": "tib"}})
	ShipShare.handle_intent(2, {"kind": "claim_seat", "body": {"npc_id": "prudence"}})
	assert_false(ShipShare.seats.has("tib"), "one body per player")
	assert_eq(int(ShipShare.seats.get("prudence", -1)), 2)


func test_release_seat_returns_the_crew_member() -> void:
	_join(2)
	ShipShare.handle_intent(2, {"kind": "claim_seat", "body": {"npc_id": "tib"}})
	ShipShare.handle_intent(2, _fixture("05_release_seat.json"))
	assert_false(ShipShare.is_claimed("tib"), "released crew are NPCs again")


func test_disconnect_releases_the_seat() -> void:
	_join(2)
	ShipShare.handle_intent(2, {"kind": "claim_seat", "body": {"npc_id": "tib"}})
	ShipShare._on_peer_disconnected(2)
	assert_false(ShipShare.seats.has("tib"), "a dropped player frees their crew member")
	assert_false(ShipShare.players.has(2))


func test_station_intents_drive_ship_operation_as_the_claimed_crew() -> void:
	_join(2)
	ShipShare.handle_intent(2, {"kind": "claim_seat", "body": {"npc_id": "tib"}})
	ShipShare.handle_intent(2, _fixture("08_station_occupy.json"))  # engineering
	assert_eq(ShipOperation.occupied_by("engineering"), "tib",
		"the seat's crew member works the station")
	ShipShare.handle_intent(2, _fixture("09_control.json"))  # power_engines 0.5
	assert_eq(float(ShipOperation.get_control("engineering", "power_engines")), 0.5)


func test_controls_obey_only_the_hands_on_the_station() -> void:
	_join(2)
	_join(3, "Vesna")
	ShipShare.handle_intent(2, {"kind": "station", "body": {"op": "occupy", "station_id": "engineering"}})
	var before: float = float(ShipOperation.get_control("engineering", "power_engines"))
	ShipShare.handle_intent(3, {"kind": "control", "body": {
		"station_id": "engineering", "axis": "power_engines", "value": 0.9}})
	assert_eq(float(ShipOperation.get_control("engineering", "power_engines")), before,
		"a bystander cannot move someone else's controls")


func test_an_occupied_station_refuses_a_second_pair_of_hands() -> void:
	_join(2)
	_join(3, "Vesna")
	ShipShare.handle_intent(2, {"kind": "station", "body": {"op": "occupy", "station_id": "pilot"}})
	var pilot := ShipOperation.occupied_by("pilot")
	ShipShare.handle_intent(3, {"kind": "station", "body": {"op": "occupy", "station_id": "pilot"}})
	assert_eq(ShipOperation.occupied_by("pilot"), pilot, "first hands keep the stick")


func test_move_intents_fan_out_to_everyone_else() -> void:
	_join(2)
	_join(3, "Vesna")
	ShipShare.deliveries.clear()
	ShipShare.handle_intent(2, _fixture("07_move.json"))
	assert_false(_last_kind_to(3, "pawn").is_empty(), "the other player sees the move")
	assert_true(_last_kind_to(2, "pawn").is_empty(), "the mover is not echoed their own pawn")
	var pawn := _last_kind_to(3, "pawn")
	assert_eq(int(pawn.body.peer), 2, "the pawn names its player")


func test_say_reaches_the_dialogue_layer_as_plain_text() -> void:
	_join(2)
	var received: Array = []
	var handler := func(kind: String, body: Dictionary) -> void:
		received.append({"kind": kind, "body": body})
	ShipShare.dialogue_state.connect(handler)
	ShipShare.handle_intent(2, _fixture("11_say_transcript.json"))
	ShipShare.dialogue_state.disconnect(handler)
	assert_eq(received.size(), 1)
	assert_eq(str(received[0].kind), "say")
	assert_eq(str(received[0].body.text), "I'd do it again. She's crew.",
		"typed or spoken, the host cannot tell — by design")


func test_remote_pawns_render_and_bury_players() -> void:
	var pawns := RemotePawns.new()
	add_child_autofree(pawns)
	ShipShare.players[2] = {"name": "Boris", "npc_id": "tib"}
	# The client-side path: a pawn state payload arrives off the wire.
	ShipShare._apply_state({"kind": "pawn", "body": {
		"peer": 2, "position": [100.0, 50.0], "facing": "left", "anim": "walk"}})
	assert_eq(pawns.get_child_count(), 1, "a pawn exists for the other player")
	var sprite := pawns.get_child(0) as CharacterSprite
	assert_eq(sprite.facing_row(), CharacterSprite.ROWS.left, "state drives facing")
	ShipShare.players.erase(2)
	ShipShare.roster_changed.emit()
	await get_tree().process_frame
	assert_eq(pawns.get_child_count(), 0, "a departed player's pawn is buried")


func test_solo_intents_take_the_same_door() -> void:
	# A solo game is a hosted game with zero peers: the local player's
	# station intent runs through handle_intent, same rules as everyone.
	ShipShare.send_intent({"kind": "station", "body": {"op": "occupy", "station_id": "pilot"}})
	assert_eq(ShipOperation.occupied_by("pilot"), "player")
