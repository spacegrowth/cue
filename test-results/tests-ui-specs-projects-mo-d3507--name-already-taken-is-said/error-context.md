# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: tests/ui/specs/projects.spec.mjs >> moving a session to another project >> + New project… moves it to a new one and goes there; a name already taken is said
- Location: tests/ui/specs/projects.spec.mjs:46:3

# Error details

```
Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
Call log:
  - navigating to "app.html?scenario=board-waiting&latency=0&still=1", waiting until "load"

```

```
Error: page.evaluate: ReferenceError: CueBridge is not defined
    at eval (eval at evaluate (:303:30), <anonymous>:1:75)
    at UtilityScript.evaluate (<anonymous>:305:16)
    at UtilityScript.<anonymous> (<anonymous>:1:44)
```