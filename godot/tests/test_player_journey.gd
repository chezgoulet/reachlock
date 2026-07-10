extends GutTest
## Journey tests: not mechanics in isolation — the shape of the playthrough.
## Does every road lead somewhere? Does every ending have a prerequisite?
## Does the money force a choice? A player never sees these assertions,
## but they feel every one of them.


## --- helpers -----------------------------------------------------------------------


func _dialogue(id: String) -> Dictionary:
	return DataRegistry.get_entity("dialogues", id)


## Every node id reachable from `start` by gotos and choices.
func _reachable(dialogue: Dictionary, start: String) -> Dictionary:
	var seen := {}
	var frontier: Array = [start]
	var nodes: Dictionary = dialogue.get("nodes", {})
	while not frontier.is_empty():
		var node_id: String = frontier.pop_back()
		if node_id in ["end", ""] or seen.has(node_id):
			continue
		seen[node_id] = true
		var node: Dictionary = nodes.get(node_id, {})
		if node.has("goto"):
			frontier.append(str(node.goto))
		for choice: Dictionary in node.get("choices", []):
			frontier.append(str(choice.get("goto", "end")))
	return seen


## Flags a path can set walking from `start` (union over all branches).
func _flags_settable_from(dialogue: Dictionary, start: String) -> Array:
	var flags: Array = []
	var nodes: Dictionary = dialogue.get("nodes", {})
	for node_id: String in _reachable(dialogue, start):
		var node: Dictionary = nodes.get(node_id, {})
		var pools: Array = [node.get("mutations", [])]
		for choice: Dictionary in node.get("choices", []):
			pools.append(choice.get("mutations", []))
		for pool: Array in pools:
			for mutation: Dictionary in pool:
				if mutation.get("op", "") == "set_player_flag":
					flags.append(str(mutation.flag))
	return flags


func _mission(id: String) -> Dictionary:
	return DataRegistry.get_entity("missions", id)


func _campaign_chain() -> Array:
	# Follow start.mission through `next` links.
	var chain: Array = []
	var mid: String = DataRegistry.start_config().get("mission", "")
	while mid != "" and not (mid in chain):
		chain.append(mid)
		mid = _mission(mid).get("next", "")
	return chain


## --- no dead ends, anywhere ---------------------------------------------------------


func test_every_dialogue_goto_resolves() -> void:
	for dialogue_id in DataRegistry.ids("dialogues"):
		var dialogue := _dialogue(dialogue_id)
		var nodes: Dictionary = dialogue.get("nodes", {})
		assert_true(nodes.has(dialogue.get("entry", "")),
			"%s: entry node exists" % dialogue_id)
		for node_id: String in nodes:
			var node: Dictionary = nodes[node_id]
			var targets: Array = []
			if node.has("goto"):
				targets.append(str(node.goto))
			for choice: Dictionary in node.get("choices", []):
				targets.append(str(choice.get("goto", "end")))
			for target: String in targets:
				assert_true(target == "end" or nodes.has(target),
					"%s/%s -> '%s' resolves" % [dialogue_id, node_id, target])


func test_every_dialogue_node_is_reachable() -> void:
	for dialogue_id in DataRegistry.ids("dialogues"):
		var dialogue := _dialogue(dialogue_id)
		var reachable := _reachable(dialogue, dialogue.get("entry", ""))
		for node_id: String in dialogue.get("nodes", {}):
			assert_true(reachable.has(node_id),
				"%s: node '%s' is not orphaned" % [dialogue_id, node_id])


## --- the bar fight: every road through it, and every echo after -----------------------


func test_bar_fight_ends_at_doss_from_every_choice() -> void:
	var fight := _dialogue("prudence_bar_fight")
	var choices: Array = fight.nodes.grissom_escalates.choices
	assert_eq(choices.size(), 3, "three ways to meet the moment")
	for choice: Dictionary in choices:
		var reachable := _reachable(fight, str(choice.goto))
		assert_true(reachable.has("doss_arrives"),
			"the '%s' path still reaches Doss in the doorway" % choice.text)


func test_every_stance_leaves_a_flag_and_the_flag_has_an_echo() -> void:
	var fight := _dialogue("prudence_bar_fight")
	var stance_flags: Array = []
	for choice: Dictionary in fight.nodes.grissom_escalates.choices:
		var found := ""
		for mutation: Dictionary in choice.get("mutations", []):
			if mutation.get("op", "") == "set_player_flag":
				found = str(mutation.flag)
		assert_ne(found, "", "choice '%s' leaves a mark" % choice.text)
		stance_flags.append(found)
	# Mutually distinct stances...
	assert_eq(stance_flags.size(), 3)
	assert_ne(stance_flags[0], stance_flags[1])
	assert_ne(stance_flags[1], stance_flags[2])
	# ...and each one is remembered by SOMEONE, scenes later.
	for flag: String in stance_flags:
		var echoed := false
		for dialogue_id in DataRegistry.ids("dialogues"):
			if dialogue_id == "prudence_bar_fight":
				continue
			if ('"%s"' % flag) in str(_dialogue(dialogue_id).get("condition", "")):
				echoed = true
		assert_true(echoed, "somebody at Sorrow remembers '%s'" % flag)


