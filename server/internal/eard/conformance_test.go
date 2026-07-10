package eard

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

// fixturesDir locates godot/framework/protocol/ear/fixtures relative to
// this package (server/internal/eard).
func fixturesDir(t *testing.T) string {
	t.Helper()
	dir := filepath.Join("..", "..", "..", "godot", "framework", "protocol", "ear", "fixtures")
	if _, err := os.Stat(dir); err != nil {
		t.Fatalf("fixtures dir not found at %s: %v", dir, err)
	}
	return dir
}

// Every fixture must decode into the wire Envelope, re-serialize, and
// re-parse to the same JSON value. If a fixture stops fitting, the
// contract drifted — fix the code or bump the protocol deliberately;
// never edit a fixture to make an implementation pass.
func TestEveryFixtureRoundTripsThroughWireTypes(t *testing.T) {
	dir := fixturesDir(t)
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	count := 0
	types := map[string]bool{}
	for _, e := range entries {
		if filepath.Ext(e.Name()) != ".json" {
			continue
		}
		count++
		raw, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil {
			t.Fatal(err)
		}
		var fixture struct {
			Direction string          `json:"direction"`
			Message   json.RawMessage `json:"message"`
		}
		if err := json.Unmarshal(raw, &fixture); err != nil {
			t.Errorf("%s: fixture shape: %v", e.Name(), err)
			continue
		}
		if fixture.Direction != "host_to_daemon" && fixture.Direction != "daemon_to_host" {
			t.Errorf("%s: bad direction %q", e.Name(), fixture.Direction)
		}
		env, reject := ParseLine(string(fixture.Message))
		if reject != nil {
			t.Errorf("%s: ParseLine rejected the fixture: %s %s", e.Name(), reject.Code, reject.Message)
			continue
		}
		types[env.Type] = true
		out, err := env.Marshal()
		if err != nil {
			t.Errorf("%s: marshal: %v", e.Name(), err)
			continue
		}
		var a, b any
		if err := json.Unmarshal(fixture.Message, &a); err != nil {
			t.Fatal(err)
		}
		if err := json.Unmarshal(out, &b); err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(a, b) {
			t.Errorf("%s: round-trip drifted:\n  in:  %s\n  out: %s", e.Name(), fixture.Message, out)
		}
	}
	if count != 13 {
		t.Errorf("expected 13 fixtures, found %d", count)
	}
	// Coverage: every one of the 11 message types has a fixture.
	for ty := range knownTypes {
		if !types[ty] {
			t.Errorf("no fixture covers message type %q", ty)
		}
	}
}

// The typed bodies must decode from their fixture bodies — a field
// rename in the Go structs breaks here, not in production.
func TestFixtureBodiesDecodeIntoTypedBodies(t *testing.T) {
	dir := fixturesDir(t)
	load := func(name string) *Envelope {
		raw, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil {
			t.Fatal(err)
		}
		var fixture struct {
			Message json.RawMessage `json:"message"`
		}
		if err := json.Unmarshal(raw, &fixture); err != nil {
			t.Fatal(err)
		}
		env, reject := ParseLine(string(fixture.Message))
		if reject != nil {
			t.Fatalf("%s: %s", name, reject.Message)
		}
		return env
	}

	var hello HelloBody
	if err := json.Unmarshal(load("01_hello.json").Body, &hello); err != nil || hello.Profile != Profile {
		t.Errorf("hello body: %v (profile %q)", err, hello.Profile)
	}
	var welcome WelcomeBody
	if err := json.Unmarshal(load("02_welcome.json").Body, &welcome); err != nil || welcome.Server != ServerIdentity {
		t.Errorf("welcome body: %v (server %q)", err, welcome.Server)
	}
	var begin AudioBeginBody
	if err := json.Unmarshal(load("03_audio_begin.json").Body, &begin); err != nil ||
		begin.SampleRate != SampleRate || begin.Format != Format {
		t.Errorf("audio_begin body: %v (%d %q)", err, begin.SampleRate, begin.Format)
	}
	var transcript TranscriptBody
	if err := json.Unmarshal(load("07_transcript.json").Body, &transcript); err != nil ||
		transcript.Text == "" || transcript.Confidence <= 0 {
		t.Errorf("transcript body: %v", err)
	}
	var silence TranscriptBody
	if err := json.Unmarshal(load("08_transcript_silence.json").Body, &silence); err != nil || silence.Text != "" {
		t.Errorf("silence transcript body: %v (text %q)", err, silence.Text)
	}
}
