// Package universe is the deterministic simulation engine for REACHLOCK.
// It implements the universe tick contract (C8 in the Sprint 01 brief;
// the full design lives in docs/UNIVERSE-TICK.md). The same API is the
// shape an SP embed would use to drive the same simulation locally —
// only the driver differs.
//
// Determinism property (enforced by the property test in universe_test.go):
//
//	same state + same ordered inputs + same seed  →  same next state
//
// The implementation honors it in three places:
//
//  1. RNG is per-named-stream. Each consumer draws from a stream seeded
//     by (universe_seed, stream_name, tick); no global RNG. Reordering
//     or adding systems never perturbs each other's draws.
//
//  2. The event queue is ordered by (due_tick, priority, insertion_seq).
//     Insertion sequence is the total-ordering tiebreak — never map
//     iteration order or wall time. Same inputs in the same order
//     produce the same fire order.
//
//  3. All simulation quantities are integers or fixed-point. The reprice
//     curve uses integer math. A tick snapshot round-trips losslessly
//     through JSON, so a save/load is bit-for-bit identical.
//
// Batch advancement equals N single-tick calls: Advance(s, n) returns
// the same state as n sequential AdvanceOne calls. This is the property
// that makes "wars start while you're three jumps deep" honest.
package universe

import (
	"encoding/json"
	"fmt"
	"sort"
)

// State is the entire live tick state. It is plain data: copyable,
// JSON-serializable, and the same struct the SP embed will use. Wrap it
// in a mutex at the driver layer (see Manager).
type State struct {
	// Tick is the universe clock. One tick = one in-game minute.
	// Determinism contract: snapshots round-trip the tick, so
	// Advance-from-snapshot equals Advance-from-previous-tick.
	Tick int64 `json:"tick"`
	// Seed is the universe RNG seed. It is part of the state on
	// purpose: a saved game carries its own seed forward, so two
	// players who load the same save with the same inputs see the
	// same universe. (MMO uses the server's seed; SP uses the save's.)
	Seed uint64 `json:"seed"`
	// Factions is the live faction table, keyed by faction id.
	// Trust is an integer in [-100, 100] (the same scale the SP
	// dialogue UI already uses). Stance is one of the framework
	// vocabulary strings: allied | friendly | neutral | tense |
	// hostile | war. Both drift during a tick.
	Factions map[string]*Faction `json:"factions"`
	// Market is the live per-good price table, keyed by good id.
	// Prices are integers (the economy engine scales a base_price
	// by a supply/demand delta; no floats in tick state).
	Market map[string]*GoodPrice `json:"market"`
	// Locations is the live per-location supply/demand snapshot,
	// keyed by location id. These values feed the next reprice tick.
	Locations map[string]*LocationEconomy `json:"locations"`
	// Events is the ordered pending event queue. See EventQueue.
	Events EventQueue `json:"events"`
	// Journal is the emitted-event log: things that HAPPENED during
	// ticks (reprices, stance changes, patrols, skirmishes, trades,
	// fired events), in occurrence order, capped at JournalCap. This
	// is what the in-game news feed renders — every feed item was a
	// real simulation event. Additive field (v0 snapshots load with
	// an empty journal).
	Journal []JournalEntry `json:"journal,omitempty"`
	// insertSeq is the monotonic counter that gives every event a
	// total-ordering tiebreak. Not serialized — it is rebuilt from
	// the event list on snapshot load (see Snapshot).
	insertSeq uint64 `json:"-"`
}

// JournalCap bounds the journal; the oldest entries fall off. 256 is
// hours of in-game news at the current emission rates.
const JournalCap = 256