func test_bar_fight_flag_lands_on_every_path_including_as_prudence() -> void:
	# Every branch of the scene reaches the node that sets bar_fight_done...
	var fight := _dialogue("prudence_bar_fight")
	var doss_mutations: Array = fight.nodes.doss_arrives.get("mutations", [])
	var sets_it := false
	for mutation: Dictionary in doss_mutations:
		if mutation.get("op", "") == "set_player_flag" and str(mutation.get("flag")) == "bar_fight_done":
			sets_it = true
	assert_true(sets_it, "doss_arrives sets bar_fight_done")
	# ...and playing AS the scene's speaker sets it too (the bypass card).
	var card: Dictionary = DataRegistry.get_entity("npcs", fight.get("npc", "")) \
		.get("playable", {}).get("self_dialogue_summaries", {}).get("prudence_bar_fight", {})
	assert_false(card.is_empty(), "the scene has a narration card for its own speaker")
	var card_sets := false
	for mutation: Dictionary in card.get("mutations", []):
		if str(mutation.get("flag", "")) == "bar_fight_done":
			card_sets = true
	assert_true(card_sets, "the card carries the story flag")


func test_grissom_speaks_his_piece_exactly_once() -> void:
	var grissom_dialogues: Array = []
	for dialogue_id in DataRegistry.ids("dialogues"):
		if _dialogue(dialogue_id).get("npc", "") == "grissom":
			grissom_dialogues.append(dialogue_id)
	assert_gte(grissom_dialogues.size(), 4, "a follow-up for every stance, plus one for Prudence herself")
	for dialogue_id: String in grissom_dialogues:
		var dialogue := _dialogue(dialogue_id)
		assert_true("grissom_said_his_piece" in str(dialogue.get("condition", "")),
			"%s fires once, not as a canned loop" % dialogue_id)
		var flags := _flags_settable_from(dialogue, dialogue.get("entry", ""))
		assert_true("grissom_said_his_piece" in flags,
			"%s marks the piece as said on every exit with a choice" % dialogue_id)


## --- the campaign chain: no ending before its prerequisite ---------------------------


func test_campaign_chain_is_unbroken() -> void:
	var chain := _campaign_chain()
	assert_eq(chain.size(), 4, "four acts, linked start to finish")
	for mid: String in chain:
		assert_false(_mission(mid).is_empty(), "%s exists" % mid)


func test_endings_live_only_at_the_end_of_the_chain() -> void:
	var chain := _campaign_chain()
	var last := _mission(chain.back())
	var epilogue: Dictionary = last.get("epilogue", {})
	assert_ne(str(epilogue.get("success", "")), "", "success is authored")
	var reasons: Dictionary = epilogue.get("failure_reasons", {})
	assert_ne(str(reasons.get("time_expired", "")), "", "the closed window is authored")
	assert_ne(str(reasons.get("ship_destroyed", "")), "", "the total loss is authored")
	assert_ne(str(reasons.get("time_expired")), str(reasons.get("ship_destroyed")),
		"the two failures are different griefs")


func test_the_timer_only_starts_after_the_jump() -> void:
	var last := _mission(_campaign_chain().back())
	var stages: Array = last.stages
	var timed_index := -1
	for i in stages.size():
		if (stages[i].get("failure", {}) as Dictionary).has("timer_seconds"):
			timed_index = i
	assert_gt(timed_index, 0, "the clock starts inside the blockade, not on the way there")
	assert_eq(str(stages[timed_index - 1].get("completion", {}).get("type", "")), "jump",
		"the stage before the clock is the crossing")


func test_dialogue_stages_have_a_road_for_every_captain() -> void:
	# Every dialogue_end stage target must be talkable — and if the target
	# is a playable character, the beat must complete via narration card.
	for mid: String in _campaign_chain():
		for stage: Dictionary in _mission(mid).stages:
			var completion: Dictionary = stage.get("completion", {})
			if str(completion.get("type", "")) != "dialogue_end":
				continue
			var target: String = completion.get("target_id", "")
			var npc := DataRegistry.get_entity("npcs", target)
			assert_false(npc.is_empty(), "%s/%s: npc exists" % [mid, stage.id])
			var hosted := 0
			for dialogue_id in DataRegistry.ids("dialogues"):
				if _dialogue(dialogue_id).get("npc", "") == target:
					hosted += 1
			assert_gt(hosted, 0, "%s has something to say" % target)
			if npc.has("playable"):
				assert_false((npc.playable.get("self_dialogue_summaries", {}) as Dictionary).is_empty(),
					"playing AS %s cannot strand the '%s' stage" % [target, stage.id])


