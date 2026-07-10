extends Node2D
## Ring 0 — RemotePawns: the other players, drawn (SHIP-SHARE.md). Any
## walkable scene adds one of these next to its own walker; it listens to
## ShipShare's pawn state and renders each remote player as the crew
## member they claimed — name tag on, same sheet the NPC wore before a
## person stepped into them. Positions ease toward the latest state so
## 10 Hz intents read as walking, not teleporting.
##
## Solo: no signals ever fire, no children ever exist, zero cost.

class_name RemotePawns

const EASE_RATE := 12.0
const FACING_VECTORS := {
	"down": Vector2.DOWN, "up": Vector2.UP,
	"left": Vector2.LEFT, "right": Vector2.RIGHT,
}

var _pawns: Dictionary = {}    # peer -> CharacterSprite
var _targets: Dictionary = {}  # peer -> Vector2


func _ready() -> void:
	ShipShare.pawn_updated.connect(_on_pawn)
	ShipShare.roster_changed.connect(_sync_roster)
	ShipShare.seats_changed.connect(_sync_roster)


func _on_pawn(peer: int, position_2d: Vector2, facing: String, anim: String) -> void:
	var sprite: CharacterSprite = _pawns.get(peer)
	if sprite == null:
		sprite = _spawn(peer)
	_targets[peer] = position_2d
	sprite.set_motion(FACING_VECTORS.get(facing, Vector2.DOWN), anim == "walk")


func _process(delta: float) -> void:
	for peer: int in _targets:
		var sprite: CharacterSprite = _pawns.get(peer)
		if sprite != null:
			sprite.position = sprite.position.lerp(_targets[peer],
				1.0 - exp(-EASE_RATE * delta))


func _spawn(peer: int) -> CharacterSprite:
	var sprite := CharacterSprite.new()
	_configure(sprite, peer)
	add_child(sprite)
	_pawns[peer] = sprite
	return sprite


func _configure(sprite: CharacterSprite, peer: int) -> void:
	var entry: Dictionary = ShipShare.players.get(peer, {})
	var npc_id := str(entry.get("npc_id", ""))
	var display := str(entry.get("name", "crew"))
	if npc_id != "":
		var npc := DataRegistry.get_entity("npcs", npc_id)
		sprite.setup("npcs", npc_id, StandIn.character_color(npc, npc_id), display)
	else:
		sprite.setup("player", "character", Color(0.85, 0.86, 0.9), display)


## Roster/seat changes re-dress existing pawns (a friend claimed a seat:
## their pawn becomes that crew member) and bury departed ones.
func _sync_roster() -> void:
	for peer: int in _pawns.keys():
		if not ShipShare.players.has(peer):
			(_pawns[peer] as CharacterSprite).queue_free()
			_pawns.erase(peer)
			_targets.erase(peer)
		else:
			_configure(_pawns[peer], peer)
