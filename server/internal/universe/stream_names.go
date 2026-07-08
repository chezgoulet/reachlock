package universe

// Stream names for per-stream RNG seeding in rng.go.
//
// The per-stream RNG seeds each stream by (universe_seed, stream_name, tick).
// Every consuming subsystem must use a unique stream name.  Keeping them all in
// one const block makes collisions visible at compile time (a duplicate const
// triggers a go vet warning) and documents every RNG consumer in one place.
const (
	// StreamFactionDrift is used by the faction simulation for territory drift,
	// relationship changes, and AI decision-making.
	StreamFactionDrift = "faction_drift"

	// StreamEconomyReprice is used by the economy simulation for price
	// fluctuations, supply/demand shifts, and trade route re-evaluation.
	StreamEconomyReprice = "economy_reprice"

	// StreamEventDraw is used by the universe event queue for random event
	// selection, scheduling, and outcome determination.
	StreamEventDraw = "event_draw"
)
