// Fakes for src-tauri/src/mini: the mini window's size and place, its keyboard, and ⤢ In Cue. The pages here are
// the main window, which never calls them; mini.html would, and only sizes a window that isn't here.
CueBridge.area("mini", () => ({
  mini_shape: () => null,
  mini_key: () => null,
  mini_drag: () => null,
  mini_open_in_cue: () => null,
}));
