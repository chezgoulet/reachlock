extends RefCounted
## Ring 0 — in-engine evaluator for the trigger-condition DSL.
##
## THE SEMANTICS ARE DEFINED BY scripts/trigger_dsl.py (the reference
## implementation and its 27-case battery); this evaluator must agree with it.
## One deliberate difference per the contract: at runtime we are LENIENT —
## `evaluate()` returns false and logs a warning on any parse/eval error,
## because CI already validated every authored condition. Use `parse_check()`
## in tools if you need strictness.

class_name TriggerDSL

const _TOKEN_PATTERN := "\\s*(?:(?<num>-?\\d+(?:\\.\\d+)?)|(?<str>\"[^\"\\n]*\")|(?<ident>[a-z_][a-z0-9_]*(?:\\.[a-z_][a-z0-9_]*)*)|(?<op>==|!=|<=|>=|<|>|\\(|\\)))"


static func evaluate(condition: String, context: Dictionary) -> bool:
	var result := _evaluate_strict(condition, context)
	if result.has("error"):
		push_warning("trigger-dsl: %s in condition %s" % [result.error, condition])
		return false
	return result.value


## Strict evaluation for tests/tools: {value: bool} or {error: String}.
static func _evaluate_strict(condition: String, context: Dictionary) -> Dictionary:
	var tokens := _tokenize(condition)
	if tokens.size() > 0 and tokens[-1].get("kind") == "error":
		return {"error": tokens[-1].value}
	var parser := _Parser.new(tokens)
	var ast: Variant = parser.parse()
	if parser.error != "":
		return {"error": parser.error}
	var ev := _Eval.new(context)
	var value: Variant = ev.eval_bool(ast)
	if ev.error != "":
		return {"error": ev.error}
	return {"value": value}


static func _tokenize(text: String) -> Array:
	var regex := RegEx.new()
	regex.compile(_TOKEN_PATTERN)
	var tokens: Array = []
	var pos := 0
	while pos < text.length():
		var m := regex.search(text, pos)
		if m == null or m.get_start() != _skip_ws_start(text, pos):
			if text.substr(pos).strip_edges() == "":
				break
			tokens.append({"kind": "error", "value": "unexpected character at offset %d" % pos})
			return tokens
		pos = m.get_end()
		if m.get_string("num") != "":
			tokens.append({"kind": "num", "value": m.get_string("num").to_float()})
		elif m.get_string("str") != "":
			var s := m.get_string("str")
			tokens.append({"kind": "str", "value": s.substr(1, s.length() - 2)})
		elif m.get_string("ident") != "":
			var word := m.get_string("ident")
			if word in ["and", "or", "not", "in"]:
				tokens.append({"kind": "kw", "value": word})
			elif word == "true":
				tokens.append({"kind": "bool", "value": true})
			elif word == "false":
				tokens.append({"kind": "bool", "value": false})
			else:
				tokens.append({"kind": "path", "value": word})
		else:
			tokens.append({"kind": "op", "value": m.get_string("op")})
	return tokens


static func _skip_ws_start(text: String, pos: int) -> int:
	while pos < text.length() and (text[pos] == " " or text[pos] == "\t"):
		pos += 1
	return pos


class _Parser:
	var tokens: Array
	var i := 0
	var error := ""

	func _init(t: Array) -> void:
		tokens = t

	func _peek() -> Dictionary:
		return tokens[i] if i < tokens.size() else {}

	func _take() -> Dictionary:
		var tok := _peek()
		if tok.is_empty():
			error = "unexpected end of condition"
			return {}
		i += 1
		return tok

	func parse() -> Variant:
		if tokens.is_empty():
			error = "empty condition"
			return null
		var node: Variant = _or_expr()
		if error == "" and i < tokens.size():
			error = "trailing input"
		return node

	func _or_expr() -> Variant:
		var node: Variant = _and_expr()
		while error == "" and _peek() == {"kind": "kw", "value": "or"}:
			_take()
			node = ["or", node, _and_expr()]
		return node

	func _and_expr() -> Variant:
		var node: Variant = _unary()
		while error == "" and _peek() == {"kind": "kw", "value": "and"}:
			_take()
			node = ["and", node, _unary()]
		return node

	func _unary() -> Variant:
		if _peek() == {"kind": "kw", "value": "not"}:
			_take()
			return ["not", _unary()]
		return _comparison()

	func _comparison() -> Variant:
		var left: Variant = _operand()
		if error != "":
			return null
		var tok := _peek()
		if tok.get("kind") == "op" and tok.get("value") in ["==", "!=", "<", "<=", ">", ">="]:
			var op: String = _take().value
			return ["cmp", op, left, _operand()]
		if tok == {"kind": "kw", "value": "in"}:
			_take()
			return ["in", left, _operand()]
		return left

	func _operand() -> Variant:
		var tok := _take()
		if error != "":
			return null
		match tok.get("kind"):
			"num", "str", "bool":
				return ["lit", tok.value]
			"path":
				return ["get", tok.value]
			"op":
				if tok.value == "(":
					var node: Variant = _or_expr()
					if error == "" and _take() != {"kind": "op", "value": ")"}:
						error = "expected ')'"
					return node
		error = "unexpected token %s" % str(tok.get("value"))
		return null


class _Eval:
	var context: Dictionary
	var error := ""

	func _init(ctx: Dictionary) -> void:
		context = ctx

	func eval_bool(node: Variant) -> bool:
		if error != "" or node == null:
			return false
		var head: String = node[0]
		match head:
			"or":
				return eval_bool(node[1]) or eval_bool(node[2])
			"and":
				return eval_bool(node[1]) and eval_bool(node[2])
			"not":
				return not eval_bool(node[1])
			"cmp":
				return _compare(node[1], _value(node[2]), _value(node[3]))
			"in":
				var needle: Variant = _value(node[1])
				var haystack: Variant = _value(node[2])
				if error != "":
					return false
				if not haystack is Array:
					error = "'in' needs a list on the right"
					return false
				return needle in haystack
			"lit", "get":
				var v: Variant = _value(node)
				if error != "":
					return false
				if v is bool:
					return v
				error = "bare non-boolean operand is not a condition"
				return false
		error = "unknown node %s" % head
		return false

	func _value(node: Variant) -> Variant:
		if node == null:
			return null
		if node[0] == "lit":
			return node[1]
		if node[0] == "get":
			return _resolve(node[1])
		return eval_bool(node)

	func _resolve(path: String) -> Variant:
		var current: Variant = context
		for part in path.split("."):
			if not current is Dictionary or not (current as Dictionary).has(part):
				error = "path '%s' does not resolve" % path
				return null
			current = current[part]
		return current

	func _compare(op: String, left: Variant, right: Variant) -> bool:
		if error != "":
			return false
		var left_num := _is_number(left)
		var right_num := _is_number(right)
		if op in ["<", "<=", ">", ">="]:
			if not left_num or not right_num:
				error = "operator '%s' needs numbers" % op
				return false
			var a := float(left)
			var b := float(right)
			match op:
				"<": return a < b
				"<=": return a <= b
				">": return a > b
				">=": return a >= b
		if not (left_num and right_num) and typeof(left) != typeof(right):
			error = "'%s' compares mismatched types" % op
			return false
		return (left == right) if op == "==" else (left != right)

	func _is_number(v: Variant) -> bool:
		return (v is int or v is float) and not v is bool
