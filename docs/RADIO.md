# Radio

openOMSI plays internet radio through the bus's own radio, with no plugin needed. This page says
which buses play it, how to set your stations, and what to do when nothing plays.

OMSI 2 plays no music itself: a bus's radio only sets variables that radio plugins turn into
sound. openOMSI reads those variables and plays the stream.

## Which buses

Every bus whose radio sets one of these variables:

- `Snd_Radio`: the cassette player of the stock SD200, SD202 and NL and of many mods. It plays the
  first station.
- `SndExt_Radio`: radios made for the Sound Extension plugin. Station button *n* plays the *n*-th
  station, and the volume knob `SndVol_Radio` sets how loud.

Switch the radio on in the cockpit as in OMSI. The screen says which station plays, and the song
when the stream names it.

## Your stations

`~/.openomsi/radio.cfg` (on Windows `C:\Users\<you>\.openomsi\radio.cfg`) is written with a few
stations the first time the game starts. One station a line; the first line is on the first
button:

```text
volume = 0.7
Radiozurnal = https://rozhlas.stream/radiozurnal.mp3
Evropa 2 = https://ice.actve.net/fm-evropa2-128
My playlist = https://example.org/station.m3u
```

- A station is an MP3, AAC or Ogg stream, or an `.m3u` / `.pls` playlist pointing to one.
- The address is the one a media player opens: from the station's website, or a directory such as
  radio-browser.info.
- Streams in HE-AAC with a program config element (some `.aacp` stations) cannot be played.
- `volume` (0 to 1) is the radio's loudness on top of the knob.

The file is read when the game starts.

### Editing in the launcher

**Settings → Sound → Radio stations** edits the same list:

1. Type a name and an address for a station.
2. **Add a station** adds a line; the bin removes one.

It is saved at once. The file's comments, `volume` and frequencies stay as they are.

### Stations of a radio plugin

Stations already set up for an OMSI radio plugin (SuperRadio and the like) are taken over: every
line with an `http(s)` address in the text files under `plugins` becomes a station, after those of
`radio.cfg`.

### More stations than buttons

**Shift+R** moves the whole list one station on, so the buttons reach the stations behind the
first ones.

## A map's stations

A map may bring its own `radio.cfg` beside its `global.cfg`. Its stations come first on the buttons
while you drive on that map, yours after them. It may also name the frequency each station is on
at places of the map. How to write one: [Modding overview](MODDING.md).

## The radio's display

A radio with a text display shows the station and the song, scrolling where they do not fit. On
Dmitrij's "Magnitola" of P3ta's SOR buses and similar radios (ten characters), the first line shows
the map's frequency for the place, for example `94.6 MHz`. Without stations, or with the radio off,
a display shows its own texts.

## How it sounds

In the cab the radio sounds from the driver's corner, a little ahead of the driver's eyes. Towards
the back of the bus it is heard from the front but does not fade away, as a bus's speakers are
spread along it. Changing the view, the sound glides to its new side in a quarter of a second.
From outside, the bus plays through its windows.

Three sliders in **Settings → Sound** (in the game: **Options → Sound**) colour the sound. All are
off at first:

| Slider | Key | What it does |
| --- | --- | --- |
| Radio: small speaker | `radio_speaker` | a small dashboard speaker: no deep bass, no bright treble, nearly mono |
| Radio: FM hiss | `radio_hiss` | the hiss and crackle of a weak signal |
| Radio: signal dropouts | `radio_dropouts` | the signal lost now and then for a moment (a bridge, a tall building) |

With all three at 0 the stream plays untouched.

## When nothing plays

Look in `~/.openomsi/game.log`:

| Log line | Meaning |
| --- | --- |
| `radio: 23 stations` | the list was read |
| `radio: station 1 Radiozurnal (https://…)` | a button was pressed |
| `Radio 1 plays Radiozurnal (buffering …)` | the stream plays, then the song |
| `no signal`, with a `radio <url>: …` line | the reason: mostly an address that is not a stream, or a station that is down |

No `radio: station` line at all means the bus's radio sets neither variable.
