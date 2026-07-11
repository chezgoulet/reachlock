package eard

import (
	"encoding/base64"
	"encoding/json"
	"testing"
)

func newTestSession() *Session {
	return NewSession(&EchoEngine{Text: "I'd do it again. She's crew."})
}

func send(t *testing.T, s *Session, seq uint64, ty string, body any) *Envelope {
	t.Helper()
	raw, err := json.Marshal(body)
	if err != nil {
		t.Fatal(err)
	}
	reply, err := s.Handle(&Envelope{V: ProtocolVersion, Seq: seq, Type: ty, Body: raw})
	if err != nil {
		t.Fatal(err)
	}
	return reply
}

func decodeError(t *testing.T, reply *Envelope) ErrorBody {
	t.Helper()
	if reply == nil || reply.Type != "error" {
		t.Fatalf("expected an error reply, got %+v", reply)
	}
	var body ErrorBody
	if err := json.Unmarshal(reply.Body, &body); err != nil {
		t.Fatal(err)
	}
	return body
}

func TestHelloWelcomeCarriesEngineAndModel(t *testing.T) {
	s := newTestSession()
	reply := send(t, s, 0, "hello", HelloBody{ProtocolVersion: 0, Profile: Profile, Client: "test"})
	if reply.Type != "welcome" {
		t.Fatalf("expected welcome, got %s", reply.Type)
	}
	var body WelcomeBody
	if err := json.Unmarshal(reply.Body, &body); err != nil {
		t.Fatal(err)
	}
	if body.Engine != "echo" || body.Model != "none" || body.Server != ServerIdentity {
		t.Errorf("welcome = %+v", body)
	}
}

func TestVersionMismatchIsRefusedAndCloses(t *testing.T) {
	s := newTestSession()
	raw, _ := json.Marshal(HelloBody{ProtocolVersion: 9, Profile: Profile, Client: "test"})
	env := &Envelope{V: ProtocolVersion, Seq: 0, Type: "hello", Body: raw}
	reply, err := s.Handle(env)
	if err != nil {
		t.Fatal(err)
	}
	if body := decodeError(t, reply); body.Code != CodeVersionUnsupported {
		t.Errorf("code = %s", body.Code)
	}
	if !CloseAfter(env, reply) {
		t.Error("a refused hello must close the connection")
	}
}

func TestUtteranceLifecycleProducesOneTranscript(t *testing.T) {
	s := newTestSession()
	send(t, s, 0, "hello", HelloBody{ProtocolVersion: 0, Profile: Profile, Client: "test"})
	if reply := send(t, s, 1, "audio_begin", AudioBeginBody{
		UtteranceID: "utt_1", SampleRate: SampleRate, Format: Format}); reply != nil {
		t.Fatalf("audio_begin is one-way on success, got %+v", reply)
	}
	pcm := base64.StdEncoding.EncodeToString(make([]byte, 3200)) // 100 ms
	if reply := send(t, s, 2, "audio_chunk", AudioChunkBody{UtteranceID: "utt_1", Data: pcm}); reply != nil {
		t.Fatalf("audio_chunk is one-way on success, got %+v", reply)
	}
	reply := send(t, s, 3, "audio_end", AudioEndBody{UtteranceID: "utt_1"})
	if reply == nil || reply.Type != "transcript" {
		t.Fatalf("expected transcript, got %+v", reply)
	}
	var body TranscriptBody
	if err := json.Unmarshal(reply.Body, &body); err != nil {
		t.Fatal(err)
	}
	if body.Text != "I'd do it again. She's crew." || body.UtteranceID != "utt_1" {
		t.Errorf("transcript = %+v", body)
	}
	if body.DurationMS != 100 {
		t.Errorf("duration = %d ms, want 100", body.DurationMS)
	}
	if reply.Re == nil || *reply.Re != 3 {
		t.Errorf("transcript must answer the audio_end seq")
	}
	// The utterance is spent: a second end is unknown.
	if body := decodeError(t, send(t, s, 4, "audio_end", AudioEndBody{UtteranceID: "utt_1"})); body.Code != CodeUnknownUtterance {
		t.Errorf("code = %s", body.Code)
	}
}

func TestSilenceIsAValidTranscript(t *testing.T) {
	s := newTestSession()
	send(t, s, 0, "audio_begin", AudioBeginBody{UtteranceID: "utt_2", SampleRate: SampleRate, Format: Format})
	reply := send(t, s, 1, "audio_end", AudioEndBody{UtteranceID: "utt_2"})
	var body TranscriptBody
	if err := json.Unmarshal(reply.Body, &body); err != nil {
		t.Fatal(err)
	}
	if body.Text != "" || body.Confidence != 0.0 {
		t.Errorf("zero audio should transcribe to silence, got %+v", body)
	}
}

func TestCancelForgetsTheUtterance(t *testing.T) {
	s := newTestSession()
	send(t, s, 0, "audio_begin", AudioBeginBody{UtteranceID: "utt_3", SampleRate: SampleRate, Format: Format})
	if reply := send(t, s, 1, "cancel", CancelBody{UtteranceID: "utt_3"}); reply.Type != "ack" {
		t.Fatalf("cancel answers ack, got %s", reply.Type)
	}
	// A cancelled utterance MUST NOT produce a transcript.
	if body := decodeError(t, send(t, s, 2, "audio_end", AudioEndBody{UtteranceID: "utt_3"})); body.Code != CodeUnknownUtterance {
		t.Errorf("code = %s", body.Code)
	}
}

func TestErrorPaths(t *testing.T) {
	s := newTestSession()
	if body := decodeError(t, send(t, s, 0, "audio_chunk",
		AudioChunkBody{UtteranceID: "ghost", Data: "AAAA"})); body.Code != CodeUnknownUtterance {
		t.Errorf("chunk for unknown utterance: %s", body.Code)
	}
	if body := decodeError(t, send(t, s, 1, "cancel",
		CancelBody{UtteranceID: "ghost"})); body.Code != CodeUnknownUtterance {
		t.Errorf("cancel for unknown utterance: %s", body.Code)
	}
	if body := decodeError(t, send(t, s, 2, "audio_begin",
		AudioBeginBody{UtteranceID: "utt_4", SampleRate: 44100, Format: Format})); body.Code != CodeInvalidArgs {
		t.Errorf("wrong sample rate: %s", body.Code)
	}
	send(t, s, 3, "audio_begin", AudioBeginBody{UtteranceID: "utt_5", SampleRate: SampleRate, Format: Format})
	if body := decodeError(t, send(t, s, 4, "audio_chunk",
		AudioChunkBody{UtteranceID: "utt_5", Data: "not base64!!!"})); body.Code != CodeInvalidArgs {
		t.Errorf("bad base64: %s", body.Code)
	}
	if body := decodeError(t, send(t, s, 5, "transcript",
		TranscriptBody{UtteranceID: "x"})); body.Code != CodeUnknownType {
		t.Errorf("daemon-to-host type from host: %s", body.Code)
	}
}

func TestParseLineStagesRejections(t *testing.T) {
	if _, reject := ParseLine("not json at all"); reject == nil || reject.Code != CodeBadFrame {
		t.Errorf("bad frame: %+v", reject)
	}
	if _, reject := ParseLine(`{"v":0,"seq":0,"type":"listen","body":{}}`); reject == nil || reject.Code != CodeUnknownType {
		t.Errorf("unknown type: %+v", reject)
	}
	if env, reject := ParseLine(`{"v":0,"seq":0,"type":"ack","body":{}}`); reject != nil || env.Type != "ack" {
		t.Errorf("sound envelope rejected: %+v", reject)
	}
}
