extends GutTest
## Contract test: the Duskway Run demo campaign (Sprint 03).
##
## Drives the four chained missions end-to-end through MissionManager's
## event interface — the same events the mode scenes report in play — and
## asserts the chain advances, rewards land, timers arm, epilogues fire,
## and the campaign terminates cleanly. This is the demo's spine, testable
## without a renderer.

const ACT1 := "duskway_demo_act1"
const ACT2 := "duskway_demo_act2"
const ACT3 := "duskway_demo_act3"
const ACT4 := "duskway_demo_act4"


func before_each() -> void:
	if MissionManager.is_active():
		MissionManager.fail("test cleanup")
	GameState.clear_flag("campaign_over")
	GameState.player.credits = 200
	GameState.player.upgrades = []
	GameState.player.ship.cargo = {}
	GameState.mission = {}


func after_all() -> void:
	if MissionManager.is_active():
		MissionManager.fail("test cleanup")
	GameState.clear_flag("campaign_over")
	GameState.mission = {}


func _stage_id() -> String:
	return MissionManager.current_stage().get("id", "")


func test_full_campaign_chain() -> void:
	assert_true(MissionManager.start_mission(ACT1), "act1 should start")
	assert_eq(_stage_id(), "shift_briefing")

	MissionManager.report_event("dialogue_end", {"npc_id": "tove"})
	assert_eq(_stage_id(), "take_the_stick", "briefing done -> launch stage")

	MissionManager.report_event("launched")
	assert_eq(_stage_id(), "work_the_claim")

	MissionManager.report_event("ore_mined")
	MissionManager.report_event("ore_mined")
	assert_eq(_stage_id(), "work_the_claim", "two of three ore mined")
	assert_eq(MissionManager.event_remaining(), 1)
	MissionManager.report_event("ore_mined")
	assert_eq(_stage_id(), "survive_the_ambush")

	MissionManager.report_event("survived_ambush")
	assert_eq(_stage_id(), "limp_home")

	MissionManager.report_event("self_jump_completed", {"to": "sorrow_station"})
	# Act 1 completed; the chain starts act 2 automatically.
	assert_true(MissionManager.is_active(), "chain should continue into act2")
	assert_eq(GameState.mission.get("id", ""), ACT2)
	assert_eq(_stage_id(), "limp_in")

	MissionManager.report_event("docked", {"location_id": "sorrow_station"})
	assert_eq(_stage_id(), "the_interval")

	MissionManager.report_event("dialogue_end", {"npc_id": "prudence"})
	assert_eq(GameState.mission.get("id", ""), ACT3)
	assert_eq(_stage_id(), "see_doss")

	var credits_before: int = GameState.player.credits
	MissionManager.report_event("dialogue_end", {"npc_id": "doss"})
	assert_eq(GameState.player.credits, credits_before + 2400, "Doss's credit lands")
	assert_eq(GameState.cargo_count("medical_cache"), 1, "the McGuffin is aboard")
	assert_eq(_stage_id(), "outfit_and_lift")

	MissionManager.report_event("undocked", {"location_id": "sorrow_station"})
	assert_eq(GameState.mission.get("id", ""), ACT4)
	assert_eq(_stage_id(), "the_final_leg")

	MissionManager.report_event("self_jump_completed", {"to": "earth_landing"})
	assert_eq(_stage_id(), "run_the_cordon")
	assert_gt(MissionManager.timer_remaining(), 0.0, "the cordon clock is running")

	MissionManager.report_event("docked", {"location_id": "earth_landing"})
	assert_eq(_stage_id(), "the_handover")
	assert_eq(MissionManager.timer_remaining(), -1.0, "landing stops the clock")

	var earth_trust_before := int(GameState.faction_standing("earth_remnant").get("trust", 0))
	MissionManager.report_event("dialogue_end", {"npc_id": "noor"})
	assert_false(MissionManager.is_active(), "campaign complete")
	assert_true(GameState.has_flag("campaign_over"), "no restart on next boot")
	assert_eq(int(GameState.faction_standing("earth_remnant").get("trust", 0)),
		earth_trust_before + 40, "the Remnant remembers")


func test_cordon_timer_failure_shows_ending() -> void:
	assert_true(MissionManager.start_mission(ACT4))
	MissionManager.report_event("self_jump_completed", {"to": "earth_landing"})
	assert_eq(_stage_id(), "run_the_cordon")

	var caught: Array = []
	var handler := func(_mid: String, text: String, success: bool) -> void:
		caught.append({"text": text, "success": success})
	MissionManager.epilogue_ready.connect(handler)
	MissionManager.tick(9999.0)
	MissionManager.epilogue_ready.disconnect(handler)

	assert_false(MissionManager.is_active(), "time expired fails the run")
	assert_eq(caught.size(), 1, "a failure ending card fires")
	if caught.size() == 1:
		assert_false(caught[0].success)
		assert_string_contains(str(caught[0].text), "cordon")


func test_ship_destroyed_uses_reason_specific_ending() -> void:
	assert_true(MissionManager.start_mission(ACT4))
	MissionManager.report_event("self_jump_completed", {"to": "earth_landing"})

	var caught: Array = []
	var handler := func(_mid: String, text: String, success: bool) -> void:
		caught.append({"text": text, "success": success})
	MissionManager.epilogue_ready.connect(handler)
	MissionManager.report_event("ship_destroyed")
	MissionManager.epilogue_ready.disconnect(handler)

	assert_false(MissionManager.is_active())
	assert_eq(caught.size(), 1)
	if caught.size() == 1:
		assert_string_contains(str(caught[0].text), "Everyone dies")


func test_cryo_recalibration_buys_cordon_time() -> void:
	assert_true(MissionManager.start_mission(ACT4))
	MissionManager.report_event("self_jump_completed", {"to": "earth_landing"})
	var base_timer := MissionManager.timer_remaining()
	MissionManager.fail("test cleanup")

	GameState.add_upgrade("cryo_recalibration")
	assert_true(MissionManager.start_mission(ACT4))
	MissionManager.report_event("self_jump_completed", {"to": "earth_landing"})
	assert_almost_eq(MissionManager.timer_remaining(), base_timer + 60.0, 1.0,
		"recalibrated pods buy sixty seconds on the far side")
	MissionManager.fail("test cleanup")


func test_upgrade_effects_aggregate() -> void:
	GameState.add_upgrade("transponder_ghost")
	GameState.add_upgrade("decoy_beacon")
	assert_almost_eq(GameState.upgrade_effect_product("detection_mult"), 0.55 * 0.85, 0.001)
	assert_true(GameState.has_flag("upgrade_transponder_ghost"),
		"purchases surface as player flags for dialogue guards")


func test_mission_progress_survives_persistence_roundtrip() -> void:
	assert_true(MissionManager.start_mission(ACT1))
	MissionManager.report_event("dialogue_end", {"npc_id": "tove"})
	MissionManager.report_event("launched")
	MissionManager.report_event("ore_mined")
	var block: Dictionary = GameState.mission.duplicate(true)
	assert_eq(block.get("id", ""), ACT1)
	assert_eq(int(block.get("stage_index", -1)), 2)
	assert_eq(int(block.get("event_counts", {}).get("ore_mined", 0)), 1)

	# Simulate a reload: wipe the live state, restore from the block.
	MissionManager.fail("test cleanup")
	GameState.mission = block
	GameState.universe_loaded.emit()
	assert_true(MissionManager.is_active(), "mission restored from save block")
	assert_eq(_stage_id(), "work_the_claim")
	assert_eq(MissionManager.event_remaining(), 2, "counted progress restored")
	MissionManager.fail("test cleanup")
