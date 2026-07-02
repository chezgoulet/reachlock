package economy

import (
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/chezgoulet/reachlock/server/internal/loader"
	"github.com/chezgoulet/reachlock/server/internal/universe"
)

func testSetup(t *testing.T) (*Economy, *universe.State, *httptest.Server) {
	t.Helper()
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	state := universe.NewState(42)
	content := &loader.Result{
		Goods: map[string]loader.Good{
			"ore":  {ID: "ore", BasePrice: 10},
			"food": {ID: "food", BasePrice: 8},
		},
		Locations: map[string]loader.Location{
			"station_one": {ID: "station_one", Produces: []string{"food"}, Consumes: []string{"ore"}},
		},
	}
	e := New(log, state, content)
	mux := http.NewServeMux()
	e.RegisterRoutes(mux)
	return e, state, httptest.NewServer(mux)
}

func TestPrices(t *testing.T) {
	_, _, srv := testSetup(t)
	defer srv.Close()
	resp, err := http.Get(srv.URL + "/api/economy/prices")
	if err != nil {
		t.Fatalf("request failed: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200", resp.StatusCode)
	}
	if ct := resp.Header.Get("Content-Type"); ct != "application/json" {
		t.Errorf("content-type = %q, want application/json", ct)
	}
	body, _ := io.ReadAll(resp.Body)
	var payload struct {
		Prices map[string]struct {
			BasePrice int `json:"base_price"`
			Price     int `json:"price"`
		} `json:"prices"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		t.Fatalf("body is not valid JSON: %v (%s)", err, body)
	}
	// Initial prices should equal base prices; the first reprice
	// happens on tick 60, and we haven't ticked yet.
	if got, want := payload.Prices["ore"].Price, 10; got != want {
		t.Errorf("ore price = %d, want %d (the starting price before the first reprice)", got, want)
	}
	if got, want := payload.Prices["food"].Price, 8; got != want {
		t.Errorf("food price = %d, want %d (the starting price before the first reprice)", got, want)
	}
}

// TestPrices_AfterReprice drives a reprice through the HTTP layer
// and asserts the live price is what the universe computed. This is
// the S2 acceptance test in wire form.
func TestPrices_AfterReprice(t *testing.T) {
	e, state, srv := testSetup(t)
	defer srv.Close()

	// Flood the location with ore, advance to the next reprice
	// tick, then ask the endpoint for the new price.
	universe.ApplyInput(state, &universe.Input{
		Kind: "supply", AtLocation: "station_one", GoodID: "ore", Amount: 100,
	})
	universe.Advance(state, 60, nil)
	// Hit the endpoint again — the state behind the handler is the
	// same one we just advanced, so the response should reflect
	// the reprice.
	resp, err := http.Get(srv.URL + "/api/economy/prices")
	if err != nil {
		t.Fatalf("request failed: %v", err)
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	var payload struct {
		Prices map[string]struct {
			Price int `json:"price"`
		} `json:"prices"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		t.Fatalf("body is not valid JSON: %v", err)
	}
	// Sanity: the endpoint reflects live state. Reprice logic
	// itself is covered exhaustively in the universe tests; here
	// we just assert the value made it back to the wire.
	_ = e
	if payload.Prices["ore"].Price >= 10 {
		t.Errorf("ore price = %d, want < 10 after flooding (reprice not visible on the wire?)",
			payload.Prices["ore"].Price)
	}
}

// TestNew_NilContent: a nil content result is not a panic.
func TestNew_NilContent(t *testing.T) {
	defer func() {
		if r := recover(); r != nil {
			t.Fatalf("New with nil content panicked: %v", r)
		}
	}()
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	state := universe.NewState(1)
	New(log, state, nil)
	if len(state.Market) != 0 || len(state.Locations) != 0 {
		t.Errorf("nil content should leave Market and Locations empty")
	}
}

// TestNew_LiveContent: the live-content smoke test. Load the
// three REACHLOCK goods and Sorrow Station, then assert the engine
// boots without crashing. This is what the brief calls the "live
// content integration" check.
func TestNew_LiveContent(t *testing.T) {
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	state := universe.NewState(1)
	content, err := loader.Load(findRepoMods(t), []string{"factions", "locations", "goods"})
	if err != nil {
		t.Fatalf("loader.Load: %v", err)
	}
	New(log, state, content)
	t.Logf("seeded market with %d goods, %d locations",
		len(state.Market), len(state.Locations))
	// We don't assert specific goods — the content set will grow.
	// But the engine should not have panicked and every loaded
	// good should have a base price >= 1.
	for id, g := range state.Market {
		if g.BasePrice < 1 {
			t.Errorf("good %s has base_price %d, want >= 1", id, g.BasePrice)
		}
	}
}

// findRepoMods walks up from this package's directory to find the
// repo's godot/mods directory. See internal/loader/loader_test.go
// for the detailed rationale.
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
