package universe

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
	u := New(slog.New(slog.NewTextHandler(io.Discard, nil)))
	mux := http.NewServeMux()
	u.RegisterRoutes(mux)
	return httptest.NewServer(mux)
}

func TestState(t *testing.T) {
	srv := testServer(t)
	defer srv.Close()

	resp, err := http.Get(srv.URL + "/api/universe/state")
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
		Status string `json:"status"`
		Tick   int    `json:"tick"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		t.Fatalf("body is not valid JSON: %v (%s)", err, body)
	}
	if payload.Status != "ok" {
		t.Errorf("status field = %q, want ok", payload.Status)
	}
}

func TestSystems(t *testing.T) {
	srv := testServer(t)
	defer srv.Close()

	resp, err := http.Get(srv.URL + "/api/universe/systems")
	if err != nil {
		t.Fatalf("request failed: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200", resp.StatusCode)
	}

	body, _ := io.ReadAll(resp.Body)
	var payload struct {
		Systems []json.RawMessage `json:"systems"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		t.Fatalf("body is not valid JSON: %v (%s)", err, body)
	}
}
