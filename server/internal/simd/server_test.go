package simd

import (
	"bufio"
	"encoding/json"
	"fmt"
	"net"
	"testing"

	"github.com/chezgoulet/reachlock/server/internal/universe"
)

// testState mirrors the journal_test universe: two locations pulling the
// same goods opposite ways, one strained faction pair.
func testState(seed uint64) *universe.State {
	s := universe.NewState(seed)
	s.AddFaction("faction_a", "Faction A", []string{"expand"}, map[string]string{"faction_b": "tense"})
	s.AddFaction("faction_b", "Faction B", []string{"survive"}, map[string]string{"faction_a": "tense"})
	s.AddGood("good_ore", 12)
	s.AddGood("good_food", 8)
	s.AddLocation("station_a", []string{"good_food"}, []string{"good_ore"})
	s.AddLocation("outpost_b", []string{"good_ore"}, []string{"good_food"})
	return s
}

type client struct {
	conn   net.Conn
	reader *bufio.Reader
	seq    uint64
}

func dial(t *testing.T, addr net.Addr) *client {
	t.Helper()
	conn, err := net.Dial("tcp", addr.String())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { conn.Close() })
	return &client{conn: conn, reader: bufio.NewReader(conn)}
}

// call sends one request and decodes the reply body into out (unless nil).
// Returns the reply envelope.
func (c *client) call(t *testing.T, ty string, body any, out any) *Envelope {
	t.Helper()
	raw, err := json.Marshal(body)
	if err != nil {
		t.Fatal(err)
	}
	line, _ := json.Marshal(map[string]any{"v": 0, "seq": c.seq, "type": ty, "body": json.RawMessage(raw)})
	sent := c.seq
	c.seq++
	if _, err := fmt.Fprintf(c.conn, "%s\n", line); err != nil {
		t.Fatal(err)
	}
	replyLine, err := c.reader.ReadString('\n')
	if err != nil {
		t.Fatalf("reading reply to %s: %v", ty, err)
	}
	var env Envelope
	if err := json.Unmarshal([]byte(replyLine), &env); err != nil {
		t.Fatalf("reply to %s is not an envelope: %v", ty, err)
	}
	if env.Re == nil || *env.Re != sent {
		t.Fatalf("reply to %s: re=%v, want %d", ty, env.Re, sent)
	}
	if out != nil && env.Type != "error" {
		if err := json.Unmarshal(env.Body, out); err != nil {
			t.Fatalf("reply body to %s: %v", ty, err)
		}
	}
	return &env
}

func startServer(t *testing.T) (*Server, *universe.State) {
	t.Helper()
	state := testState(42)
	server, err := Bind(0, state, nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { server.Close() })
	go server.Serve()
	return server, state
}

// The full happy path over real TCP: hello → advance → prices at two
// locations differ → trade shifts the local price → journal has entries →
// snapshot/load round-trip → shutdown closes.
func TestLifecycleOverTCP(t *testing.T) {
	server, _ := startServer(t)
	c := dial(t, server.Addr())

	var welcome WelcomeBody
	env := c.call(t, "hello", HelloBody{ProtocolVersion: 0, Profile: Profile, Client: "test"}, &welcome)
	if env.Type != "welcome" || welcome.Server != ServerIdentity || welcome.Seed != 42 {
		t.Fatalf("welcome: %s %+v", env.Type, welcome)
	}

	var advanced AdvancedBody
	env = c.call(t, "advance", AdvanceBody{Ticks: 600}, &advanced)
	if env.Type != "advanced" || advanced.Tick != 600 {
		t.Fatalf("advanced: %s tick=%d", env.Type, advanced.Tick)
	}
	if len(advanced.Snapshot) == 0 {
		t.Fatal("advanced carried no snapshot")
	}

	var atSorrow, atVerne PricesBody
	c.call(t, "query_prices", QueryPricesBody{LocationID: "station_a"}, &atSorrow)
	c.call(t, "query_prices", QueryPricesBody{LocationID: "outpost_b"}, &atVerne)
	priceOf := func(p PricesBody, good string) int {
		for _, e := range p.Prices {
			if e.GoodID == good {
				return e.Price
			}
		}
		t.Fatalf("no %s in prices", good)
		return 0
	}
	if priceOf(atVerne, "good_ore") >= priceOf(atSorrow, "good_ore") {
		t.Errorf("ore should be cheaper at the producer: producer=%d consumer=%d",
			priceOf(atVerne, "good_ore"), priceOf(atSorrow, "good_ore"))
	}

	before := priceOf(atSorrow, "good_ore")
	env = c.call(t, "apply_input", ApplyInputBody{Input: universe.Input{
		Kind: "trade", AtLocation: "station_a", GoodID: "good_ore", Amount: 200,
	}}, nil)
	if env.Type != "ack" {
		t.Fatalf("apply_input: %s", env.Type)
	}
	var after PricesBody
	c.call(t, "query_prices", QueryPricesBody{LocationID: "station_a"}, &after)
	if priceOf(after, "good_ore") >= before {
		t.Errorf("dumping 200 ore should depress the local price: %d -> %d",
			before, priceOf(after, "good_ore"))
	}

	var journal JournalBody
	c.call(t, "query_journal", QueryJournalBody{SinceTick: 0}, &journal)
	if len(journal.Entries) == 0 {
		t.Error("journal is empty after 600 ticks + a trade")
	}

	// load: rewind to the tick-600 snapshot; tick must come back.
	env = c.call(t, "load", LoadBody{Snapshot: advanced.Snapshot}, nil)
	if env.Type != "ack" {
		t.Fatalf("load: %s (%s)", env.Type, env.Body)
	}
	var w2 WelcomeBody
	c.call(t, "hello", HelloBody{ProtocolVersion: 0, Profile: Profile, Client: "test"}, &w2)
	if w2.Tick != 600 {
		t.Errorf("after load, tick = %d, want 600", w2.Tick)
	}

	env = c.call(t, "shutdown", struct{}{}, nil)
	if env.Type != "ack" {
		t.Fatalf("shutdown: %s", env.Type)
	}
	if _, err := c.reader.ReadString('\n'); err == nil {
		t.Error("connection should close after shutdown ack")
	}
}

