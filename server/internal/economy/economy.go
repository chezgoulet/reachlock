package economy

import (
	"log/slog"
	"net/http"
)

// Economy manages the galaxy-spanning dynamic market: supply/demand curves,
// trade route pricing, and faction tariffs.
type Economy struct {
	log *slog.Logger
}

func New(log *slog.Logger) *Economy {
	return &Economy{log: log}
}

func (e *Economy) RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/economy/prices", e.handlePrices)
}

func (e *Economy) handlePrices(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.Write([]byte(`{"prices":{}}`))
}
