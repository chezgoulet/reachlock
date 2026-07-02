// REACHLOCK MMO server: the simulation engine. The networking layer
// (auth, matchmaking, account state) is Phase 5 work and is not in
// this binary; what lives here is the sim — faction goals, economy,
// the universe tick — and the small HTTP surface that lets the
// in-game UI and the MMO client drive it.
//
// Ring-0 invariant: this binary contains zero content ids. All
// content is read by internal/loader from godot/mods on boot, and
// the architecture guard (scripts/check_architecture.py) verifies
// that no hardcoded content id leaks into this package.
package main

import (
	"context"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"

	"github.com/chezgoulet/reachlock/server/internal/economy"
	"github.com/chezgoulet/reachlock/server/internal/factions"
	"github.com/chezgoulet/reachlock/server/internal/loader"
	"github.com/chezgoulet/reachlock/server/internal/universe"
)

func main() {
	log := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

	addr := os.Getenv("REACHLOCK_ADDR")
	if addr == "" {
		addr = ":8080"
	}

	// Mods root: the engine-side default is "<repo>/godot/mods". The
	// env var REACHLOCK_MODS lets deployments point at a different
	// content directory (e.g. a packed overlay for an MMO shard).
	modsRoot := os.Getenv("REACHLOCK_MODS")
	if modsRoot == "" {
		modsRoot = defaultModsRoot()
	}

	// Load content. The sim tolerates a missing mods root (warning,
	// not crash), so the boot order is: read content, build state,
	// seed state from content, start HTTP.
	content, err := loader.Load(modsRoot, []string{"factions", "locations", "goods"})
	if err != nil {
		// A hard error from the loader (e.g. permission denied on
		// the mods root, not just a missing dir) is fatal: it means
		// the operator pointed us at something broken.
		log.Error("loader: fatal", "err", err, "mods_root", modsRoot)
		os.Exit(1)
	}
	log.Info("loader: ok",
		"mods_root", modsRoot,
		"mods", len(content.Mods),
		"factions", len(content.Factions),
		"locations", len(content.Locations),
		"goods", len(content.Goods),
		"warnings", len(content.Warnings),
	)

	// One universe state, one HTTP mux, every handler set sees the
	// same data. The state is a value type behind a pointer; all
	// handlers mutate it through the universe.Advance and friends.
	state := universe.NewState(0xC0FFEE)
	uni := universe.NewManager(log, state)
	eco := economy.New(log, state, content)
	fac := factions.New(log, state, content)

	mux := http.NewServeMux()
	uni.RegisterRoutes(mux)
	eco.RegisterRoutes(mux)
	fac.RegisterRoutes(mux)

	srv := &http.Server{
		Addr:         addr,
		Handler:      mux,
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 30 * time.Second,
	}

	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()

	go func() {
		log.Info("reachlock-server starting", "addr", addr)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Error("server error", "err", err)
			os.Exit(1)
		}
	}()

	<-ctx.Done()
	log.Info("shutting down")

	shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := srv.Shutdown(shutdownCtx); err != nil {
		log.Error("shutdown error", "err", err)
	}
}

// defaultModsRoot resolves the engine's default mods root from the
// process working directory. The server is run from the repo root
// (the Makefile's server-run target is at the repo root), so the
// default path is "godot/mods" relative to the cwd. We resolve it
// to an absolute path so the loader's error messages are not
// relative-to-cwd-dependent.
func defaultModsRoot() string {
	cwd, err := os.Getwd()
	if err != nil {
		return "godot/mods"
	}
	return filepath.Join(cwd, "godot", "mods")
}
