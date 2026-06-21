package main

import (
	"context"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/chezgoulet/reachlock/server/internal/auth"
	"github.com/chezgoulet/reachlock/server/internal/economy"
	"github.com/chezgoulet/reachlock/server/internal/factions"
	"github.com/chezgoulet/reachlock/server/internal/universe"
)

func main() {
	log := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

	addr := os.Getenv("REACHLOCK_ADDR")
	if addr == "" {
		addr = ":8080"
	}

	uni := universe.New(log)
	eco := economy.New(log)
	fac := factions.New(log)
	_ = auth.New(log)

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
