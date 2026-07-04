package universe

import (
	"testing"
)

// A little universe with two locations pulling the same good in opposite
// directions, plus a related faction pair — enough for every M6 system to
// have something to do.
func m6State(seed uint64) *State {
	s := NewState(seed)
	s.AddFaction("faction_a", "Faction A", []string{"expand"}, map[string]string{"faction_b": "tense"})
	s.AddFaction("faction_b", "Faction B", []string{"survive"}, map[string]string{"faction_a": "tense"})
	s.AddGood("good_ore", 12)
	s.AddGood("good_food", 8)
	s.AddLocation("station_a", []string{"good_food"}, []string{"good_ore"})
	s.AddLocation("outpost_b", []string{"good_ore"}, []string{"good_food"})
	// A third consumer keeps the universe ASYMMETRIC (2 consumers vs 1
	// producer of ore): a perfectly balanced universe nets to ~zero and
	// prices sit still, which is not what any authored content looks like.
	s.AddLocation("depot_c", nil, []string{"good_ore"})
	return s
}

// The economy must MOVE on its own: after enough ticks for a few reprice
// intervals, prices differ from base and the journal is not empty.
func TestEconomyMovesWithoutInputs(t *testing.T) {
	s := m6State(42)
	Advance(s, 600, nil) // 10 reprice intervals
	moved := false
	for _, g := range s.Market {
		if g.Price != g.BasePrice {
			moved = true
		}
	}
	if !moved {
		t.Error("after 600 ticks no price moved off its baseline — the universe is dead")
	}
	if len(s.Journal) == 0 {
		t.Error("after 600 ticks the journal is empty — nothing to report as news")
	}
}

// Per-location prices must differ when local supply/demand differ: verne
// produces raw_ore (surplus -> cheap), sorrow_station consumes it
// (backlog -> dear).
func TestPriceAtDivergesAcrossLocations(t *testing.T) {
	s := m6State(42)
	Advance(s, 600, nil)
	atVerne, _, _, ok1 := PriceAt(s, "outpost_b", "good_ore")
	atSorrow, _, _, ok2 := PriceAt(s, "station_a", "good_ore")
	if !ok1 || !ok2 {
		t.Fatalf("PriceAt reported unknown location/good")
	}
	if atVerne >= atSorrow {
		t.Errorf("ore should be cheaper where it is produced: producer=%d consumer=%d",
			atVerne, atSorrow)
	}
	if _, _, _, ok := PriceAt(s, "nowhere", "good_ore"); ok {
		t.Error("PriceAt(nowhere) should report !ok")
	}
}

// A trade input shifts the local price against the trader and lands in
// the journal.
func TestTradeInputMovesLocalPriceAndJournals(t *testing.T) {
	s := m6State(42)
	Advance(s, 120, nil) // settle in
	before, _, _, _ := PriceAt(s, "station_a", "good_ore")
	// Sell a big load of ore TO the station: supply spikes.
	ApplyInput(s, &Input{Kind: "trade", AtLocation: "station_a", GoodID: "good_ore", Amount: 200})
	after, _, _, _ := PriceAt(s, "station_a", "good_ore")
	if after >= before {
		t.Errorf("selling 200 ore should depress the local price: before=%d after=%d", before, after)
	}
	last := s.Journal[len(s.Journal)-1]
	if last.Kind != "trade" || last.GoodID != "good_ore" || last.Amount != 200 {
		t.Errorf("trade journal entry wrong: %+v", last)
	}
}

// Determinism holds for every new system: same seed + same inputs =>
// identical state INCLUDING the journal; different seed diverges.
func TestJournalAndDynamicsAreDeterministic(t *testing.T) {
	a, b := m6State(7), m6State(7)
	inputs := []Input{{Kind: "trade", AtLocation: "outpost_b", GoodID: "good_food", Amount: -5}}
	Advance(a, 3000, inputs)
	Advance(b, 3000, inputs)
	if !a.Equal(b) {
		t.Fatal("same seed + same inputs produced different states (journal included)")
	}
	c := m6State(8)
	Advance(c, 3000, inputs)
	if a.Equal(c) {
		t.Error("different seeds produced identical states — RNG is not wired in")
	}
}

// Batch == sequential must survive the new per-tick systems.
func TestBatchEqualsSequentialWithM6Systems(t *testing.T) {
	batch := m6State(99)
	seq := m6State(99)
	Advance(batch, 2880, nil) // two faction-goal evaluations
	for i := 0; i < 2880; i++ {
		Advance(seq, 1, nil)
	}
	if !batch.Equal(seq) {
		t.Fatal("Advance(2880) != 2880 x Advance(1) — batch time-skips would cheat")
	}
}

// Snapshot round-trip carries the journal.
func TestSnapshotRoundTripCarriesJournal(t *testing.T) {
	s := m6State(11)
	Advance(s, 1500, nil)
	if len(s.Journal) == 0 {
		t.Fatal("test needs a non-empty journal")
	}
	raw, err := MarshalJSON(s)
	if err != nil {
		t.Fatal(err)
	}
	loaded, err := UnmarshalJSON(raw)
	if err != nil {
		t.Fatal(err)
	}
	if !s.Equal(loaded) {
		t.Error("snapshot round-trip lost state (journal?)")
	}
	if len(loaded.JournalSince(0)) != len(s.Journal) {
		t.Error("JournalSince(0) should return the whole journal")
	}
}

// Strained faction pairs eventually make news. 20 in-game days of a tense
// pair with negative trust should produce at least one stance/patrol/
// skirmish entry (1-in-8 + 1-in-3 chances per day; P(nothing in 20 days)
// is negligible, and the run is deterministic anyway — this asserts the
// dynamics are alive at THIS seed, which is the property the news needs).
func TestStrainedFactionsMakeNews(t *testing.T) {
	s := m6State(42)
	s.Factions["faction_a"].Trust = -80
	s.Factions["faction_b"].Trust = -80
	Advance(s, 1440*20, nil)
	kinds := map[string]int{}
	for _, e := range s.Journal {
		kinds[e.Kind]++
	}
	if kinds["stance_change"]+kinds["patrol"]+kinds["skirmish"] == 0 {
		t.Errorf("20 days of a strained pair produced no faction news; journal kinds: %v", kinds)
	}
}

// A snapshot that round-tripped through a float-only JSON implementation
// (Godot writes every number with a fraction: -43.0) must still load —
// this is the SP save/resume path.
func TestUnmarshalToleratesFloatStyleNumbers(t *testing.T) {
	raw := []byte(`{"tick": 3611.0, "seed": 1.0,
		"factions": {"faction_a": {"id": "faction_a", "name": "Faction A",
			"goals": [], "trust": -43.0, "relationships": {"faction_b": "tense"}}},
		"market": {"good_ore": {"good_id": "good_ore", "base_price": 12.0,
			"price": 24.0, "net_delta": -48.0, "last_reprice_tick": 3599.0}},
		"locations": {}, "events": [],
		"journal": [{"tick": 3599.0, "kind": "reprice", "good_id": "good_ore",
			"amount": 24.0, "delta": 1.0}]}`)
	s, err := UnmarshalJSON(raw)
	if err != nil {
		t.Fatalf("float-style snapshot rejected: %v", err)
	}
	if s.Tick != 3611 || s.Factions["faction_a"].Trust != -43 {
		t.Errorf("float-style snapshot decoded wrong: tick=%d trust=%d",
			s.Tick, s.Factions["faction_a"].Trust)
	}
}
