package simd

import (
	"encoding/json"
	"fmt"
	"sort"

	"github.com/chezgoulet/reachlock/server/internal/universe"
)

// maxAdvanceTicks caps one advance request. A year of in-game minutes is
// ~525k ticks; anything past that is a host bug, not a time skip.
const maxAdvanceTicks = 1_000_000

// Session is the per-connection protocol state machine. The universe
// state is NOT per-session — the daemon owns it across connections (a
// reconnecting host finds the universe where it left it); the session
// only owns the outgoing seq counter.
type Session struct {
	state   *universe.State
	nextSeq uint64
}

// NewSession wraps the daemon's live state for one connection.
func NewSession(state *universe.State) *Session {
	return &Session{state: state}
}

// allocSeq draws the next outgoing seq (sender-local monotonic).
func (s *Session) allocSeq() uint64 {
	n := s.nextSeq
	s.nextSeq++
	return n
}

// out builds an outgoing envelope answering inbound seq `re`.
func (s *Session) out(re uint64, ty string, body any) (*Envelope, error) {
	raw, err := json.Marshal(body)
	if err != nil {
		return nil, err
	}
	reCopy := re
	return &Envelope{V: ProtocolVersion, Seq: s.allocSeq(), Re: &reCopy, Type: ty, Body: raw}, nil
}

// RejectLine wraps a parse-stage ErrorBody in an envelope (no `re`: the
// offending line may not even have had a seq).
func (s *Session) RejectLine(reject *ErrorBody) (*Envelope, error) {
	raw, err := json.Marshal(reject)
	if err != nil {
		return nil, err
	}
	return &Envelope{V: ProtocolVersion, Seq: s.allocSeq(), Type: "error", Body: raw}, nil
}

func (s *Session) errorOut(re uint64, code, message string) (*Envelope, error) {
	return s.out(re, "error", ErrorBody{Code: code, Message: message})
}

// CloseAfter reports whether the connection must close after answering
// this envelope (shutdown, or a version-mismatched hello).
func CloseAfter(env *Envelope, reply *Envelope) bool {
	if env.Type == "shutdown" {
		return true
	}
	if env.Type == "hello" && reply != nil && reply.Type == "error" {
		return true
	}
	return false
}

// Handle processes one inbound envelope and returns the reply.
func (s *Session) Handle(env *Envelope) (*Envelope, error) {
	switch env.Type {
	case "hello":
		return s.onHello(env)
	case "advance":
		return s.onAdvance(env)
	case "apply_input":
		return s.onApplyInput(env)
	case "query_prices":
		return s.onQueryPrices(env)
	case "query_factions":
		return s.onQueryFactions(env)
	case "query_journal":
		return s.onQueryJournal(env)
	case "load":
		return s.onLoad(env)
	case "shutdown":
		return s.out(env.Seq, "ack", struct{}{})
	default:
		// Known types that are daemon-to-host only.
		return s.errorOut(env.Seq, CodeUnknownType,
			fmt.Sprintf("`%s` is daemon-to-host only", env.Type))
	}
}

func (s *Session) onHello(env *Envelope) (*Envelope, error) {
	var body HelloBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "hello body shape")
	}
	if body.ProtocolVersion != ProtocolVersion || body.Profile != Profile {
		return s.errorOut(env.Seq, CodeVersionUnsupported, fmt.Sprintf(
			"protocol_version/profile mismatch: client=%d %q, daemon=%d %q",
			body.ProtocolVersion, body.Profile, ProtocolVersion, Profile))
	}
	return s.out(env.Seq, "welcome", WelcomeBody{
		ProtocolVersion: ProtocolVersion,
		Server:          ServerIdentity,
		Tick:            s.state.Tick,
		Seed:            s.state.Seed,
	})
}

func (s *Session) onAdvance(env *Envelope) (*Envelope, error) {
	var body AdvanceBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "advance body shape")
	}
	if body.Ticks < 0 || body.Ticks > maxAdvanceTicks {
		return s.errorOut(env.Seq, CodeInvalidArgs,
			fmt.Sprintf("ticks must be in [0, %d], got %d", maxAdvanceTicks, body.Ticks))
	}
	// One-shot inputs never ride an advance: universe.Advance re-applies
	// its inputs argument every tick of the batch (deep review finding
	// #6). Player actions arrive via apply_input, exactly once.
	universe.Advance(s.state, body.Ticks, nil)
	snapshot, err := universe.MarshalJSON(s.state)
	if err != nil {
		return s.errorOut(env.Seq, CodeBadSnapshot, fmt.Sprintf("snapshot encode: %v", err))
	}
	return s.out(env.Seq, "advanced", AdvancedBody{Tick: s.state.Tick, Snapshot: snapshot})
}

func (s *Session) onApplyInput(env *Envelope) (*Envelope, error) {
	var body ApplyInputBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "apply_input body shape")
	}
	if body.Input.Kind == "" {
		return s.errorOut(env.Seq, CodeInvalidArgs, "input needs a kind")
	}
	universe.ApplyInput(s.state, &body.Input)
	return s.out(env.Seq, "ack", struct{}{})
}

func (s *Session) onQueryPrices(env *Envelope) (*Envelope, error) {
	var body QueryPricesBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "query_prices body shape")
	}
	if _, ok := s.state.Locations[body.LocationID]; !ok {
		return s.errorOut(env.Seq, CodeUnknownLocation,
			fmt.Sprintf("location_id '%s' is not in the loaded content", body.LocationID))
	}
	goodIDs := make([]string, 0, len(s.state.Market))
	for id := range s.state.Market {
		goodIDs = append(goodIDs, id)
	}
	sort.Strings(goodIDs)
	prices := make([]PriceEntry, 0, len(goodIDs))
	for _, goodID := range goodIDs {
		price, supply, demand, _ := universe.PriceAt(s.state, body.LocationID, goodID)
		prices = append(prices, PriceEntry{
			GoodID:    goodID,
			BasePrice: s.state.Market[goodID].BasePrice,
			Price:     price,
			Supply:    supply,
			Demand:    demand,
		})
	}
	return s.out(env.Seq, "prices", PricesBody{
		LocationID: body.LocationID, Tick: s.state.Tick, Prices: prices,
	})
}

func (s *Session) onQueryFactions(env *Envelope) (*Envelope, error) {
	ids := make([]string, 0, len(s.state.Factions))
	for id := range s.state.Factions {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	factions := make([]FactionEntry, 0, len(ids))
	for _, id := range ids {
		f := s.state.Factions[id]
		factions = append(factions, FactionEntry{
			ID: f.ID, Name: f.Name, Trust: f.Trust, Relationships: f.Relationships,
		})
	}
	return s.out(env.Seq, "factions", FactionsBody{Tick: s.state.Tick, Factions: factions})
}

func (s *Session) onQueryJournal(env *Envelope) (*Envelope, error) {
	var body QueryJournalBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "query_journal body shape")
	}
	entries := s.state.JournalSince(body.SinceTick)
	if entries == nil {
		entries = []universe.JournalEntry{}
	}
	return s.out(env.Seq, "journal", JournalBody{Tick: s.state.Tick, Entries: entries})
}

func (s *Session) onLoad(env *Envelope) (*Envelope, error) {
	var body LoadBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "load body shape")
	}
	loaded, err := universe.UnmarshalJSON(body.Snapshot)
	if err != nil {
		return s.errorOut(env.Seq, CodeBadSnapshot, err.Error())
	}
	*s.state = *loaded
	return s.out(env.Seq, "ack", struct{}{})
}
