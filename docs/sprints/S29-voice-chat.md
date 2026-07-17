# S29 — Voice Chat (WebRTC)

**Spec:** §8 (server routing), §14 Mode 3 (space flight proximity) ·
**Wave 7 (tooling) · Depends on:** S23 (MMO presence, interest scoping)

## Outcome

Players in the same star system can talk to each other in real time.
Proximity-based spatial audio places voices in space — a ship 500m away is
quiet and directionally panned; a ship 50m away is loud and centered.
Push-to-talk with a HUD indicator. Per-player mute and volume controls.
WebRTC peer connections carry audio P2P; the existing WebSocket carries
signaling. A TURN server handles NAT traversal for players behind symmetric
NAT. Voice is system-scoped only — no global voice, no cross-system calls.

## Context

- S23 adds player presence (who is in the system), interest scoping
  (messages fan out per-system), and text chat. Voice builds on the exact
  same scoping: you can hear anyone you can see in the system.
- WebRTC is the standard for P2P real-time audio. The browser (WASM target)
  has native `RTCPeerConnection` support. Native builds use a Rust WebRTC
  library (`webrtc-rs` or `livekit-rust-sdks`). This sprint uses `webrtc-rs`
  for native and the browser's built-in WebRTC API for WASM — the signaling
  protocol is identical across both.
- TURN is external infrastructure: a coturn instance or a cloud TURN service
  (Twilio, XirSys). The server doesn't relay audio — it only tells peers
  the TURN server address. TURN config is an env var.
- Audio codec: Opus (WebRTC default). 64kbps full quality for 1 speaker,
  scaling down to 16kbps with more concurrent speakers. Opus handles
  bandwidth adaptation natively.
- The existing WS already has an mpsc outbound channel and per-system
  interest scoping (S23). Signaling messages are just new `ServerMessage`
  variants that respect the same interest scope — a signaling offer only
  goes to the target peer, not the whole system.
- Spatial audio positioning uses Bevy's `SpatialAudio` or manual pan/volume
  computation from the remote player's ship position relative to the local
  camera. Bevy's audio system already supports `PlaybackSettings` with
  spatial fields.

## Freeze first

### Signaling protocol (`network/messages.rs` extension)

```rust
// Client → Server (relayed to target peer)
ClientMessage::VoiceSignal {
    target_player: String,
    signal: VoiceSignalPayload,
}

// Server → Client (from a peer)
ServerMessage::VoiceSignal {
    from_player: String,
    signal: VoiceSignalPayload,
}

pub enum VoiceSignalPayload {
    Offer { sdp: String },
    Answer { sdp: String },
    IceCandidate { candidate: String, sdp_mid: String, sdp_mline_index: u16 },
    Hangup,
}
```

### Voice room state (`src/services/voice.rs`)

```rust
pub struct VoiceRoom {
    pub system_id: String,
    pub peers: HashMap<String, VoicePeerState>,  // player_id → state
}

pub struct VoicePeerState {
    pub player_id: String,
    pub muted: bool,
    pub speaking: bool,
    pub last_signal: chrono::DateTime<chrono::Utc>,
}

pub trait VoiceRoomStore: Send + Sync {
    fn join(&self, system_id: &str, player_id: &str);
    fn leave(&self, system_id: &str, player_id: &str);
    fn peers_in(&self, system_id: &str) -> Vec<String>;
    fn mute(&self, player_id: &str, target: &str, muted: bool);
    fn is_muted(&self, player_id: &str, target: &str) -> bool;
    fn set_speaking(&self, player_id: &str, speaking: bool);
}
```

Memory impl; Redis impl behind `REACHLOCK_REDIS` for multi-instance (voice
rooms are per-system — same scoping as S23 position fan-out).

Wire tests: signaling messages serialize round-trip; `VoiceSignalPayload`
discriminants match the WebRTC signaling state machine; new `ServerMessage`
variants added and pinned with the existing wire-shape test.

## Deliverables

### 1. WebRTC signaling over WebSocket (`src/ws/handler.rs` expansion)

- [ ] New message routing in `route()`: `ClientMessage::VoiceSignal` →
      validate `target_player` is in the same system (S23 interest map) →
      forward as `ServerMessage::VoiceSignal` to the target's socket.
      If the target is not in the system, return `ServerMessage::Error {
      message: "player not in system" }`.
- [ ] `ClientMessage::VoiceSignal { target, VoiceSignalPayload::Hangup }` →
      tears down the peer connection. Forwarded to target; both sides clean
      up the `RTCPeerConnection`.
