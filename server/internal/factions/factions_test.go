package factions

import (
	"os"
	"path/filepath"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/chezgoulet/reachlock/server/internal/loader"
	"github.com/chezgoulet/reachlock/server/internal/universe"
)

// testSetup wires a logger, universe state, and a faction handler
// set backed by a small in-memory faction set. It is the
// scaffolding every test in this file uses; the live content
// integration test is in TestNew_LiveContent.
func testSetup(t *testing.T) (*Factions, *universe.State, *httptest.Server) {
	t.Helper()
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	state := universe.NewState(42)
	content := &loader.Result{
		Factions: map[string]loader.Faction{
			"alpha": {ID: "alpha", Name: "Alpha", Goals: []string{"expand"}},
			"beta":  {ID: "beta", Name: "Beta", Goals: []string{"defend"}},
		},
	}
	f := New(log, state, content)
	mux := http.NewServeMux()
	f.RegisterRoutes(mux)
	return f, state, httptest.NewServer(mux)
}

func TestList(t *testing.T) {
	_, _, srv := testSetup(t)
	defer srv.Close()
	resp, err := http.Get(srv.URL + "/api/factions")
	if err != nil {
		t.Fatalf("request failed: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200", resp.StatusCode)
	}
	body, _ := io.ReadAll(resp.Body)
	var payload struct {
		Factions []string `json:"factions"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		t.Fatalf("body is not valid JSON: %v (%s)", err, body)
	}
	if len(payload.Factions) != 2 {
		t.Errorf("got %d factions, want 2: %v", len(payload.Factions), payload.Factions)
	}
	// The set should be stable and ordered. Use a map for membership
	// so the test doesn't depend on a specific order.
	want := map[string]bool{"alpha": true, "beta": true}
	for _, id := range payload.Factions {
		if !want[id] {
			t.Errorf("unexpected faction id %q in list", id)
		}
	}
}

func TestGet(t *testing.T) {
	_, state, srv := testSetup(t)
	defer srv.Close()
	// Move a faction's trust a bit so we can assert the live value
	// round-trips through HTTP.
	universe.ApplyInput(state, &universe.Input{Kind: "faction_trust", FactionID: "alpha", Amount: 30})

	resp, err := http.Get(srv.URL + "/api/factions/alpha")
	if err != nil {
		t.Fatalf("request failed: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200", resp.StatusCode)
	}
	body, _ := io.ReadAll(resp.Body)
	var payload struct {
		ID   string `json:"id"`
		Name string `json:"name"`
		Trust int   `json:"trust"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		t.Fatalf("body is not valid JSON: %v (%s)", err, body)
	}
	if payload.ID != "alpha" || payload.Name != "Alpha" || payload.Trust != 30 {
		t.Errorf("got %+v, want {alpha Alpha 30}", payload)
	}
}

func TestGet_Unknown(t *testing.T) {
	_, _, srv := testSetup(t)
	defer srv.Close()
	resp, err := http.Get(srv.URL + "/api/factions/ghost")
	if err != nil {
		t.Fatalf("request failed: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusNotFound {
		t.Errorf("status = %d, want 404 for unknown faction", resp.StatusCode)
	}
}

// TestNew_NilContent: the constructor must accept a nil content
// result (the caller's loader may have failed to even produce one)
// without panicking. This is the "no mods loaded" path the brief
// calls out.
func TestNew_NilContent(t *testing.T) {
	defer func() {
		if r := recover(); r != nil {
			t.Fatalf("New with nil content panicked: %v", r)
		}
	}()
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	state := universe.NewState(1)
	New(log, state, nil)
	if len(state.Factions) != 0 {
		t.Errorf("nil content should leave state.Factions empty, got %d", len(state.Factions))
	}
}

// TestNew_EmptyContent: a non-nil but empty content result (the
// mods dir exists but has no factions) must boot the engine with
// zero factions, not crash.
func TestNew_EmptyContent(t *testing.T) {
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	state := universe.NewState(1)
	content := &loader.Result{
		Factions:  map[string]loader.Faction{},
		Locations: map[string]loader.Location{},
		Goods:     map[string]loader.Good{},
	}
	New(log, state, content)
	if len(state.Factions) != 0 {
		t.Errorf("empty content should leave state.Factions empty, got %d", len(state.Factions))
	}
}

// TestNew_LiveContent: the integration test the brief asks for —
// load the actual godot/mods/reachlock content from disk and assert
// the engine boots without crashing on it. The five REACHLOCK
// factions declared in the manifest may or may not all have data
// files (three of them are on the handoff), but the loader and the
// factions handler must cope with whatever the content side has
// produced. The point is "no panic, no hardcoded ids needed".
func TestNew_LiveContent(t *testing.T) {
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	state := universe.NewState(1)
	// The repo layout is fixed: reachlock-sprint01-server/godot/mods.
	// We resolve from the working directory; the test runs from the
	// server module root, which is one level under the repo.
	content, err := loader.Load(findRepoMods(t), []string{"factions", "locations", "goods"})
	if err != nil {
		t.Fatalf("loader.Load: %v", err)
	}
	t.Logf("loaded %d mods, %d factions, %d locations, %d goods",
		len(content.Mods), len(content.Factions), len(content.Locations), len(content.Goods))
	New(log, state, content)
	// Boot must not crash. We don't assert a specific faction count
	// because content will grow over the sprint.
	for id, f := range state.Factions {
		if f.ID != id {
			t.Errorf("faction id mismatch: map key=%q, value.ID=%q", id, f.ID)
		}
	}
}

// findRepoMods walks up from this package's directory to find the
// repo's godot/mods directory. See loader.Loader_test.go for the
// detailed rationale; same idea, copy-pasted so each package's
// tests are self-contained.
func findRepoMods(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 8; i++ {
		candidate := filepath.Join(dir, "godot", "mods")
		if info, err := os.Stat(candidate); err == nil && info.IsDir() {
			return candidate
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	t.Skip("could not locate godot/mods from the package directory — skipping live content test")
	return ""
}
