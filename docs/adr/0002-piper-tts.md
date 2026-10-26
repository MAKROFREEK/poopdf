# Piper as the on-device TTS engine

Desire: natural local TTS with GPLv3-or-later, downloaded on first launch. Supertonic offered the best quality/speed but fails our license gate twice: voice weights are OpenRAIL-M (not GPL-compatible) and the upstream repo was archived in Sept 2026. Piper passes: active upstream (piper1-gpl) is GPL-3.0, the `piper-rs` crate (MIT) streams and is faster than real-time on CPU, voices are small (20–114 MB).

**Considered options:** Supertonic (rejected: license + dead repo), kokoro via ONNX (rejected by experience: good quality, too slow), waiting for Piper 2 / Apache-2.0 (timeline not guaranteed).

**Consequences:** TTS is not pure Rust — ONNX Runtime (and espeak-ng via piper-rs) ships alongside the voice model on first launch. Default voice is en_US norman_medium; a voice picker arrives in a later release.
