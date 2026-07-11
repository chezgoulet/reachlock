extends RefCounted
## Ring 0 — weave resolution: the allowlist customs desk for `woven`
## dialogue nodes (godot/framework/WEAVE-CONTRACT.md).
##
## THE SEMANTICS ARE DEFINED BY scripts/check_weave_contract.py (the
## reference resolver and its golden + adversarial fixtures); this
## implementation must agree with it on every fixture — the trigger-DSL
## bridge pattern. A proposal that exceeds its grants is clamped or
## neutered, never rejected wholesale; a proposal that fails the shape
## check is discarded whole and the node plays its authored fallback.
##
## Numbers stay floats throughout (fixtures arrive via JSON, where every
## number is a float); GameState int()s amounts at apply time.

class_name WeaveLoom

const DEFAULT_MAX_CHOICES := 3
const DEFAULT_MAX_MUTATIONS := 4


## Resolve a mind's proposal against a woven node's `may` allowlist.
## Returns {line, mutations, choices} — choices carrying text/mutations/goto
## — or empty {} for discard-whole. Every clamp and drop is logged: an
## over-reaching provider must be visible in transcripts, invisible to
## the player.
static func resolve(node: Dictionary, proposal: Variant) -> Dictionary:
	if not _proposal_shape_ok(proposal):
		print("weave: discarded — proposal fails the proposal shape")
		return {}
	var may: Dictionary = node.get("may", {})
	var grants: Array = may.get("grants", [])
	var max_choices := int(may.get("max_choices", DEFAULT_MAX_CHOICES))
	var max_mutations := int(may.get("max_mutations", DEFAULT_MAX_MUTATIONS))
	var return_to: String = node.get("return_to", "end")

	var choices: Array = (proposal.get("choices", []) as Array).duplicate()
	if choices.size() > max_choices:
		print("weave: dropped %d choice(s) past max_choices" % (choices.size() - max_choices))
		choices = choices.slice(0, max_choices)

	# One shared budget across the node and every surviving choice, spent
	# in array order. (No lambda here: GDScript closures capture locals by
	# value, so a captured counter would silently reset.)
	var budget: Array = [max_mutations]
	var resolved_mutations: Array = _take(proposal.get("mutations", []), grants, budget)
	var resolved_choices: Array = []
	for choice: Dictionary in choices:
		resolved_choices.append({
			"text": choice.get("text", ""),
			"mutations": _take(choice.get("mutations", []), grants, budget),
			"goto": return_to,
		})
	return {
		"line": proposal.get("line", ""),
		"mutations": resolved_mutations,
		"choices": resolved_choices,
	}


## The engine-side shape check standing in for the proposal schema (the
## schema itself is CI's job; at runtime we verify the same constraints).
static func _proposal_shape_ok(proposal: Variant) -> bool:
	if not proposal is Dictionary:
		return false
	if str(proposal.get("line", "")).strip_edges() == "":
		return false
	for key: String in proposal.keys():
		if key not in ["line", "mutations", "choices"]:
			return false
	if not proposal.get("mutations", []) is Array or not proposal.get("choices", []) is Array:
		return false
	for mutation: Variant in proposal.get("mutations", []):
		if not _mutation_shape_ok(mutation):
			return false
	for choice: Variant in proposal.get("choices", []):
		if not choice is Dictionary or str(choice.get("text", "")).strip_edges() == "":
			return false
		for key: String in choice.keys():
			if key not in ["text", "mutations"]:
				return false
		for mutation: Variant in choice.get("mutations", []):
			if not _mutation_shape_ok(mutation):
				return false
	return true


static func _mutation_shape_ok(mutation: Variant) -> bool:
	if not mutation is Dictionary:
		return false
	match str(mutation.get("op", "")):
		"adjust_relationship":
			return mutation.has("target") and mutation.has("axis") and mutation.has("amount")
		"adjust_faction":
			return mutation.has("faction") and mutation.has("axis") and mutation.has("amount")
		"set_flag", "clear_flag", "set_player_flag", "clear_player_flag":
			return mutation.has("flag")
		"add_memory":
			return mutation.has("text")
		_:
			return false


## Filter one mutation list through the grants, then spend the shared
## max_mutations budget (a one-element array so it mutates across calls).
static func _take(mutations: Array, grants: Array, budget: Array) -> Array:
	var kept := _filter_mutations(mutations, grants)
	var remaining := int(budget[0])
	if kept.size() > remaining:
		print("weave: dropped %d mutation(s) past max_mutations" % (kept.size() - remaining))
		kept = kept.slice(0, remaining)
	budget[0] = remaining - kept.size()
	return kept


static func _filter_mutations(mutations: Array, grants: Array) -> Array:
	var kept: Array = []
	for mutation: Dictionary in mutations:
		var permitted := false
		for grant: Dictionary in grants:
			var clamped := _grant_permits(grant, mutation)
			if not clamped.is_empty():
				if clamped != mutation:
					print("weave: clamped %s: %s -> %s" % [mutation.get("op"), mutation, clamped])
				kept.append(clamped)
				permitted = true
				break
		if not permitted:
			print("weave: dropped %s: no grant permits %s" % [mutation.get("op"), mutation])
	return kept


## Does this grant permit this mutation? Returns the clamped mutation, or
## {} when the grant does not apply. Mirrors the reference `_grant_permits`.
static func _grant_permits(grant: Dictionary, mutation: Dictionary) -> Dictionary:
	var op: String = mutation.get("op", "")
	if grant.get("op", "") != op:
		return {}
	match op:
		"adjust_relationship":
			if mutation.get("target") not in grant.get("targets", []):
				return {}
			if mutation.get("axis") not in grant.get("axes", []):
				return {}
			var clamped: Dictionary = mutation.duplicate()
			var cap := float(grant.get("max_amount", 0))
			clamped["amount"] = clampf(float(mutation.get("amount", 0)), -cap, cap)
			return clamped
		"adjust_faction":
			if mutation.get("faction") not in grant.get("factions", []):
				return {}
			if mutation.get("axis") not in grant.get("axes", []):
				return {}
			var clamped: Dictionary = mutation.duplicate()
			var cap := float(grant.get("max_amount", 0))
			clamped["amount"] = clampf(float(mutation.get("amount", 0)), -cap, cap)
			return clamped
		"set_flag", "clear_flag", "set_player_flag", "clear_player_flag":
			if mutation.get("flag") not in grant.get("flags", []):
				return {}
			return mutation.duplicate()
		"add_memory":
			var clamped: Dictionary = mutation.duplicate()
			var cap := float(grant.get("max_importance", 0.0))
			clamped["importance"] = minf(float(mutation.get("importance", 0.5)), cap)
			return clamped
	return {}
