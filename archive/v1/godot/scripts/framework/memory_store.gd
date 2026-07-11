extends Node
## Ring 0 — the host side of the memory interface (docs/MEMORY-INTERFACE.md).
##
## Talks REST to a Ragamuffin deployment: one vault per soul (`soul_<id>`),
## memories written as markdown documents, recall via hybrid search,
## conversations distilled to facts by ingest. Fully async (HTTPRequest);
## fully optional — offline, souls live on authored seeds and memories
## accumulate in the save's `pending_memories`, drained on reconnect.

signal store_online
signal recalled(soul_id: String, query: String, fragments: Array)

const DEFAULT_BASE := "http://127.0.0.1:8000"
const RECALL_LIMIT := 4

var online := false

var _base := DEFAULT_BASE
var _auth_key := ""
var _vault_prefix := ""
var _seeded: Dictionary = {}  # soul_id -> true, this session


func _ready() -> void:
	var env_base := OS.get_environment("REACHLOCK_MEMORY_URL")
	if env_base != "":
		_base = env_base.trim_suffix("/")
	_auth_key = _resolve_auth_key()
	_vault_prefix = OS.get_environment("REACHLOCK_VAULT_PREFIX")
	if not _is_valid_vault_prefix(_vault_prefix):
		push_warning("memory: REACHLOCK_VAULT_PREFIX %s is not [a-z0-9-] — ignored" % _vault_prefix)
		_vault_prefix = ""
	GameState.soul_memory_pending.connect(_on_memory_pending)
	_probe()


## The dev stack runs Ragamuffin with api_key auth on by default (M5).
## Resolution: $REACHLOCK_MEMORY_KEY, else the dev stack's generated key
## file. No key is still valid — an unauthenticated store, or offline.
func _resolve_auth_key() -> String:
	var env_key := OS.get_environment("REACHLOCK_MEMORY_KEY")
	if env_key != "":
		return env_key.strip_edges()
	var key_file := OS.get_environment("HOME") + "/.local/share/reachlock/ragamuffin.key"
	if FileAccess.file_exists(key_file):
		return FileAccess.get_file_as_string(key_file).strip_edges()
	return ""


## Hyphen form per docs/REACHLOCK-VAULT-CONVENTIONS.md (ragamuffin):
## ValidVaultName accepts [a-z0-9-:] only, so snake_case npc ids map to
## hyphens — tib -> soul-tib, doc_keene -> soul-doc-keene.
## $REACHLOCK_VAULT_PREFIX namespaces every vault (vault hygiene, M5):
## automated runs set e.g. "test-" so they can NEVER touch a play vault.
func vault_name(soul_id: String) -> String:
	return _vault_prefix + "soul-" + soul_id.replace("_", "-")


## Vault names are [a-z0-9-:]; the prefix must stay inside [a-z0-9-].
func _is_valid_vault_prefix(prefix: String) -> bool:
	for i in prefix.length():
		var c := prefix.unicode_at(i)
		var is_lower := c >= 0x61 and c <= 0x7A  # a-z
		var is_digit := c >= 0x30 and c <= 0x39  # 0-9
		if not (is_lower or is_digit or c == 0x2D):  # -
			return false
	return true


## --- public API ---------------------------------------------------------------


## Fire a hybrid recall; `callback` receives Array[String] fragments (possibly
## empty). Never blocks: offline or on error the callback gets [].
func recall(soul_id: String, query: String, callback: Callable) -> void:
	_recall_from(vault_name(soul_id), soul_id, query, callback)


## Recall from a NAMED vault (weave grounding: shared lore/compendium
## vaults, read-only). The prefix still applies — automated runs must
## never touch a play vault, lore included.
func recall_vault(vault: String, query: String, callback: Callable) -> void:
	_recall_from(_vault_prefix + vault, vault, query, callback)


func _recall_from(full_vault: String, label: String, query: String, callback: Callable) -> void:
	if not online or query.strip_edges() == "" or full_vault.strip_edges() == "":
		callback.call([])
		return
	var url := "%s/vault/%s/v1/hybrid?query=%s&limit=%d" % [
		_base, full_vault, query.uri_encode(), RECALL_LIMIT]
	_request(url, HTTPClient.METHOD_GET, "", func(code: int, body: Variant) -> void:
		var fragments: Array = []
		if code == 200 and body is Dictionary:
			for result: Dictionary in body.get("results", []):
				match result.get("kind", ""):
					"chunk":
						var content := str(result.get("content", ""))
						# /v1/documents chunks front-matter as text (known
						# gap, see MEMORY-INTERFACE.md); keep prompts clean.
						if not content.begins_with("---"):
							fragments.append(content)
					"fact":
						fragments.append("%s: %s" % [result.get("key", ""), result.get("value", "")])
		recalled.emit(label, query, fragments)
		callback.call(fragments))


