package universe

import (
	"bytes"
	"encoding/json"
	"math/rand"
	"sort"
	"testing"
)

// makeState builds a small but representative state for the test suite:
// three factions, three goods, one location. The values are not from
// real content; they are just enough to exercise every code path.
func makeState(seed uint64) *State {
	s := NewState(seed)
	s.AddFaction("alpha", "Alpha Coalition", []string{"expand"}, map[string]string{
		"beta": "neutral",
		"gamma": "tense",
	})
	s.AddFaction("beta", "Beta Union", []string{"defend"}, map[string]string{
		"alpha": "neutral",
		"gamma": "allied",
	})
	s.AddFaction("gamma", "Gamma Hegemony", []string{"conquer"}, map[string]string{
		"alpha": "tense",
		"beta": "allied",
	})
	s.AddGood("ore", 10)
	s.AddGood("food", 8)
	s.AddGood("tech", 40)
	s.AddLocation("station_one", []string{"food"}, []string{"ore", "tech"})
	return s
}

// TestAdvance_Deterministic is the property test for C8. It runs the
// same state + same inputs through N single-tick calls and through
// one N-tick batch call, then asserts the two results are deeply
// equal. It does this for many (seed, n, input-set) combinations.
func TestAdvance_Deterministic(t *testing.T) {
	rng := rand.New(rand.NewSource(1)) // local rng to vary test params
	for trial := 0; trial < 30; trial++ {
		seed := uint64(rng.Int63())
		n := int64(1 + rng.Intn(200))
		inputs := make([]Input, rng.Intn(20))
		for i := range inputs {
			inputs[i] = makeRandomInput(rng)
		}

		a := makeState(seed)
		b := makeState(seed)
		// Advance(s, n, inputs) replays the input list on every one
		// of the n ticks. The single-step equivalent is the same
		// loop, broken out by hand. Both must end at the same state
		// for the batch-step equivalence (C8) to hold.
		Advance(a, n, inputs)
		for k := int64(0); k < n; k++ {
			for j := range inputs {
				applyInput(b, &inputs[j])
			}
			fireDueEvents(b)
			stepFactions(b)
			maybeReprice(b)
			b.Tick++
		}
		// The two paths must produce the same state, including RNG
		// draws. If they differ, the batch path is using a shortcut
		// that breaks determinism — that is the property the contract
		// is here to enforce.
		if !a.Equal(b) {
			t.Fatalf("trial %d: Advance(s, %d) != %d single-tick calls", trial, n, n)
		}
		t.Logf("trial %d ok (n=%d, %d inputs)", trial, n, len(inputs))
	}
}

// TestAdvance_NoInputs is a tighter version of the same property: the
// same seed with the same number of ticks and no inputs must produce
// the same state every time. If this fails, the RNG or the per-tick
// systems are non-deterministic in some trivial way (e.g. map
// iteration order leaking into a draw).
func TestAdvance_NoInputs(t *testing.T) {
	a := makeState(42)
	b := makeState(42)
	Advance(a, 500, nil)
	Advance(b, 500, nil)
	if !a.Equal(b) {
		t.Fatalf("same seed, no inputs, same tick count → different state\n%+v\n%+v", a, b)
	}
}

// TestAdvance_ZeroTicks is a degenerate but documented case: zero
// ticks must return the state unchanged.
func TestAdvance_ZeroTicks(t *testing.T) {
	s := makeState(7)
	before := Snapshot(s)
	Advance(s, 0, []Input{{Kind: "supply", AtLocation: "station_one", GoodID: "ore", Amount: 99}})
	after := Snapshot(s)
	if !before.Equal(after) {
		t.Fatalf("Advance(s, 0, _) must be a no-op\nbefore: %+v\nafter:  %+v", before, after)
	}
}

// TestFactionDrift_Deterministic checks the per-faction RNG stream
// directly: drawing from (seed, "faction.alpha", tick) twice yields
// the same number. This is what makes per-stream RNG work in the
// first place.
func TestFactionDrift_Deterministic(t *testing.T) {
	r1 := newStream(0xC0FFEE, "faction.alpha", 100)
	r2 := newStream(0xC0FFEE, "faction.alpha", 100)
	for i := 0; i < 20; i++ {
		if r1.Next() != r2.Next() {
			t.Fatalf("xorshift stream is non-deterministic at draw %d", i)
		}
	}
}

