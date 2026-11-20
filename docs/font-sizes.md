# Font sizes: which scale to pick

**Pick B, whole pixels with one size per job.** It's the best one for people using Cue. Below is why, and what the other two would mean.

## What's wrong today

Cue's screens use 20 different text sizes, from 9 to 34 pixels, set in 941 places. Eleven of those sizes aren't in the design system at all. The odd one used most is 13 pixels, which turns up 96 times.

The same kind of text gets a different size on each screen. A row title is 13 pixels in the notch and in Settings, 13 and a half on the Desk, and 14 in the Inbox. Page headings are 17, 19 or 20. Most sizes sit only half a pixel apart, so two sizes that are meant to differ look like one size that's slightly wrong.

## The three options

**A, snap to the tokens.** Every odd size moves to the nearest size the design system already has, which leaves ten sizes. Screens look almost the same as today. It's the smallest change, but the half-pixel steps stay (11, 11 and a half, 12, 12 and a half), so it's tidier rather than simpler. People can't tell those sizes apart, so the busy feeling stays.

**B, whole pixels, one size per job.** Seven sizes, each with one clear job:
- 10 for tags
- 11 for small headings and details
- 12 for buttons
- 13 for all body text and names
- 15 for titles
- 20 for page headings
- 32 for big numbers

Names stop getting their own extra half pixel; bold text sets them apart instead.

**C, five sizes, with weight doing the rest.** Only five sizes. Capitals, monospace and bold tell a tag from a detail line. It's the calmest look, but small labels grow from 10 to 11, so the busy rows on the Desk and in the notch get wider and some may wrap or get cut off. Page headings also lose most of their size jump.

## Why B is the one for users

- **It's consistent.** The same kind of text is the same size on every screen, so people learn the look once and read faster.
- **The steps are clear.** Every step is at least one whole pixel, so each size reads as meant.
- **Nothing gets squeezed.** Busy rows stay the width they are today, unlike option C.
- **Later changes are cheap.** Each job becomes one named setting, so changing it later is a single edit.

## What B costs

A medium amount of work. Almost every size in the app changes, about 940 places, and every screen needs a look afterwards to check that nothing wraps or moves. Expect a day or two.