// JournalEntry is one emitted simulation event. Kind selects which of
// the optional fields are meaningful:
//
//	"reprice"       — GoodID, Amount (new price), Delta (change)
//	"stance_change" — FactionID, With, Stance
//	"patrol"        — FactionID, With (whose space the patrol probes)
//	"skirmish"      — FactionID, With
//	"trade"         — LocationID, GoodID, Amount (+sold to station, -bought)
//	"event_fired"   — EventKind + the fired event's scoping fields
//
// Entries are plain data; the host renders text from them (the engine
// stays content-free — names come from the host's own content load).
type JournalEntry struct {
	Tick       int64  `json:"tick"`
	Kind       string `json:"kind"`
	FactionID  string `json:"faction_id,omitempty"`
	With       string `json:"with,omitempty"`
	Stance     string `json:"stance,omitempty"`
	GoodID     string `json:"good_id,omitempty"`
	LocationID string `json:"location_id,omitempty"`
	Amount     int    `json:"amount,omitempty"`
	Delta      int    `json:"delta,omitempty"`
	EventKind  string `json:"event_kind,omitempty"`
}

// journal appends an entry and enforces the cap.
func (s *State) journal(e JournalEntry) {
	s.Journal = append(s.Journal, e)
	if len(s.Journal) > JournalCap {
		s.Journal = s.Journal[len(s.Journal)-JournalCap:]
	}
}

// JournalSince returns the entries with Tick >= since, oldest first.
// (Entries older than the cap are gone; callers that fall behind more
// than JournalCap entries see a truncated window, which is the deal.)
func (s *State) JournalSince(since int64) []JournalEntry {
	// The journal is append-ordered and ticks are monotonic: binary
	// search would work, but the slice is <= JournalCap long.
	out := make([]JournalEntry, 0, 16)
	for _, e := range s.Journal {
		if e.Tick >= since {
			out = append(out, e)
		}
	}
	return out
}

// Faction is a faction's live state: identity (loaded from content) plus
// runtime values (drift, relationships).
type Faction struct {
	ID            string             `json:"id"`
	Name          string             `json:"name"`
	Goals         []string           `json:"goals"`
	Trust         int                `json:"trust"`
	Relationships map[string]string  `json:"relationships"`
}

// GoodPrice is the live price of a trade good, plus the supply/demand
// delta that produced it. Both are integers.
type GoodPrice struct {
	GoodID     string `json:"good_id"`
	BasePrice  int    `json:"base_price"`
	Price      int    `json:"price"`
	NetDelta   int    `json:"net_delta"`
	LastRepriceTick int64 `json:"last_reprice_tick"`
}

// LocationEconomy is the live supply/demand snapshot at one location.
// It is recomputed from the location's produces/consumes lists at each
// reprice tick; we store it so the next reprice has a baseline.
type LocationEconomy struct {
	LocationID   string         `json:"location_id"`
	SupplyByGood map[string]int `json:"supply_by_good"`
	DemandByGood map[string]int `json:"demand_by_good"`
}

// Input is something the universe consumes between ticks. Inputs are
// the only way the outside world (a player, a script, the network) gets
// to perturb state — there is no shared memory with the sim. The order
// of inputs within a tick matters: two inputs in opposite order can
// produce different states (e.g. a flood-supply event before vs after
// a consume burst).
type Input struct {
	// Kind is a string tag, e.g. "supply", "demand", "trade",
	// "trigger", "rng". The sim knows the framework-level kinds;
	// unknown kinds are logged and skipped.
	Kind string `json:"kind"`
	// AtLocation scopes a supply/demand input to a single location.
	// Empty means the input is universe-wide (e.g. a global rng).
	AtLocation string `json:"at_location,omitempty"`
	// GoodID scopes a supply/demand input to one trade good.
	GoodID string `json:"good_id,omitempty"`
	// Amount is the integer delta applied. Sign convention: positive
	// for supply additions, positive for demand additions.
	Amount int `json:"amount"`
	// FactionID scopes a faction-level input (trust, goal push).
	FactionID string `json:"faction_id,omitempty"`
	// Other carries unknown structured data; ignored by the sim core.
	Other map[string]any `json:"other,omitempty"`
}

// Event is a scheduled future perturbation: "on tick T, do X with
// priority P". Events fire in (due_tick, priority, insertion_seq) order.
type Event struct {
	// ID is a stable identifier the caller can use to cancel the
	// event. The sim assigns one if the input omitted it.
	ID         string `json:"id"`
	DueTick    int64  `json:"due_tick"`
	Priority   int    `json:"priority"`
	InsertSeq  uint64 `json:"insert_seq"`
	// Kind selects the handler (supply, demand, rng, …). Same
	// vocabulary as Input.Kind.
	Kind       string `json:"kind"`
	AtLocation string `json:"at_location,omitempty"`
	GoodID     string `json:"good_id,omitempty"`
	Amount     int    `json:"amount"`
	FactionID  string `json:"faction_id,omitempty"`
}

