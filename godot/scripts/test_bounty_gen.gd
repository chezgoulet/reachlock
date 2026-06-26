extends Node

# Mock-скрипт для верификации логики bounty
func _ready():
    var crime_record = 100
    var faction_power = 50
    var bounty = crime_record * faction_power
    if bounty > 0:
        print("Bounty generated: ", bounty)
    else:
        print("Failed to generate bounty")
