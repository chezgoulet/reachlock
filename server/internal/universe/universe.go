package universe

import (
	"log/slog"
	"net/http"
)

// Universe manages the persistent galaxy simulation: faction positions,
// event queue, system states, and the tick loop.
type Universe struct {
	log *slog.Logger
}

func New(log *slog.Logger) *Universe {
	return &Universe{log: log}
}

func (u *Universe) RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/universe/state", u.handleState)
	mux.HandleFunc("GET /api/universe/systems", u.handleSystems)
}

func (u *Universe) handleState(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.Write([]byte(`{"status":"ok","tick":0}`))
}

func (u *Universe) handleSystems(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.Write([]byte(`{"systems":[]}`))
}
