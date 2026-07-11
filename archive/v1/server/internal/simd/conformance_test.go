package simd

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

// fixturesDir locates godot/framework/protocol/sim/fixtures relative to
// this package (server/internal/simd). One repo, one copy — no
// byte-identity dance like the Soul Protocol needs across repos.
func fixturesDir(t *testing.T) string {
	t.Helper()
	dir := filepath.Join("..", "..", "..", "godot", "framework", "protocol", "sim", "fixtures")
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
	if count != 15 {
		t.Errorf("expected 15 fixtures, found %d", count)
	}
	// Coverage: every one of the 15 message types has a fixture.
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
	var advanced AdvancedBody
	if err := json.Unmarshal(load("04_advanced.json").Body, &advanced); err != nil || advanced.Tick != 181 {
		t.Errorf("advanced body: %v (tick %d)", err, advanced.Tick)
	}
	var prices PricesBody
	if err := json.Unmarshal(load("08_prices.json").Body, &prices); err != nil || len(prices.Prices) != 1 {
		t.Errorf("prices body: %v", err)
	} else if prices.Prices[0].Price != 18 {
		t.Errorf("prices fixture price = %d, want 18", prices.Prices[0].Price)
	}
	var journal JournalBody
	if err := json.Unmarshal(load("12_journal.json").Body, &journal); err != nil || len(journal.Entries) != 3 {
		t.Errorf("journal body: %v (%d entries)", err, len(journal.Entries))
	}
}
