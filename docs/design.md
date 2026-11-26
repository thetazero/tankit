# Design system, v0

The code is the reference: tokens and components are in `src/ui/design.rs`.
The main menu, Find work, the contract page, the garage, settings, pause
and the scoreboard (and what it
opens into at a round's end) are made from it; the other screens aren't yet.

## Principles

1. **You're a mercenary.** Every screen is about the work, the gear or the
   war, in that order.
2. **The war is always in view.** Show it happening; don't report it in a
   popup.
3. **Every screen earns its place.** No low-value panels.

## Foundations

- **Color:** light grey text on dark grey. Amber is the accent: what's
  yours, and what's selected. Blue, amber's complement, is crystal. On the
  map, factions keep their own colors.
- **Type:** Michroma, capitals, four sizes.
- **Space:** 8 px units, and plenty of them. No dense blobs of text.

## Components

Panel, bar, action (a menu's row), link, button.
