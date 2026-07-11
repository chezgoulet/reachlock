extends RefCounted
## Ring 0 — the deterministic choice matcher for voice input
## (godot/framework/protocol/EAR-PROTOCOL.md).
##
## THE SEMANTICS ARE DEFINED BY scripts/check_ear_protocol.py (the
## reference implementation over ear/match_cases.json); this matcher must
## produce the same verdict for every case — same input, same verdict,
## everywhere: host, replay, CI, no mic, no network. The constants and the
## stopword list are part of the contract; tuning them is a match_cases
## change reviewed like a schema change.

class_name EarMatch

const MATCH_THRESHOLD := 0.5
const MATCH_MARGIN := 0.1
const STOPWORD_WEIGHT := 0.25

## Closed list, verbatim from the reference. Growing it is a contract change.
const STOPWORDS := ["a", "an", "the", "and", "or", "but", "so", "if", "then",
	"than", "that", "this", "these", "those", "it", "its", "im", "id", "ill",
	"ive", "is", "are", "was", "were", "be", "been", "being", "do", "does",
	"did", "doing", "dont", "didnt", "not", "no", "yes", "i", "you", "he",
	"she", "we", "they", "me", "him", "her", "us", "them", "my", "your",
	"his", "hers", "our", "their", "to", "of", "in", "on", "at", "for",
	"with", "from", "by", "as", "about", "into", "over", "under", "out",
	"up", "down", "off", "again", "just", "very", "really", "too", "also",
	"there", "here", "what", "when", "where", "who", "whom", "why", "how",
	"all", "any", "both", "each", "few", "more", "most", "other", "some",
	"such", "only", "own", "same", "can", "cant", "will", "wont", "would",
	"could", "should", "might", "must", "let", "lets", "got", "get", "have",
	"has", "had", "having", "going", "gonna"]


## The normative verdict: index of the matched choice, or -1 for no match
## (including ambiguity — a transcript that lands between two choices is
## the player's to resolve, never the matcher's).
static func match_choice(transcript: String, choices: Array) -> int:
	var transcript_tokens := _token_set(transcript)
	if transcript_tokens.is_empty() or _all_stopwords(transcript_tokens):
		return -1
	if choices.is_empty():
		return -1
	var scores: Array[float] = []
	for choice: String in choices:
		scores.append(_score(transcript_tokens, _token_set(choice)))
	var best := 0
	for i in scores.size():
		if scores[i] > scores[best]:
			best = i
	var runner_up := 0.0
	for i in scores.size():
		if i != best and scores[i] > runner_up:
			runner_up = scores[i]
	if scores[best] >= MATCH_THRESHOLD and scores[best] - runner_up >= MATCH_MARGIN:
		return best
	return -1


## Lowercase, strip apostrophes, every other non-alphanumeric rune becomes
## a space; unique tokens.
static func _token_set(text: String) -> Dictionary:
	var lowered := text.to_lower().replace("'", "").replace("’", "")
	var regex := RegEx.create_from_string("[^\\p{L}\\p{N}]")
	var cleaned := regex.sub(lowered, " ", true)
	var tokens := {}
	for token: String in cleaned.split(" ", false):
		tokens[token] = true
	return tokens


static func _all_stopwords(tokens: Dictionary) -> bool:
	for token: String in tokens:
		if token not in STOPWORDS:
			return false
	return true


static func _weight(tokens: Dictionary) -> float:
	var total := 0.0
	for token: String in tokens:
		total += STOPWORD_WEIGHT if token in STOPWORDS else 1.0
	return total


## Weighted Dice overlap over token sets.
static func _score(transcript_tokens: Dictionary, choice_tokens: Dictionary) -> float:
	var overlap := {}
	for token: String in transcript_tokens:
		if choice_tokens.has(token):
			overlap[token] = true
	var denominator := _weight(transcript_tokens) + _weight(choice_tokens)
	if denominator == 0.0:
		return 0.0
	return 2.0 * _weight(overlap) / denominator
