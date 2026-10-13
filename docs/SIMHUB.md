# SimHub (telemetrija busa)

SimHub-ova sopstvena podrška za OMSI 2 čita memoriju `Omsi.exe` preko OmsiHook-a i ne vidi
ovaj program. Zato igra sama šalje telemetriju busa u formatu koji SimHub već zna: paketi
**Forza Motorsport 7 „Data Out”** (311 bajtova, UDP). SimHub ih prima kao da igra Forzu, pa
njegove kontrolne table, bas šejkeri, LED trake i vetar rade sa busom.

## Podešavanje

1. U launcheru: **Podešavanja → Opšte → SimHub telemetrija** → izaberi UDP port (5300 je
   SimHub-ov podrazumevani za Forzu). Isto u igri: pauzni meni → Opcije → SimHub
   telemetrija. Podešavanje je `simhub_port` u `settings.cfg` (0 = isključeno).
2. U SimHub-u: **Games → Forza Motorsport 7** → *Data Out* port isti kao gore. SimHub ne
   traži da `ForzaMotorsport7.exe` radi: čim paketi stignu, prikazuje „Game running”.
3. Pokreni vožnju. Paketi idu oko 60 puta u sekundi na `127.0.0.1:<port>`; za SimHub na
   drugom računaru postavi `OMSI_SIMHUB_HOST=<adresa>` pre starta igre.

## Šta nosi

| Polje Forze | Iz busa |
| --- | --- |
| CurrentEngineRpm, EngineIdleRpm, EngineMaxRpm | `engine_n`; idle 600 i max 2500 kad skripta nema svoje |
| Speed (m/s), VelocityZ | brzina busa iz fizike |
| Accel, Brake, Clutch (0–255) | pedale |
| HandBrake | `bremse_feststell` |
| Gear (0 = R/N, 1…) | `antrieb_getr_aktugang`, `antrieb_getr_reverse` |
| Steer (−127…127) | ugao volana prema maksimalnom |
| AccelerationX/Y/Z | ubrzanje busa |
| Yaw, Pitch, Roll | kurs i nagib busa |
| Fuel (0–1) | `cockpit_tankuhr` (pokazivač goriva) |
| WheelRotationSpeed, suspension | točkovi prve dve osovine |
| PositionX/Y/Z, DistanceTraveled | položaj na mapi, `antrieb_wegstrecke` |

Svetla, žmigavci, vrata i IBIS nemaju mesto u Forzinom paketu; za njih postoji
`~/.openomsi/telemetry.json` (vidi `docs/PLUGINS.md`, „The telemetry file”) i Lua dodaci.

Kod: `crates/omsi-app/src/simhub.rs` (`sample`, `packet`, `publish`); poziv iz
`app_events/frame/interface.rs` pored `telemetry::publish`.
