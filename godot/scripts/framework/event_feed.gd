extends PanelContainer
## Ring 0 — the EventFeed framework scene (P3, Sprint 02).
##
## Renders the simulation's journal as a chronological news feed. Every
## item is a REAL universe event (a reprice, a stance shift, a patrol, a
## skirmish, a trade) — structured data from the Sim Protocol journal,
## turned into text here with names resolved through DataRegistry. The
## engine contains zero content ids; the templates are generic.
##
## Instance scenes/framework/event_feed.tscn and call configure(). Each
## rendered entry is also re-emitted as `item_added` so a host scene can
## broadcast it to souls as a perceive event (news.<kind>) — Tib reads
## the same news you do.

class_name EventFeed

signal item_added(entry: Dictionary)

const MAX_LINES := 40

var _log: RichTextLabel
var _title: Label


func _ready() -> void:
	var root := VBoxContainer.new()
	add_child(root)
	_title = Label.new()
	_title.text = "News"
	_title.add_theme_font_size_override("font_size", 20)
	root.add_child(_title)
	_log = RichTextLabel.new()
	_log.bbcode_enabled = true
	_log.scroll_following = true
	_log.fit_content = false
	_log.custom_minimum_size = Vector2(0, 140)
	_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add_child(_log)
	SimGateway.news.connect(_on_news)


## Show the backlog seen this session, then follow live news.
func configure() -> void:
	if not SimGateway.is_ready():
		_log.append_text("[i]No signal — the news services are quiet out here.[/i]\n")
		return
	var backlog: Array = SimGateway.recent_news()
	for entry: Dictionary in backlog.slice(maxi(0, backlog.size() - MAX_LINES)):
		_log.append_text(render(entry) + "\n")


func _on_news(entries: Array) -> void:
	for entry: Dictionary in entries:
		_log.append_text(render(entry) + "\n")
		item_added.emit(entry)


## One journal entry as a feed line. Names come from loaded content;
## unknown ids render as themselves (a total-conversion mod still gets a
## readable feed).
func render(entry: Dictionary) -> String:
	var tick := int(entry.get("tick", 0))
	var stamp := "[color=#888]T%d[/color]" % tick
	match entry.get("kind", ""):
		"reprice":
			return "%s  %s now trades around [b]%d cr[/b] (%+d)." % [
				stamp, _name("goods", entry.get("good_id", "")),
				int(entry.get("amount", 0)), int(entry.get("delta", 0))]
		"stance_change":
			return "%s  Relations between %s and %s shift to [b]%s[/b]." % [
				stamp, _name("factions", entry.get("faction_id", "")),
				_name("factions", entry.get("with", "")), entry.get("stance", "?")]
		"patrol":
			return "%s  A %s patrol is pushing into %s space." % [
				stamp, _name("factions", entry.get("faction_id", "")),
				_name("factions", entry.get("with", ""))]
		"skirmish":
			return "%s  Skirmish reported between %s and %s forces." % [
				stamp, _name("factions", entry.get("faction_id", "")),
				_name("factions", entry.get("with", ""))]
		"trade":
			var amount := int(entry.get("amount", 0))
			return "%s  %d units of %s %s at %s." % [
				stamp, absi(amount), _name("goods", entry.get("good_id", "")),
				"sold" if amount > 0 else "bought",
				_name("locations", entry.get("location_id", ""))]
		"event_fired":
			return "%s  Word of a %s event%s." % [
				stamp, entry.get("event_kind", "?"),
				" near " + _name("locations", entry.get("location_id", ""))
					if entry.get("location_id", "") != "" else ""]
		_:
			return "%s  %s" % [stamp, JSON.stringify(entry)]


func _name(kind: String, id: Variant) -> String:
	var entity := DataRegistry.get_entity(kind, str(id))
	return entity.get("name", str(id))
