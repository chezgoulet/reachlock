extends RefCounted
## Ring 0 — the `reachlock/0` profile of the Soul Protocol: the capability set
## the host registers at connect, and how a soul file renders into context
## fragments. Capability ids are framework vocabulary (like `npc.move_to`),
## never content ids; the soul data flowing through here is opaque to the
## engine — it comes from mods via DataRegistry.


## The v0 capability set (SOUL-PROTOCOL.md, "The REACHLOCK profile").
## Mirrors fixture 03; the daemon's validate stage rejects anything else.
static func capabilities() -> Array:
	return [
		{
			"id": "npc.move_to",
			"summary": "Walk to a room on the current ship or station.",
			"args_schema": {"type": "object", "required": ["room"], "properties": {"room": {"type": "string"}}},
		},
		{
			"id": "npc.set_task",
			"summary": "Adopt a job: repair, cook, guard, rest.",
			"args_schema": {"type": "object", "required": ["task"], "properties": {"task": {"type": "string"}}},
		},
		{
			"id": "npc.adjust_relationship",
			"summary": "Shift how you feel about someone, on one axis. Governed state-write.",
			"args_schema": {"type": "object", "required": ["toward", "axis", "amount"], "properties": {"toward": {"type": "string"}, "axis": {"type": "string"}, "amount": {"type": "number"}}},
		},
		{
			"id": "npc.remember",
			"summary": "Commit a memory. Goes through the governed path to the memory store.",
			"args_schema": {"type": "object", "required": ["text"], "properties": {"text": {"type": "string"}, "importance": {"type": "number"}, "tags": {"type": "array", "items": {"type": "string"}}}},
		},
		{
			"id": "npc.leave_crew",
			"summary": "Leave the player's crew for good. Governed; use only at a true breaking point.",
			"args_schema": {"type": "object", "required": ["reason"], "properties": {"reason": {"type": "string"}}},
		},
	]


## Render a soul file's birth-state into the `persona` context fragment.
## First person, present tense — this is who the soul wakes up as.
static func persona_fragment(soul: Dictionary) -> String:
	var lines: PackedStringArray = []
	var name: String = soul.get("name", soul.get("id", "someone"))
	var role: String = soul.get("role", "")
	if role != "":
		lines.append("You are %s, the %s." % [name, role.replace("_", " ")])
	else:
		lines.append("You are %s." % name)
	if soul.get("description", "") != "":
		lines.append(str(soul.description))
	var personality: Dictionary = soul.get("personality", {})
	if personality.get("traits", []).size() > 0:
		lines.append("You are " + ", ".join(PackedStringArray(personality.traits)) + ".")
	if personality.get("values", []).size() > 0:
		lines.append("You care about: " + ", ".join(PackedStringArray(personality.values)).replace("_", " ") + ".")
	if personality.get("fears", []).size() > 0:
		lines.append("You fear: " + ", ".join(PackedStringArray(personality.fears)).replace("_", " ") + ".")
	lines.append("Stay in character. Speak briefly and naturally — one to three sentences. Never mention being an AI.")
	return "\n".join(lines)


## Render authored memory seeds (soul schema v1) as a starting memory
## fragment, until the memory store takes over recall.
static func seed_memory_fragment(soul: Dictionary) -> String:
	var seeds: Array = soul.get("memory_seeds", [])
	if seeds.is_empty():
		return ""
	var lines: PackedStringArray = []
	for seed: Dictionary in seeds:
		lines.append("- " + str(seed.get("text", "")))
	return "You remember:\n" + "\n".join(lines)
