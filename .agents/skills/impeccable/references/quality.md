# UI quality requirements

Apply these requirements to the built result. The explicit user brief and established design system control the visual direction.

## Content and structure

Show the real task and meaningful state. Preserve supplied product facts. Mark simulated data. Do not invent claims, decorative metrics, or unrelated dashboard content.

Keep the primary action findable. Group related controls. Separate independent responsibilities. Use consistent spacing and restrained hierarchy. Avoid repeated branding, oversized selectors, and unnecessary cards.

Use real icons from the shared family. Do not substitute emoji or arbitrary glyphs. Use the bundled font and preserve its upstream license.

## Behavior

Check default, hover, focus, pressed, selected, expanded, disabled, pending, empty, error, and recovery states where applicable. A complete pointer press activates once. Cancellation and release outside do not activate.

Verify keyboard operation, visible focus, menu dismissal, and focus restoration. Keep full accessible names. Show useful recovery instructions instead of raw backend errors.

## Readability and adaptation

Normal text requires 4.5:1 contrast. Large text requires 3:1. Essential marks and focus indicators require 3:1. Measure actual composited surfaces.

Check actual English and Russian text. Include expanded text, RTL, text scaling, and the narrow supported viewport. Preserve meaningful values and avoid horizontal page overflow.

Use semantic tokens for colors, borders, depth, and motion. Keep shadows restrained. Do not add decorative gradients, glow, or glass without a surface requirement.

Keep text selection, caret, focus, and scrollbars consistent with the theme. Respect reduced motion and forced colors. Keep native dialog and system-menu differences explicit.
