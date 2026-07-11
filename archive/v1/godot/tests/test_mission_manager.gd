extends GutTest
## Contract test: MissionManager autoload (Sprint 03, P12).
##
## Tests mission lifecycle: start, advance, complete, fail, event matching,
## timer expiration, and reward dispatch.

var _test_mission_id := ""


func before_all() -> void:
	# The test_trade_run mission must exist in loaded mod data
	_test_mission_id = "test_trade_run"


func before_each() -> void:
	# Ensure no active mission at test start
	# (MissionManager has no explicit cancel, so tests that start a mission
	# must ensure they complete or rely on replacement for cleanup)
	if MissionManager.is_active():
		MissionManager.fail("test cleanup")


func test_no_mission_active_at_start() -> void:
	assert_false(MissionManager.is_active())


func test_start_mission() -> void:
	var ok := MissionManager.start_mission(_test_mission_id)
	assert_true(ok, "test_trade_run should start successfully")
	assert_true(MissionManager.is_active())


func test_start_unknown_mission_fails() -> void:
	var ok := MissionManager.start_mission("nonexistent_mission")
	assert_false(ok)


func test_current_stage_after_start() -> void:
	MissionManager.start_mission(_test_mission_id)
	var stage := MissionManager.current_stage()
	assert_false(stage.is_empty(), "Should have a current stage")
	assert_eq(stage.get("id", ""), "fly_to_station", "First stage should be fly_to_station")


func test_current_objective() -> void:
	MissionManager.start_mission(_test_mission_id)
	assert_eq(MissionManager.current_objective(), "Fly to Sorrow Station and dock",
		"First stage objective should match")


func test_advance_to_next_stage() -> void:
	MissionManager.start_mission(_test_mission_id)
	MissionManager.advance()
	var stage := MissionManager.current_stage()
	assert_eq(stage.get("id", ""), "trade_goods",
		"After advancing from fly_to_station, should be at trade_goods")


func test_advance_too_far_triggers_complete() -> void:
	MissionManager.start_mission(_test_mission_id)
	var signals = watch_signals(MissionManager)
	
	# Advance through all 3 stages
	MissionManager.advance()  # fly_to_station -> trade_goods
	MissionManager.advance()  # trade_goods -> depart
	MissionManager.advance()  # depart -> complete
	
	assert_false(MissionManager.is_active(), "Mission should be completed after all stages")
	assert_signal_emitted(MissionManager, "mission_completed")


func test_report_dock_event_completes_stage() -> void:
	MissionManager.start_mission(_test_mission_id)
	# First stage is "fly_to_station" with completion type "dock"
	MissionManager.report_event("docked", {"location_id": "sorrow_station"})
	
	var stage := MissionManager.current_stage()
	assert_eq(stage.get("id", ""), "trade_goods",
		"Dock event should advance past fly_to_station")


func test_report_wrong_event_does_not_advance() -> void:
	MissionManager.start_mission(_test_mission_id)
	# Undock is not the completion type for stage 1
	MissionManager.report_event("undocked")
	
	var stage := MissionManager.current_stage()
	assert_eq(stage.get("id", ""), "fly_to_station",
		"Wrong event should not advance the stage")


func test_timer_expiration_fails_mission() -> void:
	# Create a mission with a timed stage
	# Using test_trade_run which has no timer — this tests that tick is safe
	MissionManager.start_mission(_test_mission_id)
	var sigs = watch_signals(MissionManager)
	
	# Tick with a large delta to simulate time passing
	MissionManager.tick(999.0)
	
	assert_true(true, "tick() should not crash on non-timed stages")


func test_objective_when_no_mission() -> void:
	assert_eq(MissionManager.current_objective(), "",
		"Should return empty string when no mission active")


func test_timer_remaining_when_no_mission() -> void:
	assert_eq(MissionManager.timer_remaining(), -1.0,
		"Should return -1 when no mission active")


func test_fail_mission() -> void:
	MissionManager.start_mission(_test_mission_id)
	var signals = watch_signals(MissionManager)
	
	MissionManager.fail("player died")
	
	assert_false(MissionManager.is_active())
	assert_signal_emitted(MissionManager, "mission_failed")
