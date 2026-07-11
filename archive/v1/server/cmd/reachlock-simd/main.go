// reachlock-simd — the simulation daemon: the universe tick served over
// the Sim Protocol (godot/framework/protocol/SIM-PROTOCOL.md). NDJSON
// over TCP loopback, same sidecar pattern as pan serve.
//
// Usage:
//
//	reachlock-simd [--port N] [--seed N] [--mods DIR]
//
// --port: TCP port on 127.0.0.1 (0 = OS-assigned; the bound port is
// printed to stderr as "simd: bound 127.0.0.1:<port>"). Defaults to
// $REACHLOCK_SIM_PORT, then 40708.
// --seed: universe seed for a FRESH universe. A host that loads a save
// replaces the whole state (seed included) via the `load` message, so
// this only matters for new games. Defaults to $REACHLOCK_SIM_SEED,
// then 1.
// --mods: mods root to seed factions/goods/locations from. Defaults to
// $REACHLOCK_MODS_ROOT, then ../godot/mods, then godot/mods (whichever
// exists relative to the CWD).
package main

import (
	"flag"
	"fmt"
	"log/slog"
	"os"
	"strconv"

	"github.com/chezgoulet/reachlock/server/internal/loader"
	"github.com/chezgoulet/reachlock/server/internal/simd"
	"github.com/chezgoulet/reachlock/server/internal/universe"
)

func main() {
	port := flag.Int("port", envInt("REACHLOCK_SIM_PORT", 40708), "TCP port on 127.0.0.1 (0 = OS-assigned)")
	seed := flag.Uint64("seed", uint64(envInt("REACHLOCK_SIM_SEED", 1)), "universe seed for a fresh universe")
	mods := flag.String("mods", os.Getenv("REACHLOCK_MODS_ROOT"), "mods root (default: ../godot/mods or godot/mods)")
	flag.Parse()

	log := slog.New(slog.NewTextHandler(os.Stderr, nil))

	modsRoot := *mods
	if modsRoot == "" {
		for _, candidate := range []string{"godot/mods", "../godot/mods"} {
			if info, err := os.Stat(candidate); err == nil && info.IsDir() {
				modsRoot = candidate
				break
			}
		}
	}

	state := universe.NewState(*seed)
	content, err := loader.Load(modsRoot, []string{"factions", "locations", "goods"})
	if err != nil {
		log.Error("simd: content load failed", "err", err)
		os.Exit(1)
	}
	for _, w := range content.Warnings {
		log.Warn("loader: " + w)
	}
	for _, f := range content.Factions {
		state.AddFaction(f.ID, f.Name, f.Goals, f.Relationships)
	}
	for _, g := range content.Goods {
		state.AddGood(g.ID, g.BasePrice)
	}
	for id, l := range content.Locations {
		state.AddLocation(id, l.Produces, l.Consumes)
	}
	log.Info("simd: universe seeded",
		"factions", len(state.Factions), "goods", len(state.Market),
		"locations", len(state.Locations), "seed", *seed, "mods", modsRoot)

	server, err := simd.Bind(*port, state, log)
	if err != nil {
		log.Error("simd: bind failed", "err", err)
		os.Exit(1)
	}
	// The exact spelling a spawning host parses; keep in sync with
	// SIM-PROTOCOL.md and the pan serve equivalent.
	fmt.Fprintf(os.Stderr, "simd: bound %s\n", server.Addr())
	if err := server.Serve(); err != nil {
		log.Error("simd: server error", "err", err)
		os.Exit(1)
	}
}

func envInt(name string, fallback int) int {
	if v := os.Getenv(name); v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			return n
		}
	}
	return fallback
}
