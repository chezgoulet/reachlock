// Package factions is the HTTP face of the faction simulator. It does
// not own faction state — the universe.State does. This package wires
// loaded content into the universe on boot and exposes the live
// faction table to callers (the in-game UI, the dialogue system,
// debugging endpoints, future MMO clients).
//
// The previous stub of this package hardcoded the five REACHLOCK
// faction ids. That is the boundary debt called out in
// docs/ARCHITECTURE.md — engine code reaching into content. This
// implementation reads the ids from the loader (which reads them from
// godot/mods/<mod>/factions/*.json), so the engine contains zero
// faction ids and adding a new faction does not require a code change
// here.
package factions

import (
	"encoding/json"
	"log/slog"
	"net/http"

	"github.com/chezgoulet/reachlock/server/internal/loader"
	"github.com/chezgoulet/reachlock/server/internal/universe"
)

// Factions is the per-request handler set. It holds a reference to
// the shared universe state and a logger; both are injected so tests
// can build a fakes without booting a real sim.
type Factions struct {
	log   *slog.Logger
	state *universe.State
}

// New builds a Factions handler set, loading faction content from the
// given mod loader result and seeding the universe state's faction
// table. The universe state itself is owned by the caller; this
// constructor only mutates the faction sub-table.
//
// Empty content is not an error — the server still boots and the
// faction list endpoint returns []. The caller's logger receives a
// "no factions loaded" warning so a missing mods root is visible
// without being fatal.
func New(log *slog.Logger, state *universe.State, content *loader.Result) *Factions {
	if log == nil {
		log = slog.Default()
	}
	if content == nil {
		log.Warn("factions: no content supplied — engine boots with zero factions")
		return &Factions{log: log, state: state}
	}
	if len(content.Factions) == 0 {
		log.Warn("factions: no factions loaded from content — engine boots with zero factions")
	}
	for _, f := range content.Factions {
		state.AddFaction(f.ID, f.Name, f.Goals, f.Relationships)
	}
	for _, w := range content.Warnings {
		log.Warn("loader: " + w)
	}
	return &Factions{log: log, state: state}
}

// RegisterRoutes wires the faction endpoints onto the given mux. The
// routes mirror the old stub's shape so existing clients keep
// working: GET /api/factions lists, GET /api/factions/{id} gets one.
func (f *Factions) RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/factions", f.handleList)
	mux.HandleFunc("GET /api/factions/{id}", f.handleGet)
}

// handleList returns every faction id currently in the live state,
// in the order the universe stores them. The shape is intentionally
// minimal — the in-game UI can ask for individual details.
func (f *Factions) handleList(w http.ResponseWriter, r *http.Request) {
	ids := make([]string, 0, len(f.state.Factions))
	for id := range f.state.Factions {
		ids = append(ids, id)
	}
	// Sort for a stable wire shape — Go map iteration order is
	// randomized, and a randomized HTTP response would be a small
	// but real bug for any caller diffing the result.
	sortStrings(ids)
	writeJSON(w, map[string]any{"factions": ids})
}

// handleGet returns the live state of one faction: identity, goals,
// trust, and the live relationship map. A 404 is returned for any
// id the loader did not register — there is no implicit "search
// across mods" or "alias" logic; the id is the id.
func (f *Factions) handleGet(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	fac, ok := f.state.Factions[id]
	if !ok {
		http.Error(w, "{\"error\":\"unknown faction\"}", http.StatusNotFound)
		return
	}
	writeJSON(w, map[string]any{
		"id":            fac.ID,
		"name":          fac.Name,
		"goals":         fac.Goals,
		"trust":         fac.Trust,
		"relationships": fac.Relationships,
	})
}

// writeJSON marshals v as JSON and writes it to w. Errors are
// logged but otherwise swallowed — at this point in the request
// pipeline, the only recourse is to drop the connection, and the
// caller will see a truncated response.
func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	raw, err := json.Marshal(v)
	if err != nil {
		http.Error(w, "{\"error\":\"encode failed\"}", http.StatusInternalServerError)
		return
	}
	w.Write(raw)
}

// sortStrings is an inlined sort.Strings to avoid importing sort in
// the public surface of this package (the universe package owns
// ordered data; the factions handler just needs ids sorted).
func sortStrings(s []string) {
	// Insertion sort — the slice is small (one entry per faction)
	// and we don't need a general sort here.
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j-1] > s[j]; j-- {
			s[j-1], s[j] = s[j], s[j-1]
		}
	}
}
