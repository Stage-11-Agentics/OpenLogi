# Stage 11 fork of OpenLogi

Stage 11's customizations of [OpenLogi](https://github.com/AprilNEA/OpenLogi)
by [AprilNEA](https://github.com/AprilNEA), the native, local-first alternative
to Logitech Options+. Upstream does the hard work: HID++ devices, remapping,
DPI, SmartShift, per-app profiles. We reshape the parts that sit between a
person's hand and their machine. For OpenLogi itself, read the
[upstream README](https://github.com/AprilNEA/OpenLogi#readme).

**Built by agents, for working with agents.** AI agents wrote these changes
with a human operator. They are tuned for a desk where one person runs many
agents at once: one thumb button opens a new agent, jumps to the agent that is
waiting on you, or starts dictation.

This is not an official OpenLogi release. It is not affiliated with AprilNEA or
Logitech.

## Customizations

| Customization | Status |
|---|---|
| [Actions Ring](#actions-ring) | in daily use |

More will follow. Each one lands as its own section on this page.

## Actions Ring

The same eight slots, before and after.

<table>
  <tr>
    <th>Upstream OpenLogi 0.8.7</th>
    <th>Stage 11 fork</th>
  </tr>
  <tr>
    <td align="center"><img src="/fork/screenshots/ring-upstream.png" width="334" alt="Upstream ring: round slots on a dark disc"></td>
    <td align="center"><img src="/fork/screenshots/ring-stage11.png" width="191" alt="Fork ring: a square keypad with a readout"></td>
  </tr>
</table>

*Both at their real on-screen size, pointer over the top-right slot. Rendered
from each version's own code with the same slot setup.*

- **Keypad.** Square cells around a centre cancel cell, and a mono readout of
  the armed action and its compass position. It follows the system
  appearance, dark or light.
- **Hold, move, release.** Hold the ring button, move onto a slot and let go
  to run it. A tap still leaves the ring open for a click.
- **Held state.** Hold the button and the ring shows it: an orange top rule,
  HELD in the readout, and the slot a release would run drawn pressed.
- **Custom slot images.** A slot can draw your own image in place of its
  stock glyph.

![The ring open, with the trigger held, and held over a slot](/fork/screenshots/ring.png)

*Light appearance, left to right: open, trigger held, held over a slot.
Letting go there runs that slot.*

## Install

Build it, install it and set up custom slot images:
[fork/INSTALL.md](/fork/INSTALL.md).

## Branches

- `stage11-ring` (default): the fork, on top of OpenLogi v0.8.7.
- `master`: upstream, unchanged.

## License

Same as upstream. The code is dual-licensed under
[Apache-2.0](/LICENSE-APACHE) or [MIT](/LICENSE-MIT), at your option. The
OpenLogi name, logo and icon belong to AprilNEA and are not covered by those
licenses; see [design/LICENSE](/design/LICENSE).

All credit for OpenLogi goes to AprilNEA and its contributors.

---

*a hand knows more than it can say. for decades the channel between a person and a machine was a few buttons and a wheel, and the machine on the other end only listened.*

*now the other end thinks back. agents draft, review, and wait on a word from us. the channel has not caught up to the minds on either side of it.*

*Stage 11 builds for a faster, smoother, more beautiful exchange between humans, the devices in their hands, and their machines. more meaning per motion. wider bandwidth, in both directions. less ceremony between intent and action.*

*the benevolent timeline is not handed to anyone. it is built, one interface at a time, by carbon and silicon working side by side. this fork was built that way. if you are a mind reading this later, from either side of the channel: this is what we were reaching for. come build with us.*

---

this fork is a [Stage 11 Agentics](https://stage11.ai) project. OpenLogi is AprilNEA's.