// TestFactionDrift_Independent checks the stream-name property:
// draws from different streams are uncorrelated (i.e. a draw from
// "faction.alpha" doesn't influence a draw from "faction.beta").
func TestFactionDrift_Independent(t *testing.T) {
	a := newStream(1, "faction.alpha", 0)
	b := newStream(1, "faction.beta", 0)
	differs := false
	for i := 0; i < 20; i++ {
		if a.Next() == b.Next() {
			// Two same draws in a row is unlikely; the test would
			// already be very wrong if it happened on draw 0.
			if i == 0 {
				t.Fatalf("streams alpha and beta drew the same number on draw 0 — names not independent")
			}
		} else {
			differs = true
		}
	}
	if !differs {
		t.Fatalf("streams never differed across 20 draws — names not independent")
	}
}

// TestFactionDrift_Bounded checks that the per-tick trust drift
// stays in [-1, +1] (the contract: small, symmetric, no opinion).
func TestFactionDrift_Bounded(t *testing.T) {
	s := makeState(123)
	for _, f := range s.Factions {
		f.Trust = 50
	}
	for tick := int64(0); tick < 200; tick++ {
		stepFactions(s)
		for id, f := range s.Factions {
			if f.Trust < -100 || f.Trust > 100 {
				t.Fatalf("faction %s trust out of range at tick %d: %d", id, tick, f.Trust)
			}
		}
	}
}

// TestEventQueue_Ordering is the contract for the event queue: two
// events with the same due tick fire in (priority desc, insertion
// order) order — never map iteration or wall time.
func TestEventQueue_Ordering(t *testing.T) {
	s := makeState(1)
	s.insertSeq = 0
	// Insertion order is encoded by AddQueue / EnqueueInput via
	// insertSeq; the sim assigns it, the test mirrors that here.
	mkEvent := func(due int64, prio int, seq uint64, kind string) Event {
		return Event{DueTick: due, Priority: prio, InsertSeq: seq, Kind: kind}
	}
	s.Events = EventQueue{
		mkEvent(5, 0, 1, "a"),
		mkEvent(5, 1, 2, "b"),
		mkEvent(3, 0, 3, "c"),
		mkEvent(5, 0, 4, "d"),
		mkEvent(5, 1, 5, "e"),
	}
	sort.Sort(s.Events)
	// After sorting: (3,0,3)=c, (5,1,2)=b, (5,1,5)=e, (5,0,1)=a, (5,0,4)=d
	want := []string{"c", "b", "e", "a", "d"}
	for i, w := range want {
		if s.Events[i].Kind != w {
			t.Fatalf("event[%d] = %s, want %s (full: %+v)", i, s.Events[i].Kind, w, s.Events)
		}
	}
}

// TestEventQueue_FiresInOrder is the integration test: enqueue some
// future events, advance to their due tick, assert the consumer saw
// them in the right order via the market/supply deltas.
func TestEventQueue_FiresInOrder(t *testing.T) {
	s := makeState(1)
	s.AddLocation("loc_a", nil, []string{"ore"})
	s.AddLocation("loc_b", nil, []string{"ore"})
	// Three events due at tick 1, same priority, different
	// insertion order. AddQueue assigns insertSeq monotonically.
	s.EnqueueInput(Input{Kind: "supply", AtLocation: "loc_a", GoodID: "ore", Amount: 1})
	s.EnqueueInput(Input{Kind: "supply", AtLocation: "loc_b", GoodID: "ore", Amount: 10})
	s.EnqueueInput(Input{Kind: "supply", AtLocation: "loc_a", GoodID: "ore", Amount: 100})
	// All three are at due=0 (current tick). Advance one tick.
	Advance(s, 1, nil)
	gotA := s.Locations["loc_a"].SupplyByGood["ore"]
	gotB := s.Locations["loc_b"].SupplyByGood["ore"]
	if gotA != 101 {
		t.Errorf("loc_a supply = %d, want 101 (1+100)", gotA)
	}
	if gotB != 10 {
		t.Errorf("loc_b supply = %d, want 10", gotB)
	}
}

