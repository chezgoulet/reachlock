package simd

import (
	"bufio"
	"fmt"
	"log/slog"
	"net"
	"strings"

	"github.com/chezgoulet/reachlock/server/internal/universe"
)

// maxLineBytes caps one NDJSON line; anything larger is a hostile host.
const maxLineBytes = 1 << 20

// Server owns the live universe state and the loopback listener. One
// connection is served at a time, line-at-a-time, single-threaded — the
// sim's "no locking, concurrency is the driver's problem" contract is
// honored by never having concurrency.
type Server struct {
	listener net.Listener
	state    *universe.State
	log      *slog.Logger
}

// Bind binds 127.0.0.1:<port> (port 0 = OS-assigned) around an existing
// universe state. The bound address is logged so a spawning host can
// parse the port.
func Bind(port int, state *universe.State, log *slog.Logger) (*Server, error) {
	if log == nil {
		log = slog.Default()
	}
	listener, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", port))
	if err != nil {
		return nil, err
	}
	return &Server{listener: listener, state: state, log: log}, nil
}

// Addr returns the bound address (useful when port 0 was requested).
func (s *Server) Addr() net.Addr { return s.listener.Addr() }

// Close stops the listener.
func (s *Server) Close() error { return s.listener.Close() }

// Serve accepts and drives connections until the listener closes. The
// universe state survives across connections; a connection-level error
// never kills the daemon.
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
			s.log.Warn("simd: connection error; awaiting next host", "err", err)
		}
		conn.Close()
	}
}

// drive runs one connection to completion: read a line, parse (staged:
// bad_frame vs unknown_type), handle, write the reply, close after
// shutdown or a failed hello.
func (s *Server) drive(conn net.Conn) error {
	session := NewSession(s.state)
	reader := bufio.NewReaderSize(conn, 64*1024)
	writer := bufio.NewWriter(conn)
	for {
		line, err := readLine(reader)
		if err != nil {
			return nil // EOF / reset: the host went away, state survives
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
		if err := writeLine(writer, reply); err != nil {
			return err
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
