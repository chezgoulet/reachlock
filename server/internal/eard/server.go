package eard

import (
	"bufio"
	"fmt"
	"log/slog"
	"net"
	"strings"
)

// maxLineBytes caps one NDJSON line. Audio chunks are ~8 KB of base64 per
// 250 ms; a megabyte is a hostile host, not a long sentence.
const maxLineBytes = 1 << 20

// Server owns the STT engine and the loopback listener. One connection is
// served at a time, line-at-a-time, single-threaded — same discipline as
// the sim daemon: no locking, because no concurrency.
type Server struct {
	listener net.Listener
	engine   Engine
	log      *slog.Logger
}

// Bind binds 127.0.0.1:<port> (port 0 = OS-assigned) around an engine.
func Bind(port int, engine Engine, log *slog.Logger) (*Server, error) {
	if log == nil {
		log = slog.Default()
	}
	listener, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", port))
	if err != nil {
		return nil, err
	}
	return &Server{listener: listener, engine: engine, log: log}, nil
}

// Addr returns the bound address (useful when port 0 was requested).
func (s *Server) Addr() net.Addr { return s.listener.Addr() }

// Close stops the listener.
func (s *Server) Close() error { return s.listener.Close() }

// Serve accepts and drives connections until the listener closes. A
// connection-level error never kills the daemon.
func (s *Server) Serve() error {
	for {
		conn, err := s.listener.Accept()
		if err != nil {
			if strings.Contains(err.Error(), "use of closed network connection") {
				return nil
			}
			return err
		}
		if err := s.drive(conn); err != nil {
			s.log.Warn("eard: connection error; awaiting next host", "err", err)
		}
		conn.Close()
	}
}

// drive runs one connection to completion. Unlike the sim daemon, some
// messages (audio_begin, audio_chunk) have no reply on success — Handle
// returns nil and nothing is written.
func (s *Server) drive(conn net.Conn) error {
	session := NewSession(s.engine)
	reader := bufio.NewReaderSize(conn, 64*1024)
	writer := bufio.NewWriter(conn)
	for {
		line, err := readLine(reader)
		if err != nil {
			return nil // EOF / reset: the host went away; nothing to keep
		}
		if strings.TrimSpace(line) == "" {
			continue
		}
		env, reject := ParseLine(line)
		if reject != nil {
			out, err := session.RejectLine(reject)
			if err != nil {
				return err
			}
			if err := writeLine(writer, out); err != nil {
				return err
			}
			continue
		}
		reply, err := session.Handle(env)
		if err != nil {
			return err
		}
		if reply != nil {
			if err := writeLine(writer, reply); err != nil {
				return err
			}
		}
		if CloseAfter(env, reply) {
			return nil
		}
	}
}

func readLine(r *bufio.Reader) (string, error) {
	var b strings.Builder
	for {
		chunk, isPrefix, err := r.ReadLine()
		if err != nil {
			return "", err
		}
		b.Write(chunk)
		if b.Len() > maxLineBytes {
			return "", fmt.Errorf("line exceeds %d bytes", maxLineBytes)
		}
		if !isPrefix {
			return b.String(), nil
		}
	}
}

func writeLine(w *bufio.Writer, env *Envelope) error {
	raw, err := env.Marshal()
	if err != nil {
		return err
	}
	if _, err := w.Write(raw); err != nil {
		return err
	}
	if err := w.WriteByte('\n'); err != nil {
		return err
	}
	return w.Flush()
}