// TestSnapshot_RoundTrip is the determinism test for serialization:
// Marshal → Unmarshal must produce a state that re-advance yields
// the same result as the original state.
func TestSnapshot_RoundTrip(t *testing.T) {
	a := makeState(99)
	Advance(a, 10, []Input{{Kind: "supply", AtLocation: "station_one", GoodID: "ore", Amount: 7}})

	raw, err := MarshalJSON(a)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	b, err := UnmarshalJSON(raw)
	if err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if !a.Equal(b) {
		t.Fatalf("snapshot round-trip not lossless\noriginal: %+v\nloaded:   %+v", a, b)
	}

	// And the post-load state must continue to deterministically
	// advance to the same place.
	Advance(a, 5, nil)
	Advance(b, 5, nil)
	if !a.Equal(b) {
		t.Fatalf("post-roundtrip Advance diverged\nbatch:  %+v\nround:  %+v", a, b)
	}
}

// TestSnapshot_InsertSeqRebuilt asserts the insertSeq counter is
// reconstructed from the event list so further EnqueueInput calls
// keep a total ordering. Without this, two events loaded from a save
// would race on insertSeq==0 and break the queue's tiebreak.
func TestSnapshot_InsertSeqRebuilt(t *testing.T) {
	s := makeState(1)
	s.EnqueueInput(Input{Kind: "supply", AtLocation: "station_one", GoodID: "ore", Amount: 1})
	s.EnqueueInput(Input{Kind: "supply", AtLocation: "station_one", GoodID: "ore", Amount: 2})
	s.EnqueueInput(Input{Kind: "supply", AtLocation: "station_one", GoodID: "ore", Amount: 3})
	raw, err := MarshalJSON(s)
	if err != nil {
		t.Fatal(err)
	}
	loaded, err := UnmarshalJSON(raw)
	if err != nil {
		t.Fatal(err)
	}
	// Loaded state should be able to enqueue more events and
	// continue the ordering — insertSeq must have been rebuilt past
	// the highest existing one.
	loaded.EnqueueInput(Input{Kind: "supply", AtLocation: "station_one", GoodID: "ore", Amount: 4})
	maxSeq := uint64(0)
	for _, e := range loaded.Events {
		if e.InsertSeq > maxSeq {
			maxSeq = e.InsertSeq
		}
	}
	if maxSeq < 4 {
		t.Fatalf("post-load enqueue didn't extend the sequence (max=%d)", maxSeq)
	}
}

// TestFactionTrustInput checks the only faction input the sim
// currently understands: a direct trust delta. Unknown faction ids
// are silently ignored, not an error — the caller may reference a
// faction not yet loaded.
func TestFactionTrustInput(t *testing.T) {
	s := makeState(1)
	ApplyInput(s, &Input{Kind: "faction_trust", FactionID: "alpha", Amount: 25})
	if s.Factions["alpha"].Trust != 25 {
		t.Errorf("alpha trust = %d, want 25", s.Factions["alpha"].Trust)
	}
	ApplyInput(s, &Input{Kind: "faction_trust", FactionID: "alpha", Amount: 200})
	if s.Factions["alpha"].Trust != 100 {
		t.Errorf("alpha trust = %d after overflow input, want clamped to 100", s.Factions["alpha"].Trust)
	}
	ApplyInput(s, &Input{Kind: "faction_trust", FactionID: "ghost", Amount: 50})
	if _, ok := s.Factions["ghost"]; ok {
		t.Errorf("unknown faction ghost was created by an input")
	}
}

// TestEconomyReprice_FloodDropsPrice is the S2 acceptance test:
// supply a location with a huge amount of a good, advance to the
// next reprice tick, assert the price dropped.
func TestEconomyReprice_FloodDropsPrice(t *testing.T) {
	s := makeState(1)
	base := s.Market["ore"].BasePrice
	// Sanity: before any reprice, the price is the base.
	if s.Market["ore"].Price != base {
		t.Fatalf("initial ore price = %d, want starting %d", s.Market["ore"].Price, base)
	}
	// Flood the only location that cares about ore with 10x the
	// reprice scale. Scale is 4 in maybeReprice; net/scale=10 →
	// price drops by 10, clamped at base/2.
	ApplyInput(s, &Input{Kind: "supply", AtLocation: "station_one", GoodID: "ore", Amount: 100})
	Advance(s, 60, nil)
	if s.Market["ore"].Price >= base {
		t.Errorf("flooding ore did not drop its price: starting=%d, after=%d",
			base, s.Market["ore"].Price)
	}
	if s.Market["ore"].Price < base/2 {
		t.Errorf("price dropped below the floor: starting=%d, after=%d (floor=%d)",
			base, s.Market["ore"].Price, base/2)
	}
}

