extends PanelContainer
## Ring 0 — UpgradeShop: the outfitting counter (upgrade contract). Mounted
## by StationDock wherever a location's services include `shipyard`. Lists
## every loaded upgrade grouped by category — ship systems, personal
## equipment, gadgets — with the merchant's pitch and a buy button.
##
## Buying deducts credits, records ownership (GameState.add_upgrade → the
## save's player.upgrades + the flag upgrade_<id> for trigger-DSL), and
## applies any instant effects (hull_bonus patches the hull on the spot).
## Everything else — stealth multipliers, damage bonuses, timer grace —
## is read later by the systems that care. Some gadgets do nothing at all;
## the shop makes no promises, exactly like the merchant.

class_name UpgradeShop

signal purchased(upgrade_id: String)

const CATEGORY_LABELS := {
	"ship": "Ship Systems",
	"equipment": "Equipment",
	"gadget": "Widgets & Gadgets",
}

var _rows: Dictionary = {}  # upgrade_id -> {button, price_label}
var _credits_label: Label


func configure() -> void:
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0.10, 0.11, 0.14)
	style.border_color = Color(0.45, 0.40, 0.25)
	style.set_border_width_all(2)
	style.set_content_margin_all(10)
	add_theme_stylebox_override("panel", style)

	var root := VBoxContainer.new()
	root.add_theme_constant_override("separation", 6)
	add_child(root)

	var header := HBoxContainer.new()
	root.add_child(header)
	var title := Label.new()
	title.text = "Outfitting"
	title.add_theme_font_size_override("font_size", 20)
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(title)
	_credits_label = Label.new()
	_credits_label.add_theme_font_size_override("font_size", 16)
	header.add_child(_credits_label)

	var by_category := {}
	for upgrade_id in DataRegistry.ids("upgrades"):
		var upgrade := DataRegistry.get_entity("upgrades", upgrade_id)
		var category: String = upgrade.get("category", "gadget")
		if not by_category.has(category):
			by_category[category] = []
		by_category[category].append(upgrade)

	for category: String in ["ship", "equipment", "gadget"]:
		if not by_category.has(category):
			continue
		var shelf_label := Label.new()
		shelf_label.text = CATEGORY_LABELS.get(category, category.capitalize())
		shelf_label.add_theme_font_size_override("font_size", 15)
		shelf_label.add_theme_color_override("font_color", Color(0.7, 0.65, 0.45))
		root.add_child(shelf_label)
		for upgrade: Dictionary in by_category[category]:
			root.add_child(_build_row(upgrade))

	GameState.state_changed.connect(_refresh)
	_refresh()


func _build_row(upgrade: Dictionary) -> Control:
	var upgrade_id: String = upgrade.get("id", "")
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 10)

	var info := VBoxContainer.new()
	info.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(info)
	var name_label := Label.new()
	name_label.text = upgrade.get("name", upgrade_id)
	name_label.add_theme_font_size_override("font_size", 15)
	info.add_child(name_label)
	var pitch := Label.new()
	pitch.text = upgrade.get("flavor", upgrade.get("description", ""))
	pitch.add_theme_font_size_override("font_size", 12)
	pitch.add_theme_color_override("font_color", Color(0.62, 0.64, 0.70))
	pitch.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	pitch.custom_minimum_size = Vector2(420, 0)
	info.add_child(pitch)

	var button := Button.new()
	button.custom_minimum_size = Vector2(130, 0)
	button.pressed.connect(_buy.bind(upgrade_id))
	row.add_child(button)

	_rows[upgrade_id] = {"button": button}
	return row


func _buy(upgrade_id: String) -> void:
	var upgrade := DataRegistry.get_entity("upgrades", upgrade_id)
	var cost := int(upgrade.get("cost", 0))
	if GameState.has_upgrade(upgrade_id) or GameState.player.credits < cost:
		return
	GameState.adjust_credits(-cost)
	GameState.add_upgrade(upgrade_id)
	# Instant effects apply at the counter; the rest ride the save.
	var hull_bonus := float(upgrade.get("effects", {}).get("hull_bonus", 0.0))
	if hull_bonus > 0.0:
		GameState.player.ship.hull_integrity = minf(1.0,
			float(GameState.player.ship.hull_integrity) + hull_bonus)
	AudioManager.play("ui_click")
	purchased.emit(upgrade_id)
	_refresh()


func _refresh() -> void:
	_credits_label.text = "%d cr" % int(GameState.player.credits)
	for upgrade_id: String in _rows:
		var upgrade := DataRegistry.get_entity("upgrades", upgrade_id)
		var cost := int(upgrade.get("cost", 0))
		var button: Button = _rows[upgrade_id].button
		if GameState.has_upgrade(upgrade_id):
			button.text = "Owned"
			button.disabled = true
		else:
			button.text = "%d cr" % cost
			button.disabled = GameState.player.credits < cost
