// Per-named-stream RNG for the universe tick.
//
// Why no math/rand or a global RNG: the determinism contract requires
// that the same state, same inputs, and same seed always produce the
// same next state — including across Go versions, across OSes, and
// across the SP embed. math/rand is a *family* of algorithms with
// versioned behavior; the SP embed may not be Go at all. A small,
// version-locked xorshift64* is the only way to keep the SP and MMO
// views of "what RNG happened on tick T" identical forever.
//
// Why a stream name: two systems drawing from "rng" would interfere
// with each other — the first draw on tick T shifts the state, so the
// second draw returns a different number. Naming the stream lets each
// system seed independently from (universe_seed, stream_name, tick),
// so the order in which systems run on a tick is irrelevant to
// outcomes. Adding a new system tomorrow can't perturb today's draws.
//
// Why mix the name with FNV-1a: Go map iteration order is randomized,
// and the stream-name table is a map. FNV-1a on the name string
// gives a stable 64-bit seed prefix independent of any iteration
// order, so two states built from the same content but different map
// orders still draw the same numbers.
package universe

import (
	"hash/fnv"
)

// newStream returns a new xorshift64* state seeded from the universe
// seed, the named stream, and the current tick. The same (seed, name,
// tick) always produces the same first draw; that's the determinism
// guarantee the contract requires.
func newStream(universeSeed uint64, streamName string, tick int64) *xorshift64 {
	s := mixSeed(universeSeed, streamName, tick)
	return &xorshift64{s: s | 1} // ensure non-zero (xorshift requires it)
}

// mixSeed combines universe seed, stream name, and tick into a single
// 64-bit value. The mixing is two FNV-1a passes plus a splitmix64
// finalizer: enough to break correlation between (seed, tick) pairs
// even when the name is the same.
func mixSeed(universeSeed uint64, streamName string, tick int64) uint64 {
	h := fnv.New64a()
	var b [8]byte
	writeU64(b[:8], universeSeed)
	h.Write(b[:])
	h.Write([]byte(streamName))
	writeU64(b[:8], uint64(tick))
	h.Write(b[:])
	// splitmix64 finalizer — turns the FNV result into a valid
	// xorshift seed (non-zero, well-mixed).
	return splitmix64(h.Sum64())
}

func writeU64(out []byte, v uint64) {
	out[0] = byte(v)
	out[1] = byte(v >> 8)
	out[2] = byte(v >> 16)
	out[3] = byte(v >> 24)
	out[4] = byte(v >> 32)
	out[5] = byte(v >> 40)
	out[6] = byte(v >> 48)
	out[7] = byte(v >> 56)
}

// splitmix64 is the finalizer used by Java's SplittableRandom and
// several other PRNGs. Bit-stable across languages.
func splitmix64(x uint64) uint64 {
	x += 0x9E3779B97F4A7C15
	z := x
	z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
	z = (z ^ (z >> 27)) * 0x94D049BB133111EB
	return z ^ (z >> 31)
}

// xorshift64* — a small, fast, version-locked PRNG. Marsaglia's
// "Xorshift RNGs" paper (2003) describes the family; the "*" variant
// multiplies the output by a gold-ratio constant to improve the low
// bits. The constants here are from the paper; do not change them.
type xorshift64 struct {
	s uint64
}

// Next returns the next 64-bit draw.
func (x *xorshift64) Next() uint64 {
	x.s ^= x.s >> 12
	x.s ^= x.s << 25
	x.s ^= x.s >> 27
	return x.s * 0x2545F4914F6CDD1D
}

// IntRange returns a uniform integer in [lo, hi] inclusive. The draw
// is taken modulo (hi-lo+1); for the small ranges sim systems need
// (e.g. -1..+1) the bias is invisible.
func (x *xorshift64) IntRange(lo, hi int) int {
	if hi <= lo {
		return lo
	}
	span := uint64(hi - lo + 1)
	return lo + int(x.Next()%span)
}
