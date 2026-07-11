// Package simd is the simulation daemon: the universe tick
// (internal/universe) served over the Sim Protocol — NDJSON over TCP
// loopback, same family as the Soul Protocol. The wire contract lives at
// godot/framework/protocol/SIM-PROTOCOL.md; the fixtures under
// godot/framework/protocol/sim/fixtures/ are the language-neutral truth
// and round-trip through these types in conformance_test.go.
package simd

import (
	"encoding/json"
	"fmt"

	"github.com/chezgoulet/reachlock/server/internal/universe"
)

// ProtocolVersion is the Sim Protocol version this daemon speaks.
const ProtocolVersion = 0

// Profile is the REACHLOCK sim profile accepted at hello.
const Profile = "reachlock-sim/0"

// ServerIdentity is reported in the welcome body; fixture 02 pins it.
const ServerIdentity = "reachlock-simd/0.1.0"

// Envelope is the wire envelope — identical discipline to the Soul
// Protocol: v/seq/re/type/body, re omitted on unsolicited messages.
type Envelope struct {
	V    uint32          `json:"v"`
	Seq  uint64          `json:"seq"`
	Re   *uint64         `json:"re,omitempty"`
	Type string          `json:"type"`
	Body json.RawMessage `json:"body"`
}

// knownTypes is the closed set of message types (both directions).
// A well-formed envelope whose type is outside this set draws
// `error: unknown_type`; a line that is not a well-formed envelope
// draws `error: bad_frame`.
var knownTypes = map[string]bool{
	"hello": true, "welcome": true,
	"advance": true, "advanced": true,
	"apply_input":    true,
	"query_prices":   true, "prices": true,
	"query_factions": true, "factions": true,
	"query_journal":  true, "journal": true,
	"load": true, "shutdown": true,
	"ack": true, "error": true,
}

// Error codes — the closed set for v0.
const (
	CodeBadFrame           = "bad_frame"
	CodeUnknownType        = "unknown_type"
	CodeVersionUnsupported = "version_unsupported"
	CodeUnknownLocation    = "unknown_location"
	CodeInvalidArgs        = "invalid_args"
	CodeBadSnapshot        = "bad_snapshot"
)

// ── Bodies ──────────────────────────────────────────────────────────────────

type HelloBody struct {
	ProtocolVersion uint32 `json:"protocol_version"`
	Profile         string `json:"profile"`
	Client          string `json:"client"`
}

type WelcomeBody struct {
	ProtocolVersion uint32 `json:"protocol_version"`
	Server          string `json:"server"`
	Tick            int64  `json:"tick"`
	Seed            uint64 `json:"seed"`
}

type AdvanceBody struct {
	Ticks int64 `json:"ticks"`
}

type AdvancedBody struct {
	Tick int64 `json:"tick"`
	// Snapshot is the full universe state block (universe.MarshalJSON's
	// shape — the same block the save schema stores). Sent on every
	// advance so the host's save cache is always current.
	Snapshot json.RawMessage `json:"snapshot"`
}

type ApplyInputBody struct {
	Input universe.Input `json:"input"`
}

type QueryPricesBody struct {
	LocationID string `json:"location_id"`
}

type PriceEntry struct {
	GoodID    string `json:"good_id"`
	BasePrice int    `json:"base_price"`
	Price     int    `json:"price"`
	Supply    int    `json:"supply"`
	Demand    int    `json:"demand"`
}

type PricesBody struct {
	LocationID string       `json:"location_id"`
	Tick       int64        `json:"tick"`
	Prices     []PriceEntry `json:"prices"`
}

type FactionEntry struct {
	ID            string            `json:"id"`
	Name          string            `json:"name"`
	Trust         int               `json:"trust"`
	Relationships map[string]string `json:"relationships"`
}

type FactionsBody struct {
	Tick     int64          `json:"tick"`
	Factions []FactionEntry `json:"factions"`
}

type QueryJournalBody struct {
	SinceTick int64 `json:"since_tick"`
}

type JournalBody struct {
	Tick    int64                   `json:"tick"`
	Entries []universe.JournalEntry `json:"entries"`
}

type LoadBody struct {
	Snapshot json.RawMessage `json:"snapshot"`
}

type ErrorBody struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

// ── Line parsing ────────────────────────────────────────────────────────────

// ParseLine stages the parse the way the protocol distinguishes its two
// rejection codes: not-JSON (or not an envelope) → bad_frame; a sound
// envelope with a type outside the closed set → unknown_type.
func ParseLine(line string) (*Envelope, *ErrorBody) {
	var probe map[string]json.RawMessage
	if err := json.Unmarshal([]byte(line), &probe); err != nil {
		return nil, &ErrorBody{Code: CodeBadFrame, Message: fmt.Sprintf("could not parse NDJSON: %v", err)}
	}
	if raw, ok := probe["type"]; ok {
		var ty string
		if err := json.Unmarshal(raw, &ty); err == nil && !knownTypes[ty] {
			return nil, &ErrorBody{Code: CodeUnknownType, Message: fmt.Sprintf("unknown message type `%s`", ty)}
		}
	}
	var env Envelope
	if err := json.Unmarshal([]byte(line), &env); err != nil {
		return nil, &ErrorBody{Code: CodeBadFrame, Message: fmt.Sprintf("could not parse NDJSON: %v", err)}
	}
	if env.Type == "" || env.Body == nil {
		return nil, &ErrorBody{Code: CodeBadFrame, Message: "envelope needs type and body"}
	}
	return &env, nil
}

// Marshal serializes an envelope to one NDJSON line (no trailing newline).
func (e *Envelope) Marshal() ([]byte, error) {
	return json.Marshal(e)
}
