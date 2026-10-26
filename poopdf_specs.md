# 1st desire: is to open and view pdfs.
# 2nd desire: is to read aloud the relevant parts of said pdf.
# 3rd desire: is Extremely simple UI.
# 4th desire: is keyboard style navigation. (vim/arrow keys)
# 5th desire: uses piper or supertonic for local natural TTS. Comes with one installed (downloads on launch of application).
# 6th desire: light & dark mode, egui opengl, so css and all that fun stuff. nothing too complicated just a dark and light mode, that changes with sytem or manually. Don't over complicate it if the easiest method is to invert the whole display of the pdf to get it white text on black background, do that. images be damned if theyre inverted too. 
# 7th desire: GPL v3 or equivalent.
# 8th desire: we're basically just making free and open source software, local first, pdf viewer, with audio play back for a pseudo audio book experience. if the pdf is all images, it doesn't play audio. plain and simple.
# 9th desire: cross platform. 
# 10th desire: pure rust
# 11th desire: performance is high priority.
# 12th desire: hovering paragraph highlights a box around the text
# 13th desire: the hovering is linked to the audio play back for the paragraph/word that the program is currently on.

<!-- The original desire list is preserved above. The settled spec lives below; vocabulary is defined in GLOSSARY.md, key architecture decisions in docs/adr/. -->

# poopdf — settled spec (post grilling)

A local-first, GPLv3-or-later, keyboard-driven PDF viewer that reads documents aloud with on-device speech (pseudo audiobook). Pure Rust app code; vanilla egui over OpenGL; cross-platform; performance-conscious.

## Stack (details in ADRs)

- PDF: `pdfium-render` — page bitmaps + per-word extraction; pdfium shared library downloads on first launch, pinned (bblanchon/pdfium-binaries).
- TTS: `piper-rs` (upstream piper1-gpl, GPL-3). ONNX Runtime + voice download on first launch. Default voice: en_US norman_medium.
- Audio: `rodio` (WAV/PCM out from piper).
- GUI: egui (OpenGL), dark chrome via egui Style/Visuals (no CSS).

## Reading & audio model

- Speech unit: Paragraph, in default Reading Order (multi-column heuristics: later).
- Mixed PDFs: speak text, skip image-only regions silently. Silent Documents (zero text): notice, no audio.
- Playback is auto-flow: space toggles; audio flows paragraph by paragraph. Keyboard navigation never repositions audio.
- Pointer steers audio: click = select + play there (re-targets if already playing).
- Pause/resume: resume replays the current paragraph from its start.
- Speed 0.5×–3× via engine resynthesis (piper speed), `,`/`.` keys + Control Bar cycle control.

## Highlights & linking

- Word-level sync approximated: Speech Highlight fills the spoken paragraph; current word rendered bold + underline.
- Hover: outline box around the paragraph under the pointer (desire 12); Speech Highlight and hover share one system, distinct visual states (desire 13).
- Auto-scroll follows the Speech Highlight. Scrolling away detaches (Free Scroll); `z` (or scrolling the paragraph back into view) re-follows.

## UI

- Single Control Bar (top strip): File | Preferences menus, play/pause, ±10s skip, speed, elapsed/total time, page indicator (visual reference: screenshot-reference.png, docked not floating).
- Preferences dialog: Session Resume toggle (default on), Chrome Theme (system/light/dark), Page Display Mode (system/normal/inverted), default speed, keybind editor (simple rows: click to capture next keypress, duplicates rejected). Voice dropdown arrives in a later release.
- Empty State: keyboard-navigable Recent Documents list (last 5) + drag-and-drop + `o` native picker + CLI argument (`poopdf file.pdf`).
- Performance: LRU page-bitmap cache at current zoom; stale bitmaps dropped on zoom change. Highlight boxes rendered as egui shape overlays over the cached page texture — no per-frame re-rasterization.

## Keys (all remappable in the Preferences keybind editor)

| Key | Action |
| --- | --- |
| `o` | open file (native picker) |
| `j` / `k`, `↓` / `↑` | paragraph cursor down / up |
| `g` / `G` | jump to top / bottom |
| `Ctrl+d` / `Ctrl+u` | half-page scroll down / up |
| `+` / `−` | zoom in / out |
| `0` | reset zoom |
| `Ctrl+wheel` | zoom |
| `h` / `l` | skip 10s back / forward |
| `space` | play / pause |
| `,` / `.` | speed down / up |
| `t` | flip chrome + page dark together |
| `z` | re-follow speech highlight |
| `f` | fullscreen toggle |
| `q` | quit |
| `Esc` | clear selection |

Mouse wheel: plain scrolling; never touches playback state.

## Theming

- Chrome Theme and Page Display Mode are independent (ADR-0003). Each can follow the system or be pinned light/dark (chrome) / normal/inverted (page).
- `t` flips everything dark together; Preferences splits them.
- Page dark = invert the rendered page bitmap (white text on black; images invert too).

## License & delivery

- GPL-3.0-or-later; LICENSE + headers on source files from the first commit.
- CI-built binaries on GitHub Releases (Linux/macOS/Windows); no code signing at MVP; ORT + voice + pdfium download on first launch.

## Build order

1. Vertical slice: one PDF → pdfium page bitmap + word boxes → one paragraph → piper → rodio → moving highlight. De-risk the core first.
2. Viewer polish: zoom, theme split, hover boxes, page cache.
3. Playback polish: speed resynthesis, skip, auto-scroll/re-follow, resume, recents.
4. Preferences: theme split, resume toggle, keybind editor.
5. Later: voice dropdown, multi-column reading heuristics, floating playbar toggle (maybe).
