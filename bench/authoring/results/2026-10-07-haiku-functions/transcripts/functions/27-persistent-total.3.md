# 27-persistent-total

## Task

Write a looped process, run on a scheduler for exactly 3 iterations (one per tick). It keeps a running total that starts at 100 and survives across iterations. Each iteration adds 5 to the total and logs it.

## Reply 0

```shards
@wire(total-keeper {
  Keep(n 100)
  n | Math.Add(5) | Update(n) | Log
} looped: true)

@mesh(main)
@schedule(main total-keeper)
@run(main iterations: 3)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["105","110","115"],"outcomes":[{"wire":"total-keeper","outcome":"running"}],"spawned_failures":[]}
stderr:
```
