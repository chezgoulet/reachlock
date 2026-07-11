// Package eard is the speech daemon: push-to-talk audio in, transcripts
// out, served over the Ear Protocol — NDJSON over TCP loopback, same
// family as the Soul and Sim Protocols. The wire contract lives at
// godot/framework/protocol/EAR-PROTOCOL.md; the fixtures under
// godot/framework/protocol/ear/fixtures/ are the language-neutral truth
// and round-trip through these types in conformance_test.go.
//
// Voice is an input method: the only thing this daemon ever produces is
// text. Choice matching, minds, and world effects are the host's business.
package eard

import (
	"encoding/json"
	"fmt"
)

// ProtocolVersion is the Ear Protocol version this daemon speaks.
const ProtocolVersion = 0

// Profile is the REACHLOCK profile accepted at hello.
const Profile = "reachlock/0"

// ServerIdentity is reported in the welcome body; fixture 02 pins it.
const ServerIdentity = "reachlock-eard/0.1.0"

// SampleRate is the only sample rate v0 accepts (whisper wants 16 kHz mono).
const SampleRate = 16000

// Format is the only audio format v0 accepts.
const Format = "pcm16"

// Envelope is the wire envelope — identical discipline to its siblings:
// v/seq/re/type/body, re omitted on unsolicited messages.
type Envelope struct {
	V    uint32          `json:"v"`
	Seq  uint64          `json:"seq"`
	Re   *uint64         `json:"re,omitempty"`
	Type string          `json:"type"`
	Body json.RawMessage `json:"body"`
}

// knownTypes is the closed set of message types (both directions).
var knownTypes = map[string]bool{
	"hello": true, "welcome": true,
	"audio_begin": true, "audio_chunk": true, "audio_end": true,
	"partial": true, "transcript": true,
	"cancel": true, "shutdown": true,
	"ack": true, "error": true,
}

// Error codes — the closed set for v0.
const (
	CodeBadFrame           = "bad_frame"
	CodeUnknownType        = "unknown_type"
	CodeVersionUnsupported = "version_unsupported"
	CodeUnknownUtterance   = "unknown_utterance"
	CodeInvalidArgs        = "invalid_args"
	CodeNoModel            = "no_model"
	CodeDecodeFailure      = "decode_failure"
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
	Engine          string `json:"engine"`
	Model           string `json:"model"`
}

type AudioBeginBody struct {
	UtteranceID string `json:"utterance_id"`
	SampleRate  int    `json:"sample_rate"`
	Format      string `json:"format"`
}

type AudioChunkBody struct {
	UtteranceID string `json:"utterance_id"`
	Data        string `json:"data"` // base64 PCM16 LE mono
}

type AudioEndBody struct {
	UtteranceID string `json:"utterance_id"`
}

type PartialBody struct {
	UtteranceID string `json:"utterance_id"`
	Text        string `json:"text"`
}

type TranscriptBody struct {
	UtteranceID string  `json:"utterance_id"`
	Text        string  `json:"text"`
	Confidence  float64 `json:"confidence"`
	DurationMS  int64   `json:"duration_ms"`
}

type CancelBody struct {
	UtteranceID string `json:"utterance_id"`
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
