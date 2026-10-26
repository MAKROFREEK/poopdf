# poopdf

A local-first, keyboard-driven PDF viewer that reads documents aloud with on-device speech. GPL v3 software; a document that cannot be spoken is simply viewed.

## Language

**Document**:
A PDF file opened in the viewer for reading or listening.
_Avoid_: book, file, pdf (when referring to the opened object)

**Paragraph**:
A contiguous run of document text treated as one reading unit, positioned in page coordinates. The base unit for highlighting, the cursor, and speech.
_Avoid_: text block, chunk, section

**Word Box**:
The position and size of one word inside a Paragraph, reported by the extraction layer. What makes word-level highlight and hover boxes possible.
_Avoid_: token, rect

**Reading Order**:
The linear sequence of Paragraphs in a Document that speech follows across pages.
_Avoid_: flow, text order

**Paragraph Cursor**:
The currently selected Paragraph, moved by keyboard navigation. Scrolling and selection only — never moves spoken audio.
_Avoid_: caret, pointer, focus

**Hover Highlight**:
The box shown around the Paragraph under the mouse pointer.
_Avoid_: selection box

**Speech Highlight**:
The visual marker synced to the audio, riding the Paragraph and Word Box currently being spoken.
_Avoid_: read-along highlight, karaoke highlight

**Silent Document**:
A Document with no extractable text (e.g. scanned images); viewable but never spoken, with the viewer saying so.
_Avoid_: image pdf, broken pdf

**Chrome Theme**:
The light-or-dark appearance of the window's own UI (Control Bar, dialogs, Empty State), set independently of the Page Display Mode.
_Avoid_: app dark mode, css

**Page Display Mode**:
How the Document's rendered pixels are shown: normal colors, or inverted (white text on black, images inverted too) — chosen independently of the Chrome Theme.
_Avoid_: inverted dark mode, night mode, pdf dark mode

**TTS Engine**:
The speech synthesizer downloaded and installed on first launch, running fully on-device.
_Avoid_: voice pack (means the model, not the engine)

**Voice Model**:
A downloadable voice the TTS Engine speaks with, plus its config.
_Avoid_: language pack, model file

**Session Resume**:
Restoring the last opened Document, position, and listening state from persistent settings, when the user's resume toggle is on.
_Avoid_: autosave, bookmark

**Control Bar**:
The single top strip of the window holding the menus (File | Preferences), playback controls, and the page indicator. The only pointer control surface.
_Avoid_: toolbar, playbar, head bar

**Recent Documents**:
The short list (five) of previously opened Documents shown in the Empty State and navigable by keyboard.
_Avoid_: history, recents sidebar

**Empty State**:
The screen shown when no Document is open: the Recent Documents list plus the open hint.
_Avoid_: home screen, welcome screen

**Re-follow**:
Restoring auto-scroll so the view tracks the Speech Highlight again (z, or scrolling the spoken Paragraph back into view).
_Avoid_: sync, goto

**Free Scroll**:
The detached condition after the user scrolls away from the Speech Highlight; auto-follow is suspended until Re-follow.
_Avoid_: lost, untracked
