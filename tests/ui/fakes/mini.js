// Fakes for src-tauri/src/mini: the mini window's size and place, its keyboard, the chat it has open, and ⤢ In Cue.
// The main window never calls them; the mini window does (tests/ui/mini.html), and only sizes a window that isn't here.
CueBridge.area("mini", () => ({
  mini_shape: () => ({ left: false }),
  mini_key: () => null,
  mini_drag: () => ({ left: true }),
  mini_reading: () => null,
  mini_open_in_cue: () => null,
}));
