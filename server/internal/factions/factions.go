package factions

import (
	"log/slog"
	"net/http"
)

// Factions manages faction simulation: goals, relationships, territory,
// and the tick-driven faction AI.
type Factions struct {
	log *slog.Logger
}

func New(log *slog.Logger) *Factions {
	return &Factions{log: log}
}

func (f *Factions) RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/factions", f.handleList)
	mux.HandleFunc("GET /api/factions/{id}", f.handleGet)
}

func (f *Factions) handleList(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.Write([]byte(`{"factions":["compact","isc","corp_charter","reach","earth_remnant"]}`))
}

func (f *Factions) handleGet(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	w.Header().Set("Content-Type", "application/json")
	w.Write([]byte(`{"id":"` + id + `"}`))
}
