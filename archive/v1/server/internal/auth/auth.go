package auth

import (
	"log/slog"
)

// Auth handles player accounts, authentication, and session management
// for the MMO server.
type Auth struct {
	log *slog.Logger
}

func New(log *slog.Logger) *Auth {
	return &Auth{log: log}
}