// EventQueue is the ordered pending event list. Order is determined
// entirely by Less, not by map iteration or wall time. The queue is
// kept sorted on every insert; the sim's expected working set is small
// (dozens, not millions) so this is fine.
type EventQueue []Event

func (q EventQueue) Len() int      { return len(q) }
func (q EventQueue) Swap(i, j int) { q[i], q[j] = q[j], q[i] }
func (q EventQueue) Less(i, j int) bool {
	if q[i].DueTick != q[j].DueTick {
		return q[i].DueTick < q[j].DueTick
	}
	if q[i].Priority != q[j].Priority {
		return q[i].Priority > q[j].Priority // higher priority first
	}
	return q[i].InsertSeq < q[j].InsertSeq
}

// NewState returns a zero state with a seed. It is the starting point
// for both a fresh universe and a deserialized snapshot. The seed is
// required: zero is a valid seed but should be set explicitly so the
// caller can't accidentally pass 0 and break reproducibility.
func NewState(seed uint64) *State {
	return &State{
		Tick:      0,
		Seed:      seed,
		Factions:  map[string]*Faction{},
		Market:    map[string]*GoodPrice{},
		Locations: map[string]*LocationEconomy{},
		Events:    EventQueue{},
	}
}

// AddFaction registers a faction in the live state. Trust starts at
// zero; relationships start at the default the caller supplies
// (typically the authored "tense"/"allied" string from the faction
// file, kept as-is so the dialogue UI and trigger DSL see the same
// values the player loaded with).
func (s *State) AddFaction(id, name string, goals []string, relationships map[string]string) {
	s.Factions[id] = &Faction{
		ID:            id,
		Name:          name,
		Goals:         append([]string(nil), goals...),
		Trust:         0,
		Relationships: cloneStringMap(relationships),
	}
}

// AddGood registers a good and its base price.
func (s *State) AddGood(id string, basePrice int) {
	s.Market[id] = &GoodPrice{
		GoodID:    id,
		BasePrice: basePrice,
		Price:     basePrice,
		NetDelta:  0,
	}
}

// AddLocation registers a location's economy table. Supply/demand for
// each produced/consumed good starts at zero — the next reprice tick
// will fold in any queued inputs.
func (s *State) AddLocation(id string, produces, consumes []string) {
	loc := &LocationEconomy{
		LocationID:   id,
		SupplyByGood: map[string]int{},
		DemandByGood: map[string]int{},
	}
	for _, g := range produces {
		loc.SupplyByGood[g] = 0
	}
	for _, g := range consumes {
		loc.DemandByGood[g] = 0
	}
	s.Locations[id] = loc
}

// EnqueueInput turns an Input into a queued event. Inputs that take
// effect this tick (due at s.Tick) and inputs scheduled for a future
// tick go through the same path; the caller decides by setting the
// input's DueTick (via the Other["due_tick"] field for now) or by
// calling ApplyInput directly for the immediate path.
func (s *State) EnqueueInput(in Input) {
	s.insertSeq++
	due := s.Tick
	if v, ok := in.Other["due_tick"]; ok {
		switch t := v.(type) {
		case float64:
			due = int64(t)
		case int64:
			due = t
		case int:
			due = int64(t)
		}
	}
	priority := 0
	if v, ok := in.Other["priority"]; ok {
		switch t := v.(type) {
		case float64:
			priority = int(t)
		case int64:
			priority = int(t)
		case int:
			priority = t
		}
	}
	id, _ := in.Other["event_id"].(string)
	ev := Event{
		ID:         id,
		DueTick:    due,
		Priority:   priority,
		InsertSeq:  s.insertSeq,
		Kind:       in.Kind,
		AtLocation: in.AtLocation,
		GoodID:     in.GoodID,
		Amount:     in.Amount,
		FactionID:  in.FactionID,
	}
	s.Events = append(s.Events, ev)
	sort.Sort(s.Events)
}