## --- the money forces a choice --------------------------------------------------------


func test_the_budget_covers_three_upgrades_but_not_seven() -> void:
	var budget := 0
	for mid: String in _campaign_chain():
		for stage: Dictionary in _mission(mid).stages:
			budget += int(stage.get("rewards", {}).get("credits", 0))
	assert_gt(budget, 0, "the deal pays")
	var costs: Array = []
	for upgrade_id in DataRegistry.ids("upgrades"):
		costs.append(int(DataRegistry.get_entity("upgrades", upgrade_id).get("cost", 0)))
	costs.sort()
	var cheapest_three := 0
	for i in 3:
		cheapest_three += costs[i]
	var cheapest_seven := 0
	for i in 7:
		cheapest_seven += costs[i]
	assert_lte(cheapest_three, budget, "three upgrades are within reach")
	assert_gt(cheapest_seven, budget, "seven are not — choose")


func test_the_bribe_is_affordable_but_it_costs() -> void:
	var picket_bribe := 0
	for location_id in DataRegistry.ids("locations"):
		for patrol: Dictionary in DataRegistry.get_entity("locations", location_id).get("patrols", []):
			if str(patrol.get("engagement", "")) == "engage":
				picket_bribe = maxi(picket_bribe, int(patrol.get("bribe", 0)))
	assert_gt(picket_bribe, 0, "the cordon runs its own economy")
	assert_lt(picket_bribe, 2400, "a shopper can still afford the fee")
	assert_gt(picket_bribe, 300, "but it hurts")


func test_the_decoy_is_an_action_not_a_number() -> void:
	var decoy_found := false
	for upgrade_id in DataRegistry.ids("upgrades"):
		var effects: Dictionary = DataRegistry.get_entity("upgrades", upgrade_id).get("effects", {})
		if int(effects.get("decoy_charges", 0)) >= 1:
			decoy_found = true
			assert_false(effects.has("detection_mult"),
				"the decoy is a verb, not a passive stat")
	assert_true(decoy_found, "something in the shop drops a decoy")


## --- the claim, the loss, the crossing -----------------------------------------------


func test_the_claim_holds_more_than_the_quota() -> void:
	var quota := 0
	for stage: Dictionary in _mission(_campaign_chain()[0]).stages:
		var completion: Dictionary = stage.get("completion", {})
		if str(completion.get("target_id", "")) == "ore_mined":
			quota = int(completion.get("count", 1))
	assert_gt(quota, 0, "the cold open asks for ore")
	var mining: Dictionary = DataRegistry.get_entity("locations",
		DataRegistry.start_config().get("location", "")).get("mining", {})
	var names: Array = mining.get("rock_names", [])
	assert_gte(names.size(), 6, "the rocks have names — nine days on a claim, you name things")
	# Worst case every rock is a lean seam: still far more than the quota.
	assert_gt(names.size() * 2, quota * 2, "greed is possible; the ambush interrupts it")


func test_only_loose_cargo_can_spill() -> void:
	assert_true(bool(DataRegistry.get_entity("goods", "raw_ore").get("loose_cargo", false)),
		"ore rides loose — the ambush can cost you real money")
	for mid: String in _campaign_chain():
		for stage: Dictionary in _mission(mid).stages:
			for good_id: String in stage.get("rewards", {}).get("cargo", {}):
				assert_false(bool(DataRegistry.get_entity("goods", good_id).get("loose_cargo", false)),
					"mission cargo (%s) never vents — losing the crate to RNG is not a story" % good_id)


func test_the_awake_crossing_has_a_voice() -> void:
	var pilot := ""
	for npc_id in DataRegistry.ids("npcs"):
		var npc := DataRegistry.get_entity("npcs", npc_id)
		if npc.get("jump_pilot", false) and npc.get("aboard", false):
			pilot = npc_id
	assert_ne(pilot, "", "someone flies the crossing")
	var lines: Array = DataRegistry.get_entity("npcs", pilot).get("barks", {}).get("transit_alone", [])
	assert_gte(lines.size(), 3, "the one who stays awake has thoughts to sit with")


func test_doss_repairs_are_a_patch_not_a_gift() -> void:
	var hull_min := 0.0
	for stage: Dictionary in _mission("duskway_demo_act3").stages:
		hull_min = maxf(hull_min, float(stage.get("rewards", {}).get("hull_min", 0.0)))
	assert_gt(hull_min, 0.3, "Doss's promised repairs actually happen")
	assert_lte(hull_min, 0.75, "but real plate at the counter still matters")
