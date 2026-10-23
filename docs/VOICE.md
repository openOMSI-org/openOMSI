# Voice chat

Players in a multiplayer session can talk to each other like on a radio: hold a key, speak,
let go. It is **off by default** and each player switches it on for themselves.

## Using it

Switch it on in the launcher (Settings → Sound → *Voice chat*) or in the game menu (Options →
Sound → *Voice chat in multiplayer*). The system asks for the microphone the first time
(macOS and iOS, Android, Windows' privacy setting). Then, in a session:

| Key (Controls) | Default | Heard by |
|---|---|---|
| *Multiplayer voice: nearby players (hold)* `voice_nearby` | V | the players within the session's range (75 m unless the server says otherwise), from where you stand or sit; quieter with the distance, muffled when one of you is in a bus and the other is not |
| *Multiplayer voice: radio, whole session (hold)* `voice_radio` | B | everybody in the session, on a small speaker's radio sound (300 Hz to 3.4 kHz, compressed), with a click when the key goes down and up; no position |

A key that is already used for something else on your machine is left unbound instead
(`keyboard.cfg [game]`, set it in Controls). Both can be put on a game controller button (the
same action names) and, on a touch screen, the cab panel has a *Voice: nearby* and a *Voice:
radio* button (hold). If both keys are down, the radio wins.

On the screen: a card says when you are transmitting and on which channel, another one names
each player you hear (*Settings → Show who is talking*), and their name tag shows *speaking*
or *radio*.

### Muting

Pause, open the *Player list*, pick a player: *Mute them* / *Hear them again*. A mute is by
name, kept between sessions in `~/.openomsi/voice_mutes.txt` (a text file, one name a line;
the game never gets other players' hardware identifiers). *Voice chat of everybody else* at the
top of the list (or *Everybody else's voice chat off* in the settings) switches every other voice
off. A muted voice is dropped as it arrives: it is not decoded or played.

An admin or moderator can silence a player for the whole session (the player's page in the
Player list or Administration menu; `vmute <id>` / `vunmute <id>`). The silenced player is
told, and the server drops their voice frames.

### Settings (`settings.cfg`, also in the launcher)

| Key | Default | |
|---|---|---|
| `voice_enabled` | 0 | master switch |
| `voice_mode` | `ptt` | `ptt`: the microphone is used only while a talk key is held. `open`: it is open all the time voice chat is on, and what is louder than `voice_threshold_db` is sent on the nearby channel |
| `voice_mic_gain` | 1 | multiplier on the microphone (0 to 4), with a soft limiter |
| `voice_threshold_db` | -45 | the open microphone's speech threshold (dB full scale) |
| `voice_input_device` | (system's) | a microphone by name; empty: the system's default |
| `voice_volume`, `voice_volume_nearby`, `voice_volume_radio` | 1 | loudness of all voices, and of each channel (0 to 2) |
| `voice_range` | 75 | the nearby range (m) of a session this game hosts |
| `voice_talker_list` | 1 | show who is talking |
| `voice_all_off` | 0 | everybody else's voice off |

## Privacy

* The microphone is not opened unless voice chat is on **and** a talk key is held (or the open
  mode is chosen). It stays open for 1.5 s after the key is let go so that the next press does
  not wait for the device, and is then closed (the operating system's microphone indicator
  goes out). While it is open but no key is held, nothing is encoded or sent.
* Nothing is recorded, written to disk or logged. Audio lives in memory for the 20 ms it takes
  to encode it and for the fraction of a second the receiver's buffer holds it.
* The first time voice chat is switched on the game opens the microphone for a moment so that
  the system's question comes then (a marker file `voice-microphone-asked` beside
  `settings.cfg` remembers it; delete it to see the question again). On Android the
  `RECORD_AUDIO` permission is requested at runtime; on macOS and iOS
  `NSMicrophoneUsageDescription` is in the Info.plist files (and `audio-input` in the sandbox
  and hardened-runtime entitlements).
* A server relays frames it cannot hear into: it does not decode them. It sees who sends how
  much to whom, as it sees everything else of the session.

## How it works

```
mic -> (cpal, 48 kHz mono) -> gain -> [gate] -> Opus 24 kbit/s, 20 ms -> LAN datagram 0xBC
   -> host: check, rate limit, route by channel/range -> LAN datagram -> jitter buffer
   -> Opus decode (or loss repair) -> [radio filter] -> omsi-audio stream voice, 3D placed
```

* `crates/omsi-voice`: the engine. `capture` (microphone through cpal, converted to 48 kHz
  mono), `codec` (Opus), `jitter` (reordering, 40 ms start-up delay, at most 160 ms, loss
  repair from the next frame's in-band error correction else the decoder's concealment, a
  sender silent for 5 frames is given up), `dsp` (gain and limiter, gate, the radio's band,
  compression and squelch clicks, the distance curve), `engine` (the threads: capture and
  encode, receive and decode; nothing blocks the game), `mutes`.
* `crates/omsi-net/src/voice_net.rs`: the datagram and the relay, no audio. Layout:
  `0xBC, protocol, channel (0 nearby, 1 radio), flags (bit 0: last frame of a transmission),
  player id (u32), frame number (u16), Opus frame (at most 400 bytes)`. It travels inside the
  session's `0xEA` frame with the sender's secret, like variable and sound packets, so it is
  accepted only from the player it names. The host takes at most 60 frames a second per
  player (bursts of 120), refuses frames over 400 bytes, bad channels and flags, a silenced
  player, a channel the session switched off, and passes the rest on: radio to every
  player, nearby to those within the range plus a margin of 12 m of the speaker. The LAN
  protocol number is unchanged: an older game ignores the datagram.
* `crates/omsi-app/src/voice_chat.rs`: the game's side: keys, the policy the host told
  (`voice?` / `voice <on> <radio> <range>`), the volumes, the placement of each speaker in
  the world (position, distance gain, a low-pass through a bus's body).
* Opus is `unsafe-libopus` 0.2 (libopus transpiled to Rust, BSD-3-Clause like libopus): it
  builds with plain cargo on Windows, macOS, Linux, Android and iOS, no C compiler, cmake or
  system library. Microphones are `cpal` 0.15 (Apache-2.0), already the game's audio output.

## Server

`server.cfg`: `voice` (1), `voice_radio` (1), `voice_range` (75); see [SERVER.md](SERVER.md).
A game that hosts allows both channels with the range of its own `voice_range` setting.
The server has no audio code path to run: it needs no sound device.

## Testing without a person

`OMSI_VOICE_TEST_TONE=1` (or a pitch in Hz) replaces the microphone by a tone made at the
pace of real time; the device is never opened. The example `voice_peer` of `omsi-voice` is a
whole player in one process:

```
OMSI_VOICE_TEST_TONE=440 cargo run -p omsi-voice --example voice_peer -- host 27500 12 listen 0
OMSI_VOICE_TEST_TONE=880 cargo run -p omsi-voice --example voice_peer -- join 27500 12 radio 40
```

Each prints `RESULT ... sent=N received=M max_level_db=...`: the level of what it decoded. A
third process far from the speaker (`listen 900`) hears the radio and not a nearby talker.
`cargo test -p omsi-voice -p omsi-net voice` covers the codec, the jitter buffer, the radio and
distance shaping through a real (deviceless) mixer, routing by range, the relay's limits and
a flood.
