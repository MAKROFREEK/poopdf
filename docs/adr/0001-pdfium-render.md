# pdfium-render as the PDF rendering and text-extraction backbone

We needed page rasterization plus per-word text boxes (hover highlights, read-along sync). No maintained pure-Rust crate offers both; MuPDF's Rust wrapper is AGPL which complicates distribution for a GPLv3-only-minimum project. We chose `pdfium-render` (MIT/Apache bindings) over Google's BSD-licensed pdfium, accepting one native shared library downloaded on first launch — the same pattern the TTS engine already required.

**Considered options:** mupdf-rs (AGPL, rejected for licensing friction), pure-Rust text-only rendering via pdf-extract (rejected: degrades page layout badly), hayro (pure Rust rasterizer, promising but lacks a finished text-extraction API at decision time).

**Consequences:** every platform needs an injected pdfium dynamic library at runtime; auto-download with pinned release (bblanchon/pdfium-binaries) is part of the app's startup contract.
