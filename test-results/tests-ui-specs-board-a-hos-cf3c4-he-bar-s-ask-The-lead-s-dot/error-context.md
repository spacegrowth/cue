# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: tests/ui/specs/board.spec.mjs >> a hosted lead's crew on its own page >> a picked teammate on the left: its head line, what it was asked, steps, review found, files and three tabs; Talk, the bar's ask, The lead's dot
- Location: tests/ui/specs/board.spec.mjs:574:3

# Error details

```
Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
Call log:
  - navigating to "app.html?scenario=world-team&latency=0&still=1", waiting until "load"

```

```
Error: page.evaluate: ReferenceError: CueBridge is not defined
    at eval (eval at evaluate (:303:30), <anonymous>:1:75)
    at UtilityScript.evaluate (<anonymous>:305:16)
    at UtilityScript.<anonymous> (<anonymous>:1:44)
```