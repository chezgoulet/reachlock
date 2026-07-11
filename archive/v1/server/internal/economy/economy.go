// Package economy is the HTTP face of the economy engine. It does
// not own prices or supply/demand — the universe.State does. This
// package seeds the universe's market and location economy tables
// from loaded content, and exposes the live price list to callers.
//
// The price curve itself lives in universe.maybeReprice: a per-tick
// 60-tick cadence, integer math, supply lowers the price, demand
// raises it, clamped to [base/2, base*2]. The tests for the curve
// are in internal/universe; this package is the wire layer only.
package economy

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"sort"

	"github.com/chezgoulet/reachlock/server/internal/loader"
	"github.com/chezgoulet/reachlock/server/internal/universe"
)

// Economy is the per-request handler set.
type Economy struct {
	log   *slog.Logger
	state *universe.State
}

// New builds the economy handler set, seeding the universe state's
// market and location tables from the loader's content. Empty
// content is non-fatal (a warning, then the server boots with no
// market and no prices to report).
func New(log *slog.Logger, state *universe.State, content *loader.Result) *Economy {
	if log == nil {
		log = slog.Default()
	}
	if content == nil {
		log.Warn("economy: no content supplied — engine boots with zero goods and zero locations")
		return &Economy{log: log, state: state}
	}
	if len(content.Goods) == 0 {
		log.Warn("economy: no goods loaded from content — engine boots with zero prices")
	}
	if len(content.Locations) == 0 {
		log.Warn("economy: no locations loaded from content — supply/demand has no anchors")
	}
	for _, g := range content.Goods {
		state.AddGood(g.ID, g.BasePrice)
	}
	for id, l := range content.Locations {
		state.AddLocation(id, l.Produces, l.Consumes)
	}
	for _, w := range content.Warnings {
		log.Warn("loader: " + w)
	}
	return &Economy{log: log, state: state}
}

// RegisterRoutes wires the price endpoint onto the given mux.
func (e *Economy) RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/economy/prices", e.handlePrices)
}

// handlePrices returns the live price of every good in the universe,
// in stable id order, with each price carrying its base for
// debugging. Empty market is a valid response (a fresh universe with
// no content yet).
func (e *Economy) handlePrices(w http.ResponseWriter, r *http.Request) {
	ids := make([]string, 0, len(e.state.Market))
	for id := range e.state.Market {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	out := make(map[string]priceEntry, len(ids))
	for _, id := range ids {
		g := e.state.Market[id]
		out[id] = priceEntry{
			BasePrice: g.BasePrice,
			Price:     g.Price,
			NetDelta:  g.NetDelta,
		}
	}
	w.Header().Set("Content-Type", "application/json")
	raw, _ := json.Marshal(map[string]any{"prices": out})
	w.Write(raw)
}

type priceEntry struct {
	BasePrice int `json:"base_price"`
	Price     int `json:"price"`
	NetDelta  int `json:"net_delta"`
}
