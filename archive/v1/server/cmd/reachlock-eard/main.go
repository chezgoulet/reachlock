// reachlock-eard — the speech daemon: push-to-talk audio in, transcripts
// out, served over the Ear Protocol
// (godot/framework/protocol/EAR-PROTOCOL.md). NDJSON over TCP loopback,
// same sidecar pattern as pan serve and reachlock-simd.
//
// Usage:
//
//	reachlock-eard [--port N] [--engine exec|echo] [--whisper-bin PATH]
//	               [--model PATH] [--echo-text TEXT]
//
// --port: TCP port on 127.0.0.1 (0 = OS-assigned; the bound port is
// printed to stderr as "eard: bound 127.0.0.1:<port>"). Defaults to
// $REACHLOCK_EAR_PORT, then 40709.
// --engine: "exec" wraps a whisper.cpp CLI per utterance (the real thing);
// "echo" answers every utterance with --echo-text (wire tests, dev without
// a model). Defaults to $REACHLOCK_EAR_ENGINE, then exec.
// --whisper-bin: the whisper.cpp CLI. Defaults to $REACHLOCK_WHISPER_BIN,
// then "whisper-cli" on PATH.
// --model: the ggml model file. Size to the box — pan and the game share
// the memory budget, so base/small. Defaults to $REACHLOCK_WHISPER_MODEL,
// then ~/.local/share/reachlock/models/ggml-base.en.bin.
//
// A missing binary or model kills the daemon at startup, loudly, here —
// so the game side stays silent: no daemon on the port simply means the
// voice affordance does not exist.
package main

import (
	"flag"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strconv"

	"github.com/chezgoulet/reachlock/server/internal/eard"
)

func main() {
	port := flag.Int("port", envInt("REACHLOCK_EAR_PORT", 40709), "TCP port on 127.0.0.1 (0 = OS-assigned)")
	engineName := flag.String("engine", envStr("REACHLOCK_EAR_ENGINE", "exec"), "STT engine: exec (whisper.cpp CLI) or echo")
	whisperBin := flag.String("whisper-bin", envStr("REACHLOCK_WHISPER_BIN", "whisper-cli"), "whisper.cpp CLI binary")
	model := flag.String("model", envStr("REACHLOCK_WHISPER_MODEL", defaultModelPath()), "ggml whisper model file")
	echoText := flag.String("echo-text", "the quick brown fox", "transcript the echo engine answers with")
	flag.Parse()

	log := slog.New(slog.NewTextHandler(os.Stderr, nil))

	var engine eard.Engine
	switch *engineName {
	case "echo":
		engine = &eard.EchoEngine{Text: *echoText}
	case "exec":
		exe := &eard.ExecEngine{Bin: *whisperBin, ModelPath: *model}
		if err := exe.Check(); err != nil {
			log.Error("eard: cannot start; voice will not exist", "err", err)
			os.Exit(1)
		}
		engine = exe
	default:
		log.Error("eard: unknown engine", "engine", *engineName)
		os.Exit(1)
	}

	server, err := eard.Bind(*port, engine, log)
	if err != nil {
		log.Error("eard: bind failed", "err", err)
		os.Exit(1)
	}
	// The exact spelling a spawning host parses; keep in sync with
	// EAR-PROTOCOL.md and the simd equivalent.
	fmt.Fprintf(os.Stderr, "eard: bound %s\n", server.Addr())
	log.Info("eard: serving", "engine", engine.Name(), "model", engine.Model())
	if err := server.Serve(); err != nil {
		log.Error("eard: serve", "err", err)
		os.Exit(1)
	}
}

func envInt(key string, fallback int) int {
	if raw := os.Getenv(key); raw != "" {
		if n, err := strconv.Atoi(raw); err == nil {
			return n
		}
	}
	return fallback
}

func envStr(key, fallback string) string {
	if raw := os.Getenv(key); raw != "" {
		return raw
	}
	return fallback
}

func defaultModelPath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return "ggml-base.en.bin" // arch-allow: whisper model filename, not a mod id
	}
	return filepath.Join(home, ".local", "share", "reachlock", "models", "ggml-base.en.bin") // arch-allow: whisper model filename, not a mod id
}