// State survives across connections: a second connection sees the tick
// the first one advanced to.
func TestStateSurvivesReconnect(t *testing.T) {
	server, _ := startServer(t)
	c1 := dial(t, server.Addr())
	c1.call(t, "hello", HelloBody{ProtocolVersion: 0, Profile: Profile, Client: "t"}, nil)
	c1.call(t, "advance", AdvanceBody{Ticks: 120}, nil)
	c1.call(t, "shutdown", struct{}{}, nil)

	c2 := dial(t, server.Addr())
	var welcome WelcomeBody
	c2.call(t, "hello", HelloBody{ProtocolVersion: 0, Profile: Profile, Client: "t"}, &welcome)
	if welcome.Tick != 120 {
		t.Errorf("reconnect sees tick %d, want 120", welcome.Tick)
	}
}

// Error paths: bad_frame keeps the connection open; unknown type;
// unknown location; version mismatch closes.
func TestErrorPaths(t *testing.T) {
	server, _ := startServer(t)
	c := dial(t, server.Addr())
	c.call(t, "hello", HelloBody{ProtocolVersion: 0, Profile: Profile, Client: "t"}, nil)

	// bad frame — connection stays open.
	fmt.Fprintf(c.conn, "{not json\n")
	line, err := c.reader.ReadString('\n')
	if err != nil {
		t.Fatal(err)
	}
	var env Envelope
	json.Unmarshal([]byte(line), &env)
	var errBody ErrorBody
	json.Unmarshal(env.Body, &errBody)
	if errBody.Code != CodeBadFrame {
		t.Errorf("bad frame -> %s, want %s", errBody.Code, CodeBadFrame)
	}

	// unknown type — still open.
	fmt.Fprintf(c.conn, `{"v":0,"seq":50,"type":"frobnicate","body":{}}`+"\n")
	line, _ = c.reader.ReadString('\n')
	json.Unmarshal([]byte(line), &env)
	json.Unmarshal(env.Body, &errBody)
	if errBody.Code != CodeUnknownType {
		t.Errorf("unknown type -> %s, want %s", errBody.Code, CodeUnknownType)
	}

	// unknown location.
	reply := c.call(t, "query_prices", QueryPricesBody{LocationID: "nowhere"}, nil)
	json.Unmarshal(reply.Body, &errBody)
	if reply.Type != "error" || errBody.Code != CodeUnknownLocation {
		t.Errorf("unknown location -> %s/%s", reply.Type, errBody.Code)
	}

	// invalid advance.
	reply = c.call(t, "advance", AdvanceBody{Ticks: -1}, nil)
	json.Unmarshal(reply.Body, &errBody)
	if errBody.Code != CodeInvalidArgs {
		t.Errorf("negative ticks -> %s, want %s", errBody.Code, CodeInvalidArgs)
	}

	// bad snapshot.
	reply = c.call(t, "load", LoadBody{Snapshot: json.RawMessage(`"nope"`)}, nil)
	json.Unmarshal(reply.Body, &errBody)
	if errBody.Code != CodeBadSnapshot {
		t.Errorf("bad snapshot -> %s, want %s", errBody.Code, CodeBadSnapshot)
	}

	c.call(t, "shutdown", struct{}{}, nil)

	// version mismatch on a fresh connection: error then close.
	c2 := dial(t, server.Addr())
	reply = c2.call(t, "hello", HelloBody{ProtocolVersion: 99, Profile: Profile, Client: "t"}, nil)
	json.Unmarshal(reply.Body, &errBody)
	if reply.Type != "error" || errBody.Code != CodeVersionUnsupported {
		t.Errorf("version mismatch -> %s/%s", reply.Type, errBody.Code)
	}
	if _, err := c2.reader.ReadString('\n'); err == nil {
		t.Error("connection should close after version_unsupported")
	}
}
