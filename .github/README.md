# Actions Ring fork of OpenLogi

This is Stage 11's fork of [OpenLogi](https://github.com/AprilNEA/OpenLogi) by
[AprilNEA](https://github.com/AprilNEA), a native, local-first alternative to
Logitech Options+. The fork changes one surface, the Actions Ring. Everything
else is upstream. For OpenLogi itself, read the
[upstream README](https://github.com/AprilNEA/OpenLogi#readme).

It is not an official OpenLogi release. It is not affiliated with AprilNEA or
Logitech.

![The ring open, with the trigger held, and held over a slot](/fork/screenshots/ring.png)

*Left to right: the ring open, the trigger held, and held over a slot. Letting
go there runs that slot.*

## What changes

- **Keypad ring.** The eight slots sit in a 3×3 keypad of square cells around
  a centre cancel cell. A mono readout names the armed action and its compass
  position. The ring follows the system appearance, dark or light.
- **Hold, move, release.** Hold the ring button, move onto a slot and let go
  to run it. A tap, or a release over the centre, a gap or an empty cell,
  leaves the ring open for a click. A release counts only after 12 pt of
  travel from where the ring opened.
- **Held state.** Once the button has been down 180 ms, the ring shows it
  held: an orange rule along the top edge, HELD in the readout, and the slot a
  release would run drawn pressed. A tap never shows it.
- **Custom slot images.** A slot with a custom label can draw your own image
  in place of its stock glyph. See below.
- **Preview harness.** The `ring-preview` cargo feature opens a static sample
  ring for screenshots, with no agent and no IPC. Release builds leave it off.

### Wire compatibility

`ActionRingInvocation` gains two trailing fields, `trigger_released` and
`release_selects`. Only the agent and the overlay exchange it, so ship those two
together. `PROTOCOL_VERSION` is not bumped, on purpose: the GUI and CLI demand an
exact version match, and a bump would lock out upstream builds of them.

## Custom slot images

Give a ring slot a custom label, then put images named after the label in the
`ring-icons` folder of OpenLogi's config directory (`~/.config/openlogi` on
macOS and Linux; dev builds use `~/.config/openlogi-dev`):

| File | Drawn on | Ink |
|---|---|---|
| `<label>.svg` or `<label>.png` | dark cells | light |
| `<label>.light.svg` or `<label>.light.png` | light cells: the light sheet, or a pressed cell on the dark one | dark |

Without a `.light` file, the slot shows its stock glyph on light cells. Labels
that could escape the folder are ignored.

[`fork/ring-icons/`](/fork/ring-icons/) holds sample images and the macOS Swift
scripts that draw them. `draw_glyphs.swift` draws the glyph set:

```sh
swiftc -O fork/ring-icons/draw_glyphs.swift -o draw
./draw <out dir>          # light ink, for dark cells
./draw <out dir> light    # dark ink (#111214), for light cells
```

`raster_svg.swift` rasterizes an SVG to a 128 px PNG.

## Build

The toolchain is upstream's: see [docs/DEVELOPMENT.md](/docs/DEVELOPMENT.md).
On macOS that means full Xcode with the Metal Toolchain.

Run the fork as a dev app. This builds and starts the fork's agent and overlay
too, under dev bundle identifiers and the `openlogi-dev` profile, so an
installed OpenLogi is left alone:

```sh
cargo run -p openlogi-desktop
```

Package a DMG signed with your own Developer ID:

```sh
OPENLOGI_SIGN_IDENTITY="Developer ID Application: <name> (<team id>)" \
  cargo run -p xtask -- macos package
```

### Patch an installed OpenLogi 0.8.7 (macOS)

Build the two helpers:

```sh
cargo build --release -p openlogi-agent -p openlogi-overlay
```

Then, in a staging copy of `OpenLogi.app`:

1. Copy both binaries into
   `Contents/Library/LoginItems/OpenLogi {Agent,Overlay}.app/Contents/MacOS/`.
2. Re-sign with your Developer ID
   (`codesign --force --options runtime --timestamp -s "<identity>"`): each
   helper app, then `Contents/MacOS/openlogi` with
   `--preserve-metadata=identifier,entitlements`, then the outer app with
   `--preserve-metadata=entitlements`.
3. Check it with `codesign --verify --deep --strict`.
4. Quit OpenLogi, swap the staged app into `/Applications`, and start the agent.

Always re-sign the whole app. Replacing a helper without it breaks the outer
seal, and macOS then refuses to launch the agent. A new signer means granting
Input Monitoring and Accessibility again.

An OpenLogi update replaces the patched helpers. Rebase `stage11-ring` onto the
new release tag and rebuild.

### Preview screenshots (macOS)

Build the overlay with the preview feature and put the binary next to the
script:

```sh
cargo build --release -p openlogi-overlay --features ring-preview
cp target/release/openlogi-overlay fork/preview/openlogi-overlay-preview
fork/preview/shot.sh <mode> <hover> <empty> <name> [held] [light|dark]
```

The script opens the sample ring, captures it, and kills it on a hard timer.
`screencapture` fails while the screen is locked.

## Branches

- `stage11-ring` (default): the fork, on top of OpenLogi v0.8.7.
- `master`: upstream, unchanged.

## License

Same as upstream. The code is dual-licensed under
[Apache-2.0](/LICENSE-APACHE) or [MIT](/LICENSE-MIT), at your option. The
OpenLogi name, logo and icon belong to AprilNEA and are not covered by those
licenses; see [design/LICENSE](/design/LICENSE).

All credit for OpenLogi goes to AprilNEA and its contributors.
