# Independent Chrome Theme and Page Display Mode

"Dark mode" was ambiguous: inverting the whole display is the cheapest dark reading experience, but we realized the window UI and the PDF pixels have separate users of "dark". We decided the window chrome (Control Bar, dialogs, Empty State) has its own light/dark setting, and the page display has its own normal/inverted setting; each can also follow the system independently. `t` flips everything dark together; Preferences is where the settings are split.

**Consequences:** dark mode is achieved by pixel inversion of the rendered page bitmap (white text on black, images inverted too) — we deliberately do not try to re-render documents in a dark-friendly palette.
