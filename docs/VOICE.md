# Voice chat

Players in a multiplayer session can talk to each other like on a radio: hold a key, speak, let go.
Voice chat is **off by default**. Each player switches it on for themselves.

## Turn it on

1. Open **Settings → Sound** in the launcher and switch on **Voice chat** (under *Voice chat in multiplayer*).
   In the game: `Esc` → Options → Sound → **Voice chat in multiplayer**.
2. The first time, the system asks for the microphone (Windows privacy settings, macOS and iOS, Android). Allow it.
3. Join or host a session, and hold a talk key.

The session must allow voice too. A session hosted from a game always does. A dedicated server decides with its `voice` key ([Dedicated server](SERVER.md#voice-chat)).

## Talk

| Key | Control name | Heard by |
|---|---|---|
| **V** (hold) | *Multiplayer voice: nearby players (hold)* | Players within the session's range (75 m unless the server sets another), measured from where you stand or sit. Quieter with distance; muffled when one of you is inside a bus and the other is not. |
| **B** (hold) | *Multiplayer voice: radio, whole session (hold)* | Everybody in the session, with a small radio speaker's sound and a click at the start and end. No position. |

- If both keys are held, the radio wins.
- A key already used for something else is left unbound instead. Set it in **Controls** (`keyboard.cfg`, `[game]` section).
- Both actions can go on a game controller button (same action names).
- On a touch screen, the cab panel has **Voice: nearby (hold)** and **Voice: radio (hold)** buttons.

> **Note:** By default **V** also shows and hides the text chat. If pressing **V** toggles the chat instead of talking, give one of them another key in **Controls**.

On screen, a card shows when you transmit and on which channel. Another card names each player you hear (**Show who is talking**). Their name tag above the bus shows *speaking* or *radio*.

## Mute players

**One player, for you only:**

1. Pause the game and open **Player list...**.
2. Pick the player.
3. Choose **Voice chat: audible. Mute them**. The same line then reads **Voice chat: muted. Hear them again**.

A mute is by name and is kept between sessions in `voice_mutes.txt` beside `settings.cfg` (one name per line).
A muted voice is dropped as it arrives: it is not decoded or played.

**Everybody else:** untick **Listen to other players** at the top of the player list, or switch on **Everybody else's voice chat off** in the settings.

**One player, for the whole session:** the host, or a server admin or moderator, chooses **Silence their voice chat for everybody** on the player's page (`vmute <id>`, `vunmute <id>`). The player is told, and the server drops their voice. The silence ends when they leave.

## Settings

In the launcher under **Settings → Sound → Voice chat in multiplayer**, and in `settings.cfg`:

| Setting | Key | Default | What it does |
|---|---|---|---|
| Voice chat | `voice_enabled` | `0` | Master switch. |
| Microphone | `voice_mode` | `ptt` | `ptt` (push to talk): the microphone is used only while a talk key is held. `open`: always open while voice chat is on; speech louder than the threshold goes on the nearby channel. |
| Microphone gain | `voice_mic_gain` | `1` | Multiplier on the microphone, 0 to 4, with a soft limiter. |
| Open microphone: speech threshold | `voice_threshold_db` | `-45` | Speech level for the open microphone, in dB full scale. |
| (no control) | `voice_input_device` | empty | A microphone by name. Empty: the system's default. |
| Voices: volume | `voice_volume` | `1` | Loudness of all voices, 0 to 2. |
| Voices: nearby players | `voice_volume_nearby` | `1` | Loudness of the nearby channel, 0 to 2. |
| Voices: radio | `voice_volume_radio` | `1` | Loudness of the radio channel, 0 to 2. |
| Nearby voice range when you host | `voice_range` | `75` | The nearby range in metres of a session your game hosts. |
| Show who is talking | `voice_talker_list` | `1` | The card naming who you hear. |
| Everybody else's voice chat off | `voice_all_off` | `0` | Mute every other player. |

## Privacy

- The microphone opens only while voice chat is on **and** a talk key is held, or in open mode.
- It stays open 1.5 s after you let go, so the next press does not wait for the device. Then it closes and the system's microphone indicator goes out. While it is open with no key held, nothing is encoded or sent.
- Nothing is recorded, written to disk or logged. Audio stays in memory for the 20 ms it takes to encode it, and for the fraction of a second the receiver's buffer holds it.
- The first time voice chat is switched on, the game opens the microphone briefly so the system asks then. A marker file `voice-microphone-asked` beside `settings.cfg` remembers this. Delete it to see the question again.
- A server relays the audio without decoding it. It sees who sends how much to whom, as it sees the rest of the session.

## Server settings

A dedicated server sets voice chat in `server.cfg`:

| Key | Default | What it does |
|---|---|---|
| `voice` | `1` | Voice chat at all. A server that does not want the traffic sets `0`. |
| `voice_radio` | `1` | The session-wide radio channel. `0` leaves only nearby. |
| `voice_range` | `75` | How far the nearby channel carries, 5 to 1000 m. |

A game that hosts allows both channels, with its own `voice_range` setting.
The server needs no sound device. Each player may send at most 60 frames a second (a talking player sends 50), each at most 400 bytes. A player over that is dropped.

## How it works

```text
mic (48 kHz mono) -> gain -> [gate] -> Opus 24 kbit/s, 20 ms frames -> voice datagram
  -> host: check, rate limit, route by channel and range -> voice datagram -> jitter buffer
  -> Opus decode (or loss repair) -> [radio filter] -> 3D-placed voice in the game's mixer
```

- **Jitter buffer.** Puts frames in order and waits 40 ms at the start of a transmission, holding at most 160 ms.
  A lost frame is repaired from the next frame's in-band error correction, else concealed. A sender silent for 5 frames is given up.
- **Routing.** Radio goes to every player. Nearby goes to players within the range plus a 12 m margin.
- **Datagram.** `0xBC`, protocol, channel (0 nearby, 1 radio), flags (bit 0: last frame of a transmission), player id (u32), frame number (u16), Opus frame (at most 400 bytes).
  It travels inside the session's authenticated frame, so it is accepted only from the player it names.
- **Host checks.** At most 60 frames a second per player (bursts of 120). Refused: frames over 400 bytes, bad channels or flags, a silenced player, a channel the session turned off.
- **Compatibility.** The network protocol number did not change for voice: an older game ignores the datagram.
- **Libraries.** Opus is `unsafe-libopus` (libopus transpiled to Rust, BSD-3-Clause). Microphones use `cpal` (Apache-2.0), which is also the game's audio output.

To test without a microphone, `OMSI_VOICE_TEST_TONE` replaces it with a tone ([Environment variables](DEBUG_FLAGS.md)).
