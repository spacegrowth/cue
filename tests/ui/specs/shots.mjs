// Screenshots against their baselines: each shot is saved to tests/ui/shots/<name>.png and compared pixel by
// pixel (pixelmatch) with tests/ui/baseline/<name>.png (or shots/ and baseline/ under another `root`). A shot that differs saves <name>.diff.png beside it,
// the changed pixels in red. CUE_UI_BASELINE=1 writes the baselines instead: only after looking at every shot.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

const dir = (d, root) => { const u = new URL(`${d}/`, root); mkdirSync(u, { recursive: true }); return u; };
/** How far apart two pixels' colors may be and still count as the same (pixelmatch's 0–1 scale). */
const COLOR_THRESHOLD = 0.1;

/** Saves `png` (a screenshot's bytes) as shot `name` and returns how many pixels differ from its baseline. `root`:
 *  the folder (a file: URL ending in /) its shots/ and baseline/ are in; tests/ui by default. */
export function compareShot(name, png, root = new URL("../", import.meta.url)) {
  const shot = new URL(`${name}.png`, dir("shots", root)), base = new URL(`${name}.png`, dir("baseline", root));
  writeFileSync(shot, png);
  if (process.env.CUE_UI_BASELINE === "1") { writeFileSync(base, png); return { differ: 0, baseline: "written" }; }
  if (!existsSync(base)) return { differ: -1, baseline: "missing" };
  const a = PNG.sync.read(png), b = PNG.sync.read(readFileSync(base));
  if (a.width !== b.width || a.height !== b.height) return { differ: a.width * a.height, baseline: `is ${b.width}×${b.height}, the shot ${a.width}×${a.height}` };
  const diff = new PNG({ width: a.width, height: a.height });
  const differ = pixelmatch(a.data, b.data, diff.data, a.width, a.height, { threshold: COLOR_THRESHOLD });
  if (differ) writeFileSync(new URL(`${name}.diff.png`, dir("shots", root)), PNG.sync.write(diff));
  return { differ, baseline: "compared" };
}
