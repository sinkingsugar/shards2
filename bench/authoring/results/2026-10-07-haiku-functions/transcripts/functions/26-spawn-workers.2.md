# 26-spawn-workers

## Task

Define a worker process that receives an integer as its input and logs that integer multiplied by 10. From a main process, start three independent worker instances with inputs 1, 2 and 3, in that order. Run it on a scheduler.

## Reply 0

```shards
@wire(worker {
  Math.Multiply(10) | Log
})

@wire(main {
  Keep(index 0)
  If({index | IsLess(3)} {
    [1 2 3] | Take(index) | Spawn(worker)
    Inc(index)
  } {
    Stop
  })
} looped: true)

@mesh(scheduler)
@schedule(scheduler main)
@run(scheduler)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["10","20","30"],"outcomes":[{"wire":"main","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
