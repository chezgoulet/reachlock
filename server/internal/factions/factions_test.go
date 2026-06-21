package factions

import (
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"testing"
)

func testServer(t *testing.T) *httptest.Server {
	t.Helper()
	f := New(slog.New(slog.NewTextHandler(io.Discard, nil)))
	mux := http.NewServeMux()
	f.RegisterRoutes(mux)
	return httptest.NewServer(mux)
}

func TestList(t *testing.T) {
	srv := testServer(t)
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

	// The five authored REACHLOCK factions must all be present.
	want := []string{"compact", "isc", "corp_charter", "reach", "earth_remnant"}
	got := make(map[string]bool, len(payload.Factions))
	for _, f := range payload.Factions {
		got[f] = true
	}
	for _, w := range want {
		if !got[w] {
			t.Errorf("faction list missing %q (got %v)", w, payload.Factions)
		}
	}
}

func TestGet(t *testing.T) {
	srv := testServer(t)
	defer srv.Close()

	resp, err := http.Get(srv.URL + "/api/factions/compact")
	if err != nil {
		t.Fatalf("request failed: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200", resp.StatusCode)
	}

	body, _ := io.ReadAll(resp.Body)
	var payload struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		t.Fatalf("body is not valid JSON: %v (%s)", err, body)
	}
	if payload.ID != "compact" {
		t.Errorf("id = %q, want compact (path value not wired through)", payload.ID)
	}
}
