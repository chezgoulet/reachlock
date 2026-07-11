extends PanelContainer
## Ring 0 — the MarketBoard framework scene (P2, Sprint 02).
##
## Renders {good, price, supply/demand, legality} for one location from ANY
## price source: live from SimGateway when the sim daemon is up, the static
## authored fallback when not — callers never know which. Buys and sells
## move credits and cargo through GameState and flow back into the sim as
## trade inputs, so the market remembers you.
##
## Data-driven: a mod gets a market by listing "market" in a location's
## services and giving goods files — no engine code. Instance the scene
## (scenes/framework/market_board.tscn) and call configure(location).

class_name MarketBoard

signal traded(good_id: String, amount: int, price: int)

var _location: Dictionary = {}
var _grid: GridContainer
var _title: Label
var _tick_of_prices := -1


func _ready() -> void:
	var root := VBoxContainer.new()
	root.add_theme_constant_override("separation", 6)
	add_child(root)
	_title = Label.new()
	_title.add_theme_font_size_override("font_size", 20)
	root.add_child(_title)
	_grid = GridContainer.new()
	_grid.columns = 6
	_grid.add_theme_constant_override("h_separation", 16)
	root.add_child(_grid)
	SimGateway.prices_received.connect(_on_prices)
	SimGateway.advanced.connect(_on_tick)


## Point the board at a location (the location's own dictionary). Prices
## arrive asynchronously via SimGateway.
func configure(location: Dictionary) -> void:
	_location = location
	_title.text = "Market — %s" % location.get("name", location.get("id", "?"))
	SimGateway.request_prices(str(location.get("id", "")))


func _on_tick(tick: int) -> void:
	# Refresh on the sim's reprice cadence (every 60 ticks = 1 in-game
	# hour); trades refresh immediately via apply_trade's follow-up query.
	if not _location.is_empty() and tick % 60 == 0:
		SimGateway.request_prices(str(_location.get("id", "")))


func _on_prices(location_id: String, tick: int, prices: Array) -> void:
	if location_id != str(_location.get("id", "")):
		return
	_tick_of_prices = tick
	_title.text = "Market — %s   %s" % [
		_location.get("name", location_id),
		"(live, tick %d)" % tick if SimGateway.is_ready() else "(static — no sim)",
	]
	for child in _grid.get_children():
		child.queue_free()
	_header_row()
	for entry: Dictionary in prices:
		_price_row(entry)


func _header_row() -> void:
	for text in ["Good", "Price", "vs avg", "Local S/D", "Legality", "Trade"]:
		var label := Label.new()
		label.text = text
		label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		_grid.add_child(label)


func _price_row(entry: Dictionary) -> void:
	var good_id := str(entry.get("good_id", ""))
	var good := DataRegistry.get_entity("goods", good_id)
	var price := int(entry.get("price", 0))
	var base := int(entry.get("base_price", price))
	var legality := _legality(good)

	var name_label := Label.new()
	name_label.text = good.get("name", good_id)
	_grid.add_child(name_label)

	var price_label := Label.new()
	price_label.text = "%d cr" % price
	_grid.add_child(price_label)

	var delta_label := Label.new()
	var delta := price - base
	delta_label.text = "%+d" % delta if delta != 0 else "—"
	if delta > 0:
		delta_label.add_theme_color_override("font_color", Color(0.9, 0.55, 0.3))
	elif delta < 0:
		delta_label.add_theme_color_override("font_color", Color(0.4, 0.85, 0.5))
	_grid.add_child(delta_label)

	var sd_label := Label.new()
	sd_label.text = "%d / %d" % [int(entry.get("supply", 0)), int(entry.get("demand", 0))]
	_grid.add_child(sd_label)

	var legality_label := Label.new()
	legality_label.text = legality
	if legality != "legal":
		legality_label.add_theme_color_override("font_color", Color(0.9, 0.35, 0.35))
	_grid.add_child(legality_label)

	var buttons := HBoxContainer.new()
	if legality == "illegal":
		var banned := Label.new()
		banned.text = "banned here"
		buttons.add_child(banned)
	else:
		var held := GameState.cargo_count(good_id)
		var buy := Button.new()
		buy.text = "Buy"
		buy.disabled = GameState.player.credits < price
		buy.pressed.connect(_trade.bind(good_id, -1, price))
		buttons.add_child(buy)
		var sell := Button.new()
		sell.text = "Sell"
		sell.disabled = held <= 0
		sell.pressed.connect(_trade.bind(good_id, 1, price))
		buttons.add_child(sell)
		if held > 1:
			var sell_all := Button.new()
			sell_all.text = "Sell %d" % held
			sell_all.pressed.connect(_trade.bind(good_id, held, price))
			buttons.add_child(sell_all)
	_grid.add_child(buttons)


## The good's authored legality under this location's controlling faction.
func _legality(good: Dictionary) -> String:
	var faction := str(_location.get("faction_control", ""))
	if faction == "":
		return "legal"
	return str((good.get("legality", {}) as Dictionary).get(faction, "legal"))


## amount follows the sim convention: > 0 sells TO the station, < 0 buys.
func _trade(good_id: String, amount: int, price: int) -> void:
	if amount < 0 and GameState.player.credits < price * -amount:
		return
	if amount > 0 and GameState.cargo_count(good_id) < amount:
		return
	GameState.adjust_credits(price * amount)
	GameState.add_cargo(good_id, -amount)
	SimGateway.apply_trade(str(_location.get("id", "")), good_id, amount)
	Reputation.trigger("on_trade_completed", {
		"good_id": good_id, "amount": amount, "price": price,
		"faction_control": _location.get("faction_control", ""),
	})
	traded.emit(good_id, amount, price)
	if not SimGateway.is_ready():
		# Offline: no prices reply will arrive; re-render so cargo-dependent
		# buttons stay honest.
		SimGateway.request_prices(str(_location.get("id", "")))