- [ ] Signaling messages are NEVER broadcast — they are point-to-point only.
      A voice offer is between two specific players.
- [ ] Rate limit: 10 signaling messages per second per connection. Burst of
      20 (SDP offers/answers and ICE candidates arrive in quick succession
      during connection setup). Exceeding → close signaling silently for 5
      seconds (not the whole connection — just drop signaling messages).
- [ ] Test: send offer from A to B → B receives it; send offer from A to C
      (different system) → A gets an error.

### 2. Voice room management (`src/services/voice.rs`)

- [ ] `VoiceRoomStore` trait with `MemoryVoiceRoomStore` impl. Rooms keyed
      by `system_id` — same scoping as S23's position fan-out.
- [ ] On WS connect: player joins no room (they're not in a system yet).
- [ ] On `seed.discover` or `player.position` (first system entry): player
      joins the voice room for that system. Existing peers get a new-peer
      notification (used by the client to pre-warm WebRTC connections).
- [ ] On system departure (entering a new system or disconnect): player
      leaves the old voice room. Existing peers get a `VoiceSignal::Hangup`
      notification (client tears down the peer connection).
- [ ] Mute state: per-player mute list. Muting is client-enforced (the muted
      player still sends audio; the muting client drops the incoming audio
      track). The server stores mute state so it persists across reconnects.
- [ ] Speaking indicator: client sends `VoiceSignal::SpeakingStart` /
      `VoiceSignal::SpeakingStop` when PTT is pressed/released or VAD
      triggers. Server relays to all room peers. Used for the HUD speaking
      indicator (glowing nameplate).
- [ ] Test: A joins system → B joins same system → A's peer list includes B;
      A leaves → B gets hangup notification.

### 3. Client WebRTC (`reachlock-client/src/systems/voice.rs`)

- [ ] WebRTC peer connection management: on `VoiceSignal::Offer` from a new
      peer, create an `RTCPeerConnection`, set remote description, create
      answer, send back via `VoiceSignal::Answer`. On ICE candidates,
      exchange until the connection is established.
- [ ] Platform abstraction: WASM uses `web_sys::RTCPeerConnection` (browser
      API). Native uses `webrtc` crate (`webrtc-rs`). Abstract behind a
      `PeerConnection` trait with `create_offer()`, `handle_answer()`,
      `add_ice_candidate()`, `close()`. Compile-time feature flags select
      the backend (`wasm` vs `native`).
- [ ] Audio pipeline: incoming audio track → Bevy `AudioSource` with
      `PlaybackSettings { spatial: true, spatial_scale: SPATIAL_SCALE }`.
      The audio source's transform is updated every frame to the remote
      player's ship position relative to the listener (local camera).
- [ ] Push-to-talk: `V` key (configurable). Hold to transmit, release to
      mute. HUD indicator: microphone icon turns green when transmitting,
      red when muted (self-mute), dim when idle. Target key documented in
      the HELP string alongside S19's combat keys.
- [ ] Voice activity detection (optional, feature-flagged): if PTT is not
      pressed but the player is speaking above a threshold, auto-transmit.
      This is a quality-of-life feature for controllers/VR; default off.
- [ ] Per-player volume slider: accessible from the proximity roster (S23's
      player list). Volume applied as a gain multiplier on the incoming
      audio track before spatialization.
- [ ] Test: two clients in integration test — establish WebRTC connection
      via signaling relay, verify audio track is created and spatial
      position updates with ship movement.

### 4. TURN server integration

- [ ] Config: `REACHLOCK_TURN_URL`, `REACHLOCK_TURN_USERNAME`,
      `REACHLOCK_TURN_CREDENTIAL`. If set, the server includes TURN config
      in the WS handshake welcome message (`ServerMessage::TurnConfig { urls,
      username, credential }`).
- [ ] The client configures `RTCPeerConnection` with the TURN server before
      creating offers/answers. TURN is a fallback relay — peers try direct
      P2P first (STUN via TURN server's STUN endpoint); TURN relays only
      when NAT traversal fails.
- [ ] Without TURN config, connections are STUN-only (direct P2P). This
      works for ~85% of players; the remaining ~15% behind symmetric NAT
      need TURN. The fallback experience is: no TURN config → connection
      fails → "Voice unavailable: network restricted" in HUD, text chat
      works fine. This is acceptable for launch.
- [ ] Test: with TURN config set → client configures peer connection with
      TURN; without config → STUN-only, connection may fail behind NAT
      (expected, documented).

### 5. Bandwidth management

- [ ] Opus codec config: 64kbps, 48kHz, stereo (or mono — measure the
      bandwidth difference and pick the lower overhead option).
- [ ] Concurrent speaker scaling: track the number of active audio tracks.
      1-3 speakers → 64kbps each. 4-8 speakers → 32kbps each. 9+ speakers
      → 16kbps each. Adjust the Opus encoder bitrate on active tracks when
      speaker count changes.
- [ ] Audio source culling: peers further than `MAX_VOICE_RANGE` (2,000
      world units, ~1 system sector) have their audio tracks paused (not
      closed — reconnecting on re-entry is expensive). Tracks resume when
      the peer re-enters range.
- [ ] Test: simulate N speakers → verify bitrate scales at the configured
      thresholds.

### 6. Moderation hooks

- [ ] Per-player mute: client-side enforcement. Muted player's audio track
      is dropped. Mute state persisted in `VoiceRoomStore` and survives
      reconnects.
- [ ] Report mechanism: right-click a player in the proximity roster →
      "Report voice abuse" → sends `ClientMessage::ReportPlayer { target,
      reason }` → server logs to audit trail (S26) with a snapshot of
      the reporting player + target + system + timestamp. No audio recording
      (privacy + legal). Operator reviews reports manually.
- [ ] Global mute (admin): `POST /admin/players/:id/voice-mute` → prevents
      the player from sending voice signals. Their client gets a
      `ServerMessage::VoiceMuted { reason }` message. Reversible via
      `POST /admin/players/:id/voice-unmute`.
- [ ] Test: mute player B → B's audio track dropped in A's client; report
      player B → audit log entry created; admin voice-mute → B cannot
      transmit.

## Acceptance gates

```
cargo test -p reachlock-server voice:: signaling::
# Two-client integration: A enters system → B enters → WebRTC established
#   via WS signaling → A speaks → B hears spatialized audio → B moves →
#   audio pan/volume changes → A leaves → B's connection closes
# Signaling rate limit: flood signals → dropped after burst
make check
```

Manual: two game clients in the same system → press V to talk → hear each
other with spatial positioning. Fly 2000 units apart → audio fades out. Mute
the other player → their nameplate shows muted icon. Fly back within range →
audio resumes. Admin voice-mute a player → they can't transmit.

## Non-goals

- Global voice channels (system-scoped only — no lobby voice, no fleet voice)
- Cross-system voice (requires server-side audio mixing; violates P2P
  architecture)
- Voice chat in Landed/OnBoard modes (Space Flight only for launch — landed
  voice is Phase 4 polish)
- Recording / clip capturing (legal/privacy; operators use external tools)
- Video / screensharing (voice-only)
- Echo cancellation / noise suppression beyond what Opus/WebRTC provides
  natively (browser WebRTC has AEC built in; native may need additional
  processing — document, don't implement)
- Mobile voice (WASM in mobile browsers supports getUserMedia; native mobile
  is Phase 4)
- Voice activity detection as default (PTT is the primary input; VAD is
  feature-flagged optional)

## Gotchas

- `webrtc-rs` is a large crate with a complex async API. The native build
  must not block the Bevy frame loop. Run the WebRTC event loop on a
  separate Tokio runtime (or a dedicated thread) — the Bevy `Main` schedule
  must never await a WebRTC future.
- WASM `RTCPeerConnection` calls `getUserMedia` which requires a user
  gesture AND a secure context (HTTPS or localhost). The web shell (S24)
  must serve over HTTPS for voice to work. Document this — `make web-serve`
  for local dev won't support voice unless it's HTTPS or localhost.
- ICE candidate gathering is asynchronous and can take seconds. During
  connection setup, show a "Voice connecting..." indicator. If gathering
  exceeds 10 seconds (ICE timeout), show "Voice unavailable: connection
  timed out" and fall back to text-only.
- The signaling protocol adds new `ClientMessage` and `ServerMessage`
  variants. This is a protocol revision — update the wire-shape test and
  version-tag the protocol per S23's gotcha ("the protocol version handshake
  must land BEFORE the new messages").
- Spatial audio panning must use Bevy's `SpatialListener` component on the
  player's camera and `SpatialAudio` components on incoming audio sources.
  Bevy 0.18's spatial audio API is in `bevy::audio::SpatialAudio` — confirm
  the exact path before committing, as Bevy's audio module has been in flux.
- TURN server credentials are sent to the CLIENT in plaintext over WS
  (necessary for WebRTC ICE config). This is standard — TURN credentials
  are short-lived (typically 24h) and scoped to a single user. If using a
  cloud TURN service, they handle credential rotation. If self-hosting
  coturn, use `use-auth-secret` + time-limited credentials.
