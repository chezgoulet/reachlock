extends PanelContainer
## Ring 0 — the ReputationPanel framework UI (P8, Sprint 02).
##
## Displays the player's standing with each known faction across multiple axes
## (trust, contribution, notoriety). Integrated with MarketBoard — prices are
## modified by standing (visible as a modifier column), and goods restricted
## by rep level are flagged.
##
## The panel is data-driven: it reads all factions from DataRegistry and their
## runtime standings from GameState. A modder adds a new faction by writing a
## faction.json — the panel renders it automatically.
##
## Instance: scenes/framework/reputation_panel.tscn and call configure().

class_name ReputationPanel

var _grid: GridContainer
var _title: Label


func _ready() -> void:
	var root := VBoxContainer.new()
	root.add_theme_constant_override("separation", 8)
	add_child(root)

	_title = Label.new()
	_title.text = "Faction Standing"
	_title.add_theme_font_size_override("font_size", 22)
	root.add_child(_title)

	_grid = GridContainer.new()
	_grid.columns = 6
	_grid.add_theme_constant_override("h_separation", 16)
	_grid.add_theme_constant_override("v_separation", 4)
	root.add_child(_grid)

	GameState.state_changed.connect(_refresh)
	_refresh()


func configure() -> void:
	_refresh()


func _refresh() -> void:
	for child in _grid.get_children():
		child.queue_free()

	_header_row()

	var faction_ids: Array[String] = DataRegistry.ids("factions")
	faction_ids.sort()
	for fid: String in faction_ids:
		var faction := DataRegistry.get_entity("factions", fid)
		var standing := GameState.faction_standing(fid)
		_faction_row(faction, fid, standing)

	var hint := Label.new()
	hint.text = "Standing affects prices and unlocks. Trade, dock, and dialogue shift these values."
	hint.add_theme_font_size_override("font_size", 12)
	hint.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	add_child(hint)


func _header_row() -> void:
	for text in ["Faction", "Trust", "Contribution", "Notoriety", "Price Mod", "Status"]:
		var label := Label.new()
		label.text = text
		label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		_grid.add_child(label)


func _faction_row(faction: Dictionary, fid: String, standing: Dictionary) -> void:
	var name_label := Label.new()
	name_label.text = faction.get("short_name", faction.get("name", fid))
	name_label.add_theme_font_size_override("font_size", 16)
	_grid.add_child(name_label)

	for axis in ["trust", "contribution", "notoriety"]:
		var val := int(standing.get(axis, 0))
		var label := Label.new()
		label.text = str(val)
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		var c := Color.GREEN_YELLOW if val >= 10 else (Color.RED if val <= -10 else Color.WHITE)
		label.add_theme_color_override("font_color", c)
		_grid.add_child(label)

	# Price modifier from standing
	var mod := GameState.price_modifier_for(fid)
	var mod_label := Label.new()
	if mod != 0.0:
		var pct := int(mod * 100)
		mod_label.text = "%+d%%" % pct
		var c := Color(0.4, 0.85, 0.5) if mod < 0 else Color(0.9, 0.55, 0.3)
		mod_label.add_theme_color_override("font_color", c)
	else:
		mod_label.text = "—"
	_grid.add_child(mod_label)

	# Status: relationship stance from authored data
	var stance: String = faction.get("relationships", {}).get("player", "neutral")
	var stance_label := Label.new()
	stance_label.text = stance.capitalize()
	match stance:
		"allied": stance_label.add_theme_color_override("font_color", Color(0.3, 0.8, 0.3))
		"friendly": stance_label.add_theme_color_override("font_color", Color(0.5, 0.9, 0.5))
		"neutral": stance_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		"tense": stance_label.add_theme_color_override("font_color", Color(0.9, 0.7, 0.3))
		"hostile": stance_label.add_theme_color_override("font_color", Color(0.9, 0.4, 0.3))
		"war": stance_label.add_theme_color_override("font_color", Color(0.9, 0.2, 0.2))
	_grid.add_child(stance_label)