// Advance runs the universe forward by n ticks. Inputs are applied
// at the start of EACH tick, in order — the same input sequence
// is replayed every tick of the batch. This is the contract: the
// same input stream + the same state + the same number of ticks
// always produces the same next state, regardless of how many
// ticks the caller asks the sim to take at once. Replay (e.g. MMO
// server crash recovery, SP autosave restore) depends on this:
// the same 1000-tick input log replays as 1 batch of 1000 or 1000
// batches of 1, with the same outcome.
//
// Per-tick: inputs → fire due events → step factions → maybe reprice
// → tick++. n must be >= 0; n==0 returns the state unchanged.
func Advance(s *State, n int64, inputs []Input) {
	if n < 0 {
		panic("universe: negative tick count")
	}
	for i := int64(0); i < n; i++ {
		for j := range inputs {
			applyInput(s, &inputs[j])
		}
		fireDueEvents(s)
		stepFactions(s)
		maybeStepFactionGoals(s)
		maybeReprice(s)
		s.Tick++
	}
}

// ApplyInput is the immediate path (input takes effect this tick, no
// queueing). Advance calls it per-tick; callers driving the sim
// directly can use it too.
func ApplyInput(s *State, in *Input) { applyInput(s, in) }

// applyInput dispatches an input to its handler. Unknown kinds are
// silently ignored — strict validation is CI's job, the sim just
// needs to keep ticking.
func applyInput(s *State, in *Input) {
	switch in.Kind {
	case "supply":
		if in.AtLocation == "" {
			return
		}
		loc, ok := s.Locations[in.AtLocation]
		if !ok {
			return
		}
		loc.SupplyByGood[in.GoodID] += in.Amount
	case "demand":
		if in.AtLocation == "" {
			return
		}
		loc, ok := s.Locations[in.AtLocation]
		if !ok {
			return
		}
		loc.DemandByGood[in.GoodID] += in.Amount
	case "faction_trust":
		f, ok := s.Factions[in.FactionID]
		if !ok {
			return
		}
		f.Trust = clampTrust(f.Trust + in.Amount)
	case "faction_stance":
		a, aok := s.Factions[in.FactionID]
		if !aok {
			return
		}
		other, _ := in.Other["with"].(string)
		if other == "" {
			return
		}
		if a.Relationships == nil {
			a.Relationships = map[string]string{}
		}
		if stance, _ := in.Other["stance"].(string); stance != "" {
			a.Relationships[other] = stance
			s.journal(JournalEntry{
				Tick: s.Tick, Kind: "stance_change",
				FactionID: in.FactionID, With: other, Stance: stance,
			})
		}
	case "trade":
		// A player (or NPC) trade at a location. Amount > 0: goods sold
		// TO the station (local supply rises); amount < 0: goods bought
		// FROM it (supply falls). Trades are the player-action input the
		// SimGateway sends (P1/P2); they perturb the next reprice and
		// land in the journal — the market remembers you.
		if in.AtLocation == "" || in.GoodID == "" {
			return
		}
		loc, ok := s.Locations[in.AtLocation]
		if !ok {
			return
		}
		loc.SupplyByGood[in.GoodID] += in.Amount
		s.journal(JournalEntry{
			Tick: s.Tick, Kind: "trade",
			LocationID: in.AtLocation, GoodID: in.GoodID, Amount: in.Amount,
		})
	default:
		// Unknown kinds are a no-op. CI catches them at authoring time.
	}
}