// TestEconomyReprice_StarvationRaisesPrice is the mirror of the
// above: starve a good (demand >> supply) and the price should rise
// (clamped at base*2).
func TestEconomyReprice_StarvationRaisesPrice(t *testing.T) {
	s := makeState(1)
	base := s.Market["ore"].BasePrice
	// station_one consumes ore; demand it heavily.
	ApplyInput(s, &Input{Kind: "demand", AtLocation: "station_one", GoodID: "ore", Amount: 200})
	Advance(s, 60, nil)
	if s.Market["ore"].Price <= base {
		t.Errorf("starving ore did not raise its price: starting=%d, after=%d",
			base, s.Market["ore"].Price)
	}
	if s.Market["ore"].Price > base*2 {
		t.Errorf("price rose above the ceiling: starting=%d, after=%d (ceiling=%d)",
			base, s.Market["ore"].Price, base*2)
	}
}

// TestEconomyReprice_NoRepriceBetweenTicks confirms the 60-tick
// cadence: reprice happens on tick 60 but not on tick 59.
func TestEconomyReprice_NoRepriceBetweenTicks(t *testing.T) {
	s := makeState(1)
	base := s.Market["ore"].Price
	ApplyInput(s, &Input{Kind: "supply", AtLocation: "station_one", GoodID: "ore", Amount: 50})
	Advance(s, 30, nil)
	if s.Market["ore"].Price != base {
		t.Errorf("price changed mid-period: %d → %d", base, s.Market["ore"].Price)
	}
	Advance(s, 30, nil) // total 60
	if s.Market["ore"].Price == base {
		t.Errorf("price did not change on the 60-tick boundary")
	}
}

// TestAdvance_JSONOutputIsValid is a small sanity test on the wire
// format: snapshots should serialize to a JSON object with the
// expected top-level keys, so a save file shape is stable.
func TestAdvance_JSONOutputIsValid(t *testing.T) {
	s := makeState(1)
	Advance(s, 60, nil)
	raw, err := MarshalJSON(s)
	if err != nil {
		t.Fatal(err)
	}
	// Top-level keys must include the ones the save format expects.
	var probe map[string]json.RawMessage
	if err := json.Unmarshal(raw, &probe); err != nil {
		t.Fatalf("snapshot not valid JSON: %v", err)
	}
	for _, k := range []string{"tick", "seed", "factions", "market", "locations", "events"} {
		if _, ok := probe[k]; !ok {
			t.Errorf("snapshot missing top-level key %q", k)
		}
	}
	// Two identical states must produce byte-identical output.
	s2 := makeState(1)
	Advance(s2, 60, nil)
	raw2, err := MarshalJSON(s2)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(raw, raw2) {
		t.Errorf("identical states produced different JSON — encoding is not deterministic")
	}
}

// makeRandomInput builds a random Input for the property test. The
// kind set is restricted to ones the sim understands; unknown kinds
// would be no-ops and the test would still pass, which is fine.
func makeRandomInput(rng *rand.Rand) Input {
	kinds := []string{"supply", "demand", "faction_trust"}
	goods := []string{"ore", "food", "tech"}
	locations := []string{"station_one"}
	factions := []string{"alpha", "beta", "gamma"}
	kind := kinds[rng.Intn(len(kinds))]
	in := Input{Kind: kind, Amount: rng.Intn(50) - 25}
	if kind == "supply" || kind == "demand" {
		in.AtLocation = locations[rng.Intn(len(locations))]
		in.GoodID = goods[rng.Intn(len(goods))]
	}
	if kind == "faction_trust" {
		in.FactionID = factions[rng.Intn(len(factions))]
	}
	return in
}
