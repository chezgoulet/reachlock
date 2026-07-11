package eard

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// Engine turns finished PCM16 audio into text. Implementations must be
// safe for sequential reuse (the daemon serves one host, one utterance at
// a time — no locking needed by contract).
type Engine interface {
	// Name is what welcome reports as `engine` ("whisper.cpp", "echo").
	Name() string
	// Model is what welcome reports as `model`.
	Model() string
	// Decode transcribes PCM16 LE mono samples at SampleRate.
	Decode(pcm []byte) (text string, confidence float64, err error)
}

// ── echo ────────────────────────────────────────────────────────────────────

// EchoEngine answers every utterance with a fixed line. It exists for the
// wire tests and for driving the full stack without a whisper model —
// integration paths get exercised, nothing pretends to listen.
type EchoEngine struct {
	Text string
}

func (e *EchoEngine) Name() string  { return "echo" }
func (e *EchoEngine) Model() string { return "none" }

func (e *EchoEngine) Decode(pcm []byte) (string, float64, error) {
	if len(pcm) == 0 {
		return "", 0.0, nil
	}
	return e.Text, 1.0, nil
}

// ── whisper.cpp via its CLI ─────────────────────────────────────────────────

// ExecEngine shells out to a whisper.cpp CLI (`whisper-cli`) per
// utterance. Local CPU decode of a few seconds of speech with a base/small
// model fits the latency mask; the streaming upgrade is a noted gap in the
// protocol, not here.
type ExecEngine struct {
	Bin       string // whisper-cli path
	ModelPath string // ggml model file
}

func (e *ExecEngine) Name() string  { return "whisper.cpp" }
func (e *ExecEngine) Model() string { return filepath.Base(e.ModelPath) }

// Check verifies the binary and model exist — run at startup so a
// misconfigured daemon dies loudly instead of erroring per-utterance.
func (e *ExecEngine) Check() error {
	if _, err := exec.LookPath(e.Bin); err != nil {
		return fmt.Errorf("whisper binary %q not found: %w", e.Bin, err)
	}
	if _, err := os.Stat(e.ModelPath); err != nil {
		return fmt.Errorf("whisper model not found at %s: %w", e.ModelPath, err)
	}
	return nil
}

func (e *ExecEngine) Decode(pcm []byte) (string, float64, error) {
	if len(pcm) == 0 {
		return "", 0.0, nil
	}
	wav, err := os.CreateTemp("", "eard-*.wav")
	if err != nil {
		return "", 0, err
	}
	defer os.Remove(wav.Name())
	if _, err := wav.Write(wavHeader(len(pcm))); err != nil {
		wav.Close()
		return "", 0, err
	}
	if _, err := wav.Write(pcm); err != nil {
		wav.Close()
		return "", 0, err
	}
	wav.Close()
	// -nt: no timestamps, -np: no progress banners — stdout is the text.
	cmd := exec.Command(e.Bin, "-m", e.ModelPath, "-f", wav.Name(), "-nt", "-np")
	var out, errb bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &errb
	if err := cmd.Run(); err != nil {
		return "", 0, fmt.Errorf("whisper-cli: %v: %s", err, strings.TrimSpace(errb.String()))
	}
	// The CLI reports no usable token probabilities; 0.5 is "unknown", and
	// the v0 host gates nothing on confidence. Streaming engines can do
	// better without a protocol change.
	return strings.TrimSpace(out.String()), 0.5, nil
}

// wavHeader builds the 44-byte RIFF header for PCM16 LE mono @ SampleRate.
func wavHeader(dataLen int) []byte {
	header := make([]byte, 44)
	copy(header[0:4], "RIFF")
	binary.LittleEndian.PutUint32(header[4:8], uint32(36+dataLen))
	copy(header[8:12], "WAVE")
	copy(header[12:16], "fmt ")
	binary.LittleEndian.PutUint32(header[16:20], 16)                 // fmt chunk size
	binary.LittleEndian.PutUint16(header[20:22], 1)                  // PCM
	binary.LittleEndian.PutUint16(header[22:24], 1)                  // mono
	binary.LittleEndian.PutUint32(header[24:28], SampleRate)         // sample rate
	binary.LittleEndian.PutUint32(header[28:32], SampleRate*2)       // byte rate
	binary.LittleEndian.PutUint16(header[32:34], 2)                  // block align
	binary.LittleEndian.PutUint16(header[34:36], 16)                 // bits per sample
	copy(header[36:40], "data")
	binary.LittleEndian.PutUint32(header[40:44], uint32(dataLen))
	return header
}