// fireDueEvents pops all events due at the current tick and applies
// them in (priority, insertion_seq) order. Sorted insertion +
// sorted-range pop = deterministic fire order.
func fireDueEvents(s *State) {
	// The queue is sorted by (due, priority desc, insert_seq asc).
	// We walk from the front while events are due.
	i := 0
	for i < len(s.Events) && s.Events[i].DueTick <= s.Tick {
		i++
	}
	if i == 0 {
		return
	}
	due := s.Events[:i]
	rest := s.Events[i:]
	for j := range due {
		ev := &due[j]
		in := Input{
			Kind:       ev.Kind,
			AtLocation: ev.AtLocation,
			GoodID:     ev.GoodID,
			Amount:     ev.Amount,
			FactionID:  ev.FactionID,
		}
		applyInput(s, &in)
		s.journal(JournalEntry{
			Tick: s.Tick, Kind: "event_fired", EventKind: ev.Kind,
			FactionID: ev.FactionID, GoodID: ev.GoodID,
			LocationID: ev.AtLocation, Amount: ev.Amount,
		})
	}
	s.Events = rest
}

// Stance ladder for the faction-goal dynamics: index 0 is the friendliest.
var stanceLadder = []string{"allied", "friendly", "neutral", "tense", "hostile", "war"}

func stanceIndex(stance string) int {
	for i, s := range stanceLadder {
		if s == stance {
			return i
		}
	}
	return 2 // unknown stances read as neutral
}

// maybeStepFactionGoals runs the coarse faction-goal evaluation every 1440
// ticks (one in-game day, per UNIVERSE-TICK.md). For each related faction
// pair (canonical order, own RNG stream) there is a small chance the stance
// shifts one step along the ladder — toward war when the pair's combined
// trust is negative, toward allied otherwise — and, in tense-or-worse
// pairs, a chance of a patrol (probing move) or skirmish (war only). Every
// shift and presence event lands in the journal: this is where the news
// feed's "Compact patrol pushing into a neighboring system" comes from —
// a real simulation event, not flavor text.
func maybeStepFactionGoals(s *State) {
	if (s.Tick+1)%1440 != 0 {
		return
	}
	// Canonical pair order: sorted faction ids, each unordered pair once.
	ids := sortedFactionIDs(s)
	for _, a := range ids {
		fa := s.Factions[a]
		for _, b := range sortedRelationshipIDs(fa) {
			if b <= a {
				continue // the (min,max) ordering owns the pair
			}
			fb, ok := s.Factions[b]
			if !ok {
				continue
			}
			r := newStream(s.Seed, "stance."+a+"."+b, s.Tick)
			idx := stanceIndex(fa.Relationships[b])
			if r.IntRange(0, 7) == 0 {
				next := idx
				if fa.Trust+fb.Trust < 0 {
					if idx < len(stanceLadder)-1 {
						next = idx + 1
					}
				} else if idx > 0 {
					next = idx - 1
				}
				if next != idx {
					stance := stanceLadder[next]
					fa.Relationships[b] = stance
					if fb.Relationships != nil {
						if _, mutual := fb.Relationships[a]; mutual {
							fb.Relationships[a] = stance
						}
					}
					idx = next
					s.journal(JournalEntry{
						Tick: s.Tick, Kind: "stance_change",
						FactionID: a, With: b, Stance: stance,
					})
				}
			}
			// Presence events in strained pairs: patrols probe, wars skirmish.
			if idx >= stanceIndex("tense") && r.IntRange(0, 2) == 0 {
				kind := "patrol"
				if stanceLadder[idx] == "war" {
					kind = "skirmish"
				}
				// The mover is the pair member with lower trust — the one
				// with something to prove. Tie: the canonical first.
				mover, target := a, b
				if fb.Trust < fa.Trust {
					mover, target = b, a
				}
				s.journal(JournalEntry{
					Tick: s.Tick, Kind: kind, FactionID: mover, With: target,
				})
			}
		}
	}
}

