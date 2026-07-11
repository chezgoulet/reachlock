// HTTP face of the universe tick. The engine itself is the State
// type and the Advance function; this file adds the routes that
// let a caller (the in-game UI, the MMO client, a debugger) inspect
// the live state, drive ticks, and load/save snapshots.
//
// Endpoints:
//
//	GET  /api/universe/state     — current tick, seed, counts
//	GET  /api/universe/systems   — locations registered in state
//	POST /api/universe/advance   — apply inputs and tick N times
//	GET  /api/universe/snapshot  — JSON snapshot for save
//	POST /api/universe/load      — replace state from a snapshot
package universe

import (
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
)

// Manager is the HTTP wrapper around a State. It is intentionally
// thin: no locking (the sim itself is single-threaded by contract —
// see docs/UNIVERSE-TICK.md; concurrency is a driver concern, e.g.
// the MMO service that owns the http.Server).
type Manager struct {
	log   *slog.Logger
	state *State
}

// NewManager builds a Manager around an existing State. The state
// is shared by reference: the factions, economy, and universe
// handlers all see the same data, and any of them can drive a tick.
func NewManager(log *slog.Logger, state *State) *Manager {
	if log == nil {
		log = slog.Default()
	}
	return &Manager{log: log, state: state}
}

// State returns the underlying state. Useful for the cmd binary
// that wires all the handlers together, and for tests.
func (m *Manager) State() *State { return m.state }

// RegisterRoutes wires the universe endpoints onto the mux.
func (m *Manager) RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/universe/state", m.handleState)
	mux.HandleFunc("GET /api/universe/systems", m.handleSystems)
	mux.HandleFunc("POST /api/universe/advance", m.handleAdvance)
	mux.HandleFunc("GET /api/universe/snapshot", m.handleSnapshot)
	mux.HandleFunc("POST /api/universe/load", m.handleLoad)
}

// handleState returns the live state summary: current tick, seed,
// and a count of factions/goods/locations/pending events. The
// summary is small and stable, suitable for an MMO client's
// status panel.
func (m *Manager) handleState(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	resp := map[string]any{
		"status":    "ok",
		"tick":      m.state.Tick,
		"seed":      m.state.Seed,
		"factions":  len(m.state.Factions),
		"goods":     len(m.state.Market),
		"locations": len(m.state.Locations),
		"events":    len(m.state.Events),
	}
	raw, _ := json.Marshal(resp)
	w.Write(raw)
}

// handleSystems returns the registered locations as a list. The old
// stub returned {"systems": []} — keep that shape so old clients
// don't break. "Systems" is a future expansion; today it is the
// location table (every dockable/landable place the engine knows
// about, which is the same set the location schema calls a "system"
// of places).
func (m *Manager) handleSystems(w http.ResponseWriter, r *http.Request) {
	type entry struct {
		ID string `json:"id"`
	}
	entries := make([]entry, 0, len(m.state.Locations))
	for id := range m.state.Locations {
		entries = append(entries, entry{ID: id})
	}
	w.Header().Set("Content-Type", "application/json")
	raw, _ := json.Marshal(map[string]any{"systems": entries})
	w.Write(raw)
}

// advanceRequest is the body of POST /api/universe/advance.
type advanceRequest struct {
	Ticks  int64   `json:"ticks"`
	Inputs []Input `json:"inputs"`
}

// handleAdvance applies the body's input list and ticks forward
// by N. The inputs are an ordered list — the determinism contract
// requires ordering to be preserved. After advancing, the response
// is the same as GET /api/universe/state so a caller can chain
// tick + status in one round trip.
func (m *Manager) handleAdvance(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, `{"error":"read body"}`, http.StatusBadRequest)
		return
	}
	var req advanceRequest
	if err := json.Unmarshal(body, &req); err != nil {
		http.Error(w, `{"error":"bad json"}`, http.StatusBadRequest)
		return
	}
	if req.Ticks < 0 {
		http.Error(w, `{"error":"negative ticks"}`, http.StatusBadRequest)
		return
	}
	Advance(m.state, req.Ticks, req.Inputs)
	w.Header().Set("Content-Type", "application/json")
	raw, _ := json.Marshal(map[string]any{
		"status": "ok",
		"tick":   m.state.Tick,
	})
	w.Write(raw)
}

// handleSnapshot returns a JSON snapshot of the state for save
// purposes. The shape is the same as a save.universe block in the
// save schema (C4), so an SP embed and the MMO server can share
// the same wire format.
func (m *Manager) handleSnapshot(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	raw, err := MarshalJSON(m.state)
	if err != nil {
		http.Error(w, `{"error":"encode failed"}`, http.StatusInternalServerError)
		return
	}
	w.Write(raw)
}

// handleLoad replaces the live state with the JSON snapshot in the
// request body. The state is not reset beforehand: callers that
// want a clean load should reboot the server. (A "load and zero
// any prior state" mode is one line if the MMO service ever needs
// it; right now the only caller is the snapshot test.)
func (m *Manager) handleLoad(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, `{"error":"read body"}`, http.StatusBadRequest)
		return
	}
	loaded, err := UnmarshalJSON(body)
	if err != nil {
		http.Error(w, `{"error":"bad snapshot"}`, http.StatusBadRequest)
		return
	}
	*m.state = *loaded
	w.Header().Set("Content-Type", "application/json")
	raw, _ := json.Marshal(map[string]any{
		"status": "ok",
		"tick":   m.state.Tick,
	})
	w.Write(raw)
}
