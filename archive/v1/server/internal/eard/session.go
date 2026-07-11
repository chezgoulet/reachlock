package eard

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
)

// maxUtteranceBytes caps buffered audio per utterance: 60 s of 16 kHz
// PCM16 is ~1.9 MB; anything past that is a stuck key, not a sentence.
const maxUtteranceBytes = SampleRate * 2 * 60

// Session is the per-connection protocol state machine. Unlike the sim,
// there is nothing worth keeping across connections: an utterance in
// flight when the host goes away is simply gone (the player says it again).
type Session struct {
	engine     Engine
	nextSeq    uint64
	utterances map[string][]byte // in-flight utterance id -> PCM buffer
}

// NewSession wraps the daemon's engine for one connection.
func NewSession(engine Engine) *Session {
	return &Session{engine: engine, utterances: map[string][]byte{}}
}

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

// unsolicited builds an outgoing envelope with no `re` (errors about
// lines that answer nothing).
func (s *Session) unsolicited(ty string, body any) (*Envelope, error) {
	raw, err := json.Marshal(body)
	if err != nil {
		return nil, err
	}
	return &Envelope{V: ProtocolVersion, Seq: s.allocSeq(), Type: ty, Body: raw}, nil
}

// RejectLine wraps a parse-stage ErrorBody in an envelope.
func (s *Session) RejectLine(reject *ErrorBody) (*Envelope, error) {
	return s.unsolicited("error", reject)
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

// Handle processes one inbound envelope. A nil reply with nil error means
// "nothing to write" — audio_begin and audio_chunk are one-way on success.
func (s *Session) Handle(env *Envelope) (*Envelope, error) {
	switch env.Type {
	case "hello":
		return s.onHello(env)
	case "audio_begin":
		return s.onAudioBegin(env)
	case "audio_chunk":
		return s.onAudioChunk(env)
	case "audio_end":
		return s.onAudioEnd(env)
	case "cancel":
		return s.onCancel(env)
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
		Engine:          s.engine.Name(),
		Model:           s.engine.Model(),
	})
}

func (s *Session) onAudioBegin(env *Envelope) (*Envelope, error) {
	var body AudioBeginBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "audio_begin body shape")
	}
	if body.UtteranceID == "" {
		return s.errorOut(env.Seq, CodeInvalidArgs, "audio_begin needs an utterance_id")
	}
	if body.SampleRate != SampleRate || body.Format != Format {
		return s.errorOut(env.Seq, CodeInvalidArgs, fmt.Sprintf(
			"v0 audio is %d Hz %s; got %d Hz %q", SampleRate, Format, body.SampleRate, body.Format))
	}
	if _, exists := s.utterances[body.UtteranceID]; exists {
		return s.errorOut(env.Seq, CodeInvalidArgs,
			fmt.Sprintf("utterance %s is already open", body.UtteranceID))
	}
	s.utterances[body.UtteranceID] = []byte{}
	return nil, nil // one-way on success
}

func (s *Session) onAudioChunk(env *Envelope) (*Envelope, error) {
	var body AudioChunkBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "audio_chunk body shape")
	}
	buf, ok := s.utterances[body.UtteranceID]
	if !ok {
		return s.errorOut(env.Seq, CodeUnknownUtterance,
			fmt.Sprintf("no utterance %s in flight", body.UtteranceID))
	}
	pcm, err := base64.StdEncoding.DecodeString(body.Data)
	if err != nil {
		return s.errorOut(env.Seq, CodeInvalidArgs, "audio_chunk data is not base64")
	}
	if len(buf)+len(pcm) > maxUtteranceBytes {
		delete(s.utterances, body.UtteranceID)
		return s.errorOut(env.Seq, CodeInvalidArgs,
			fmt.Sprintf("utterance %s exceeded %d bytes; dropped", body.UtteranceID, maxUtteranceBytes))
	}
	s.utterances[body.UtteranceID] = append(buf, pcm...)
	return nil, nil // one-way on success
}

func (s *Session) onAudioEnd(env *Envelope) (*Envelope, error) {
	var body AudioEndBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "audio_end body shape")
	}
	pcm, ok := s.utterances[body.UtteranceID]
	if !ok {
		return s.errorOut(env.Seq, CodeUnknownUtterance,
			fmt.Sprintf("no utterance %s in flight", body.UtteranceID))
	}
	delete(s.utterances, body.UtteranceID)
	text, confidence, err := s.engine.Decode(pcm)
	if err != nil {
		return s.errorOut(env.Seq, CodeDecodeFailure, err.Error())
	}
	return s.out(env.Seq, "transcript", TranscriptBody{
		UtteranceID: body.UtteranceID,
		Text:        text,
		Confidence:  confidence,
		DurationMS:  int64(len(pcm)) * 1000 / (SampleRate * 2),
	})
}

func (s *Session) onCancel(env *Envelope) (*Envelope, error) {
	var body CancelBody
	if err := json.Unmarshal(env.Body, &body); err != nil {
		return s.errorOut(env.Seq, CodeBadFrame, "cancel body shape")
	}
	if _, ok := s.utterances[body.UtteranceID]; !ok {
		return s.errorOut(env.Seq, CodeUnknownUtterance,
			fmt.Sprintf("no utterance %s in flight", body.UtteranceID))
	}
	delete(s.utterances, body.UtteranceID)
	return s.out(env.Seq, "ack", struct{}{})
}
