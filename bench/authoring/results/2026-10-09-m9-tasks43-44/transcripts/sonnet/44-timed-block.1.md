# 44-timed-block

## Task

Write a function `Timed` with a block parameter `action`. It runs `action` once and outputs, as a Float, how many seconds that took (`Time.Now` outputs a time in seconds).

Use it on an action that adds 1 to a counter starting at 0 and then pauses for 0.05 seconds, repeated 3 times inside the action. Log whether the measured time is at least 0.15 seconds (a Bool), then log the counter.

## Reply 0

```shards
@fn(Timed input: None output: Float params: {action: Flow(input: None)} {
  Time.Now | Var(start)
  Run(action)
  Time.Now | Math.Subtract(start)
})

0 | Var(counter)
Timed(action: { Repeat({ Inc(counter) Pause(0.05) } times: 3) }) | Var(elapsed)
elapsed | IsMoreEqual(0.15) | Log
counter | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["true","3"],"outcomes":[{"wire":"root","outcome":"completed","value":"3"}],"spawned_failures":[]}
stderr:
```
