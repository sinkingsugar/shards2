# 27-persistent-total

## Task

Write a looped process, run on a scheduler for exactly 3 iterations (one per tick). It keeps a running total that starts at 100 and survives across iterations. Each iteration adds 5 to the total and logs it.

## Reply 0

```shards
@wire(counter {
  Once({ 100 >= total })
  total | Add(5) | Update(total) | Log
} Looped: true)

@mesh(main)
@schedule(main counter)
@run(main Iterations: 3)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["105","110","115"],"outcomes":[{"wire":"counter","outcome":"running"}],"spawned_failures":[]}
stderr:
```
