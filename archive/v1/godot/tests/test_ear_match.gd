extends GutTest
## Contract test: the GDScript choice matcher must agree with the reference
## implementation (scripts/check_ear_protocol.py) on every case in
## ear/match_cases.json — the trigger-DSL bridge pattern. No mic, no
## network, no daemon: voice matching is engine logic, tested headlessly.

const CASES_PATH := "res://framework/protocol/ear/match_cases.json"


func test_case_battery_is_present() -> void:
	var cases: Array = JSON.parse_string(FileAccess.get_file_as_string(CASES_PATH))
	assert_gt(cases.size(), 10, "the contract case battery is present")


func test_every_case_matches_the_reference_verdict() -> void:
	var cases: Array = JSON.parse_string(FileAccess.get_file_as_string(CASES_PATH))
	for case: Dictionary in cases:
		var choices: Array = []
		for choice: String in case.get("choices", []):
			choices.append(choice)
		var got := EarMatch.match_choice(case.get("transcript", ""), choices)
		assert_eq(got, int(case.get("expect", -1)),
			"%s: transcript %s" % [case.get("name", "?"), case.get("transcript", "")])


func test_verdicts_are_deterministic() -> void:
	# Same input, same verdict — run the headline case a hundred times.
	var choices := ["I'd do it again — she's crew.", "It wasn't my business.", "Buy him a drink."]
	for i in 100:
		assert_eq(EarMatch.match_choice("I'd do it again. She's crew.", choices), 0)
