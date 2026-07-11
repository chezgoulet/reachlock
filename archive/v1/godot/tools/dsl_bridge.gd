extends SceneTree
## Ring 0 tooling — headless half of the trigger-DSL conformance bridge.
##
## Runs the Python reference battery (scripts/trigger_dsl.py) through the
## in-engine evaluator (scripts/framework/trigger_dsl.gd) and writes the
## outcomes as JSON. The Python driver (scripts/check_dsl_bridge.py) emits
## the battery, invokes this script headless, and diffs the outcomes against
## the reference. Contract: booleans must match exactly; battery cases the
## reference rejects (ParseError/EvalError) must error in strict mode here.
##
## Invocation (from the repo root):
##   godot --headless --path godot/ --script res://tools/dsl_bridge.gd \
##     -- --battery=<abs path in> --out=<abs path out>

const TriggerDSLScript := preload("res://scripts/framework/trigger_dsl.gd")


func _init() -> void:
	var battery_path := ""
	var out_path := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--battery="):
			battery_path = arg.trim_prefix("--battery=")
		elif arg.begins_with("--out="):
			out_path = arg.trim_prefix("--out=")
	if battery_path == "" or out_path == "":
		push_error("dsl_bridge: need -- --battery=<path> --out=<path>")
		quit(2)
		return

	var raw := FileAccess.get_file_as_string(battery_path)
	var parsed: Variant = JSON.parse_string(raw)
	if not parsed is Dictionary:
		push_error("dsl_bridge: battery at %s is not valid JSON" % battery_path)
		quit(2)
		return
	var battery: Dictionary = parsed
	var context: Dictionary = battery.get("context", {})

	var results: Array = []
	for case: Dictionary in battery.get("cases", []):
		var condition: String = case.get("condition", "")
		var outcome: Dictionary = TriggerDSLScript._evaluate_strict(condition, context)
		if outcome.has("error"):
			results.append({"condition": condition, "outcome": "error", "detail": outcome.error})
		else:
			results.append({"condition": condition, "outcome": outcome.value})

	var out := FileAccess.open(out_path, FileAccess.WRITE)
	if out == null:
		push_error("dsl_bridge: cannot write %s" % out_path)
		quit(2)
		return
	out.store_string(JSON.stringify({
		"semantics_version": battery.get("semantics_version", -1),
		"results": results,
	}, "  "))
	out.close()
	quit(0)