func sortedFactionIDs(s *State) []string {
	ids := make([]string, 0, len(s.Factions))
	for id := range s.Factions {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids
}

func sortedRelationshipIDs(f *Faction) []string {
	ids := make([]string, 0, len(f.Relationships))
	for id := range f.Relationships {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids
}

// stepFactions runs the per-faction drift. The drift is small and
// stochastic, but uses the faction's own RNG stream so adding another
// system elsewhere can't perturb it. Trust wanders in [-1, 0, +1] per
// tick (the bias is 0 by design — the system has no opinion, content
// pushes opinions through inputs).
func stepFactions(s *State) {
	for id, f := range s.Factions {
		// Stream name encodes the faction so a per-faction draw is
		// independent of every other draw and every other system.
		r := newStream(s.Seed, "faction."+id, s.Tick)
		drift := r.IntRange(-1, 1) // inclusive, symmetric
		f.Trust = clampTrust(f.Trust + drift)
	}
}

// maybeReprice runs the economy reprice on the coarser tick (every 60
// ticks, per UNIVERSE-TICK.md). The price is base - netDelta / scale,
// clamped to [base/2, base*2]. Scale is the same across goods so a
// single number (here 4) calibrates the curve; the test suite asserts
// "flooding supply drops the price" so any positive scale works.
//
// The cadence is "reprice happens *during* the tick that completes a
// 60-tick block" — i.e. the reprice for ticks 1..60 fires while
// closing tick 60. The check is on (s.Tick+1) so the (s.Tick++)
// afterwards lands us on a multiple of 60.
func maybeReprice(s *State) {
	if (s.Tick+1)%60 != 0 {
		return
	}
	const scale = 4
	// 1. The production/consumption pulse — the economy's own motion.
	//    Each location adds supply for what it produces and demand for
	//    what it consumes, plus a small per-(location,good) jittered
	//    draw so no two reprice intervals are identical. Without this
	//    pulse nothing perturbs the economy between player inputs and
	//    "prices differ from yesterday" would be a lie.
	const pulse = 8
	for _, locID := range sortedLocationIDs(s) {
		loc := s.Locations[locID]
		for _, goodID := range sortedIntMapKeys(loc.SupplyByGood) {
			r := newStream(s.Seed, "econ.supply."+locID+"."+goodID, s.Tick)
			loc.SupplyByGood[goodID] += pulse + r.IntRange(-2, 2)
		}
		for _, goodID := range sortedIntMapKeys(loc.DemandByGood) {
			r := newStream(s.Seed, "econ.demand."+locID+"."+goodID, s.Tick)
			loc.DemandByGood[goodID] += pulse + r.IntRange(-2, 2)
		}
	}
	// 2. Reprice from the accumulated supply/demand. Supply lowers the
	//    price, demand raises it; net surplus is the curve input.
	for _, goodID := range sortedGoodIDs(s) {
		g := s.Market[goodID]
		net := 0
		for _, loc := range s.Locations {
			net += loc.SupplyByGood[g.GoodID]
			net -= loc.DemandByGood[g.GoodID]
		}
		delta := net / scale
		price := clampPrice(g.BasePrice-delta, g.BasePrice)
		if price != g.Price {
			s.journal(JournalEntry{
				Tick: s.Tick, Kind: "reprice", GoodID: g.GoodID,
				Amount: price, Delta: price - g.Price,
			})
		}
		g.Price = price
		g.NetDelta = net
		g.LastRepriceTick = s.Tick
	}
	// 3. Decay toward equilibrium: three quarters of each stock/backlog
	//    carries into the next interval, so pulses accumulate to a
	//    bounded steady state (~4× pulse) instead of running away, and
	//    a player trade's price effect fades over a few in-game hours.
	for _, loc := range s.Locations {
		for k, v := range loc.SupplyByGood {
			loc.SupplyByGood[k] = v * 3 / 4
		}
		for k, v := range loc.DemandByGood {
			loc.DemandByGood[k] = v * 3 / 4
		}
	}
}

// clampPrice bounds a computed price to [base/2, base*2], floor 1.
func clampPrice(price, base int) int {
	minPrice := base / 2
	if minPrice < 1 {
		minPrice = 1
	}
	maxPrice := base * 2
	if price < minPrice {
		return minPrice
	}
	if price > maxPrice {
		return maxPrice
	}
	return price
}

// PriceAt derives the LOCAL price of a good at one location: the global
// price pushed by the location's own supply/demand imbalance, on a
// steeper curve (local scarcity bites harder than universe-average).
// Pure function of state — no draws, no mutation — so the daemon can
// answer price queries without perturbing determinism. ok is false when
// the location or good is unknown. A location that neither produces nor
// consumes the good trades it at the global price (supply and demand
// both zero).
func PriceAt(s *State, locationID, goodID string) (price, supply, demand int, ok bool) {
	loc, okLoc := s.Locations[locationID]
	g, okGood := s.Market[goodID]
	if !okLoc || !okGood {
		return 0, 0, 0, false
	}
	const localScale = 2
	supply = loc.SupplyByGood[goodID]
	demand = loc.DemandByGood[goodID]
	price = clampPrice(g.Price-(supply-demand)/localScale, g.BasePrice)
	return price, supply, demand, true
}

func sortedLocationIDs(s *State) []string {
	ids := make([]string, 0, len(s.Locations))
	for id := range s.Locations {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids
}

func sortedGoodIDs(s *State) []string {
	ids := make([]string, 0, len(s.Market))
	for id := range s.Market {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids
}

func sortedIntMapKeys(m map[string]int) []string {
	ids := make([]string, 0, len(m))
	for id := range m {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids
}

// Snapshot returns a deep-enough copy of s that the caller can mutate
// without affecting s. The deep copy is also what the JSON serializer
// operates on, so a serialize-then-load round-trip is bit-for-bit
// identical to the live state.
func Snapshot(s *State) *State {
	out := &State{
		Tick:      s.Tick,
		Seed:      s.Seed,
		Factions:  make(map[string]*Faction, len(s.Factions)),
		Market:    make(map[string]*GoodPrice, len(s.Market)),
		Locations: make(map[string]*LocationEconomy, len(s.Locations)),
		Events:    make(EventQueue, len(s.Events)),
		Journal:   append([]JournalEntry(nil), s.Journal...),
		insertSeq: s.insertSeq,
	}
	for k, v := range s.Factions {
		copy := *v
		copy.Goals = append([]string(nil), v.Goals...)
		copy.Relationships = cloneStringMap(v.Relationships)
		out.Factions[k] = &copy
	}
	for k, v := range s.Market {
		copy := *v
		out.Market[k] = &copy
	}
	for k, v := range s.Locations {
		copy := &LocationEconomy{
			LocationID:   v.LocationID,
			SupplyByGood: cloneIntMap(v.SupplyByGood),
			DemandByGood: cloneIntMap(v.DemandByGood),
		}
		out.Locations[k] = copy
	}
	copy(out.Events, s.Events)
	return out
}

// MarshalJSON serializes a snapshot. It exists so callers can save a
// universe state to a byte slice; the shape matches State.
func MarshalJSON(s *State) ([]byte, error) {
	return json.Marshal(Snapshot(s))
}

// UnmarshalJSON loads a snapshot. The insertSeq counter is rebuilt
// from the event list's max + 1 so further EnqueueInput calls keep a
// total ordering.
//
// Number tolerance: a snapshot that round-tripped through a JSON
// implementation without an integer type (Godot's JSON.stringify writes
// every number as a float — `"trust": -43.0`) must still load. On a
// strict-decode failure the raw JSON is normalized (decode to any,
// re-marshal: Go prints integral floats without the fraction) and
// retried. Corollary in SIM-PROTOCOL.md: numeric fields must stay
// within float64's exact-integer range (|n| <= 2^53) — in particular
// the seed.
func UnmarshalJSON(raw []byte) (*State, error) {
	s := &State{}
	if err := json.Unmarshal(raw, s); err != nil {
		var loose any
		if err2 := json.Unmarshal(raw, &loose); err2 != nil {
			return nil, fmt.Errorf("universe: snapshot decode: %w", err)
		}
		normalized, err2 := json.Marshal(loose)
		if err2 != nil {
			return nil, fmt.Errorf("universe: snapshot decode: %w", err)
		}
		s = &State{}
		if err2 := json.Unmarshal(normalized, s); err2 != nil {
			return nil, fmt.Errorf("universe: snapshot decode: %w", err2)
		}
	}
	if s.Factions == nil {
		s.Factions = map[string]*Faction{}
	}
	if s.Market == nil {
		s.Market = map[string]*GoodPrice{}
	}
	if s.Locations == nil {
		s.Locations = map[string]*LocationEconomy{}
	}
	var maxSeq uint64
	for _, e := range s.Events {
		if e.InsertSeq > maxSeq {
			maxSeq = e.InsertSeq
		}
	}
	s.insertSeq = maxSeq + 1
	return s, nil
}

// --- internals ----------------------------------------------------------

func clampTrust(t int) int {
	if t < -100 {
		return -100
	}
	if t > 100 {
		return 100
	}
	return t
}

func cloneStringMap(m map[string]string) map[string]string {
	if m == nil {
		return nil
	}
	out := make(map[string]string, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

func cloneIntMap(m map[string]int) map[string]int {
	if m == nil {
		return nil
	}
	out := make(map[string]int, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

// Equal reports whether two states are value-equal, ignoring pointer
// identity. Use it in tests; do NOT use reflect.DeepEqual on State —
// the maps hold pointers, and two independently-constructed states
// will have different pointer values for the same logical data.
//
// insertSeq is excluded from the comparison: it is an internal
// monotonic counter, not part of the universe's logical state. A
// snapshot-loaded state may have a different insertSeq than the
// pre-snapshot one and still be functionally identical.
func (s *State) Equal(other *State) bool {
	if s == nil || other == nil {
		return s == other
	}
	if s.Tick != other.Tick || s.Seed != other.Seed {
		return false
	}
	if !stringMapsEqual(s.Factions, other.Factions) {
		return false
	}
	if !goodPriceMapsEqual(s.Market, other.Market) {
		return false
	}
	if !locationEconomyMapsEqual(s.Locations, other.Locations) {
		return false
	}
	if len(s.Journal) != len(other.Journal) {
		return false
	}
	for i := range s.Journal {
		if s.Journal[i] != other.Journal[i] {
			return false
		}
	}
	return eventsEqual(s.Events, other.Events)
}

func stringMapsEqual(a, b map[string]*Faction) bool {
	if len(a) != len(b) {
		return false
	}
	for k, va := range a {
		vb, ok := b[k]
		if !ok {
			return false
		}
		if !factionEqual(va, vb) {
			return false
		}
	}
	return true
}

func factionEqual(a, b *Faction) bool {
	if a.ID != b.ID || a.Name != b.Name || a.Trust != b.Trust {
		return false
	}
	if !stringSlicesEqual(a.Goals, b.Goals) {
		return false
	}
	if len(a.Relationships) != len(b.Relationships) {
		return false
	}
	for k, v := range a.Relationships {
		if b.Relationships[k] != v {
			return false
		}
	}
	return true
}

func goodPriceMapsEqual(a, b map[string]*GoodPrice) bool {
	if len(a) != len(b) {
		return false
	}
	for k, va := range a {
		vb, ok := b[k]
		if !ok || *va != *vb {
			return false
		}
	}
	return true
}

func locationEconomyMapsEqual(a, b map[string]*LocationEconomy) bool {
	if len(a) != len(b) {
		return false
	}
	for k, va := range a {
		vb, ok := b[k]
		if !ok {
			return false
		}
		if va.LocationID != vb.LocationID {
			return false
		}
		if !intMapsEqual(va.SupplyByGood, vb.SupplyByGood) {
			return false
		}
		if !intMapsEqual(va.DemandByGood, vb.DemandByGood) {
			return false
		}
	}
	return true
}

func intMapsEqual(a, b map[string]int) bool {
	if len(a) != len(b) {
		return false
	}
	for k, v := range a {
		if b[k] != v {
			return false
		}
	}
	return true
}

func eventsEqual(a, b EventQueue) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		// InsertSeq is excluded from equality: it is a total-ordering
		// tiebreak, not a logical event attribute.
		if a[i].DueTick != b[i].DueTick ||
			a[i].Priority != b[i].Priority ||
			a[i].ID != b[i].ID ||
			a[i].Kind != b[i].Kind ||
			a[i].AtLocation != b[i].AtLocation ||
			a[i].GoodID != b[i].GoodID ||
			a[i].Amount != b[i].Amount ||
			a[i].FactionID != b[i].FactionID {
			return false
		}
	}
	return true
}

func stringSlicesEqual(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
