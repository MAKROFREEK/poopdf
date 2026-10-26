# Keyboard-first navigation with pointer-steered playhead

We deliberately split who moves what: keyboard navigation (vim-style) owns the view and the Paragraph Cursor only, and never repositions audio; the pointer owns the audio playhead (click = select + play there, or re-target when already playing). Playback itself is auto-flow: space toggles, audio advances paragraph by paragraph on its own. This honors both "playing that section instead and continuing from that spot" and "navigation keys never touch playback state".

**Consequences:** two highlight states share one system — Paragraph Cursor/hover gets an outline box, the spoken paragraph gets the filled Speech Highlight with the current word bold+underlined (word timing approximated from paragraph audio length). Auto-scroll follows the playhead unless the user scrolls away (Free Scroll); `z` or scrolling the paragraph back into view re-engages following.
