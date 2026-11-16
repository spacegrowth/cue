# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: tests/ui/specs/board.spec.mjs >> parking >> ↺ resumes a parked session and it's live again
- Location: tests/ui/specs/board.spec.mjs:20:3

# Error details

```
Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
Call log:
  - navigating to "app.html?scenario=parked-ended&latency=0&still=1", waiting until "load"

```

```
Error: page.evaluate: ReferenceError: CueBridge is not defined
    at eval (eval at evaluate (:303:30), <anonymous>:1:75)
    at UtilityScript.evaluate (<anonymous>:305:16)
    at UtilityScript.<anonymous> (<anonymous>:1:44)
```