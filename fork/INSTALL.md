# Install and configure the Stage 11 fork

Everything here applies to the `stage11-ring` branch. The README covers what
the fork changes and why; this page covers how to build it, install it and
set it up.

## Requirements

The toolchain is upstream's: see [docs/DEVELOPMENT.md](/docs/DEVELOPMENT.md).
On macOS that means full Xcode with the Metal Toolchain.

## Try it as a dev app

```sh
cargo run -p openlogi-desktop
```

This builds and starts the fork's agent and overlay too, under dev bundle
identifiers and the `openlogi-dev` profile. An installed OpenLogi is left alone.

## Package a signed DMG

```sh
OPENLOGI_SIGN_IDENTITY="Developer ID Application: <name> (<team id>)" \
  cargo run -p xtask -- macos package
```

## Patch an installed OpenLogi 0.8.7 (macOS)

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

## Preview screenshots (macOS)

The `ring-preview` cargo feature opens a static sample ring with no agent, no
IPC and no hardware, so an agent can render and check the ring on its own.
Release builds leave it off.

```sh
cargo build --release -p openlogi-overlay --features ring-preview
cp target/release/openlogi-overlay fork/preview/openlogi-overlay-preview
fork/preview/shot.sh <mode> <hover> <empty> <name> [held] [light|dark]
```

The script opens the sample ring, captures it, and kills it on a hard timer.
`screencapture` fails while the screen is locked.

## Wire compatibility

`ActionRingInvocation` gains two trailing fields, `trigger_released` and
`release_selects`. Only the agent and the overlay exchange it, so ship those two
together. `PROTOCOL_VERSION` is not bumped, on purpose: the GUI and CLI demand an
exact version match, and a bump would lock out upstream builds of them.
