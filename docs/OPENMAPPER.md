# openMapper

openMapper is a simple map editor for openOMSI. Draw roads with a couple of clicks, place
buildings and trees from a visual catalogue, shape and paint the ground, and drive your map in
openOMSI with a single button. It is built for anyone to pick up, not just for experienced map
makers.

> **Early release: expect bugs.** openMapper is new. Things may be missing, broken or change
> between versions. Report problems on [GitHub](https://github.com/openOMSI-org/openMapper/issues)
> or on [Discord](https://discord.gg/openomsi). Keep a copy of a map you care about: saving keeps
> the last ten versions, but a copy is cheaper than a regret.

openMapper needs an installed OMSI 2 (Steam or retail): maps are built from its content. The
original folder is never written to. Maps and imported assets go into openOMSI's content folder,
which the game reads before the installation.

## What it can do

* **Roads and junctions:** two clicks make a road; junctions, traffic lights, crossings, switches
  and roundabouts come by themselves.
* **Objects and plants:** a visual catalogue of the installation's buildings, trees and splines,
  lining up with roads, and a plant brush.
* **Terrain and paint:** sculpting brushes, ramps, height maps, ground texture painting, tiles and
  water.
* **Real places:** satellite imagery, terrain heights and OpenStreetMap (roads, stops, lines,
  buildings, trees, water).
* **Lines and traffic:** timetable, tours, depot files, AI traffic and paths.
* **Your own models:** import glTF, FBX, OBJ and more, edit meshes, make materials and PBR maps,
  and export.
* **Check and drive:** a map check, the game's own checks, safe saving with earlier versions, and a
  test drive in openOMSI on one key (`F5`).

## Download

Every release is on the [Releases page](https://github.com/openOMSI-org/openMapper/releases/latest),
built for each system and processor:

| Platform | Intel / AMD (x64) | ARM64 |
| --- | --- | --- |
| Windows 10 / 11 | `openMapper-<version>-windows-x64.zip` | `openMapper-<version>-windows-arm64.zip` |
| macOS 11 or newer | `openMapper-<version>-macos-x64.zip` | `openMapper-<version>-macos-arm64.zip` (Apple silicon) |
| Linux | `openMapper-<version>-linux-x64.tar.gz` | `openMapper-<version>-linux-arm64.tar.gz` |

* **Windows:** unpack the zip and run `openmapper.exe`. Nothing else is needed.
* **macOS:** unpack the zip and move `openMapper.app` to Applications. The app is not notarised:
  the first time, right-click it and choose Open.
* **Linux:** unpack the archive and run `./openmapper`. It needs GTK 3 for the file dialogs and a
  Vulkan or OpenGL driver.

## Learn more

* [Your first map](#/mapper/docs/TUTORIAL): from an empty window to a map you can drive in service.
* [Features and controls](#/mapper/docs/FEATURES): everything it can do, and the keys.
* [Roadmap](#/mapper/docs/ROADMAP): what comes next.
* [Source code](https://github.com/openOMSI-org/openMapper) and the
  [changelog](https://github.com/openOMSI-org/openMapper/blob/main/CHANGELOG.md).
