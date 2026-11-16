# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: tests/ui/specs/desk.spec.mjs >> an item opens on the right with its row lit in place; ‹ Dispatch goes back
- Location: tests/ui/specs/desk.spec.mjs:44:1

# Error details

```
Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
Call log:
  - navigating to "app.html?scenario=desk&latency=0&still=1", waiting until "load"

```

```
Error: page.evaluate: ReferenceError: CueBridge is not defined
    at eval (eval at evaluate (:303:30), <anonymous>:1:75)
    at UtilityScript.evaluate (<anonymous>:305:16)
    at UtilityScript.<anonymous> (<anonymous>:1:44)
```