## Write one memory into the soul's vault. Memories are first-person markdown;
## the tick rides in front-matter so recall and pruning can reason in game time.
func ship_memory(soul_id: String, memory: Dictionary, source := "") -> void:
	if not online:
		return
	if source == "":
		source = "memories/tick_%d_%d.md" % [memory.get("tick", GameState.universe.tick), randi() % 100000]
	var content := "---\nimportance: %s\ntick: %d\ntags: %s\n---\n\n%s\n" % [
		str(memory.get("importance", 0.5)),
		int(memory.get("tick", GameState.universe.tick)),
		JSON.stringify(memory.get("tags", [])),
		memory.get("text", ""),
	]
	var payload := {
		"vault": vault_name(soul_id),
		"content": content,
		"source": source,
		"tags": memory.get("tags", []),
	}
	_request(_base + "/v1/documents", HTTPClient.METHOD_POST, JSON.stringify(payload),
		func(code: int, _body: Variant) -> void:
			if code != 200 and code != 0:
				push_warning("memory: ship_memory %s -> HTTP %d" % [soul_id, code]))


## Idempotently write a soul's authored seeds (soul schema v1) into its vault.
## Deterministic sources mean re-instantiation overwrites, never duplicates.
func ensure_seeds(soul_id: String, soul: Dictionary) -> void:
	if not online or _seeded.get(soul_id, false):
		return
	_seeded[soul_id] = true
	var seeds: Array = soul.get("memory_seeds", [])
	for i in seeds.size():
		var seed_memory: Dictionary = seeds[i]
		ship_memory(soul_id, {
			"text": seed_memory.get("text", ""),
			"importance": seed_memory.get("importance", 0.5),
			"tags": seed_memory.get("tags", []),
			"tick": 0,
		}, "seeds/seed_%02d.md" % i)


## Distill a finished conversation into facts (Ragamuffin ingest). The soul
## remembers what was said, not the transcript verbatim.
func ingest_conversation(soul_id: String, messages: Array, context: Dictionary) -> void:
	if not online or messages.is_empty():
		return
	var payload := {
		"vault": vault_name(soul_id),
		"messages": messages,
		"context": context,
	}
	_request(_base + "/v1/ingest/conversation", HTTPClient.METHOD_POST, JSON.stringify(payload),
		func(code: int, body: Variant) -> void:
			if code == 200 and body is Dictionary:
				print("memory: %s distilled %d fact(s) from conversation" % [
					soul_id, int(body.get("fact_count", 0))])
			else:
				if code != 0:
					push_warning("memory: conversation ingest for %s -> HTTP %d" % [soul_id, code]))


## --- plumbing -------------------------------------------------------------------


func _probe() -> void:
	_request(_base + "/v1/briefing", HTTPClient.METHOD_GET, "",
		func(code: int, _body: Variant) -> void:
			online = code == 200
			if online:
				print("memory: store online at %s" % _base)
				_drain_pending()
				store_online.emit()
			else:
				print("memory: no store at %s — memories persist in the save" % _base))


func _on_memory_pending(soul_id: String, memory: Dictionary) -> void:
	if online:
		ship_memory(soul_id, memory)
		_forget_pending(soul_id, memory)


## Ship every memory that accumulated while offline (or in a pre-M3 save).
func _drain_pending() -> void:
	for soul_id: String in GameState.souls:
		var pending: Array = GameState.souls[soul_id].get("pending_memories", [])
		for memory: Dictionary in pending.duplicate():
			ship_memory(soul_id, memory)
			_forget_pending(soul_id, memory)


func _forget_pending(soul_id: String, memory: Dictionary) -> void:
	var state: Dictionary = GameState.souls.get(soul_id, {})
	(state.get("pending_memories", []) as Array).erase(memory)


## One async HTTP exchange. A fresh HTTPRequest per call keeps this trivially
## concurrent; the node frees itself after the callback.
func _request(url: String, method: int, body: String, callback: Callable) -> void:
	var request := HTTPRequest.new()
	request.timeout = 20.0
	add_child(request)
	request.request_completed.connect(
		func(result: int, code: int, _headers: PackedStringArray, raw: PackedByteArray) -> void:
			var parsed: Variant = null
			if result == HTTPRequest.RESULT_SUCCESS and raw.size() > 0:
				parsed = JSON.parse_string(raw.get_string_from_utf8())
			callback.call(code if result == HTTPRequest.RESULT_SUCCESS else 0, parsed)
			request.queue_free())
	var headers := PackedStringArray(["Content-Type: application/json"])
	if _auth_key != "":
		headers.append("Authorization: Bearer " + _auth_key)
	var err := request.request(url, headers, method, body)
	if err != OK:
		callback.call(0, null)
		request.queue_free()
