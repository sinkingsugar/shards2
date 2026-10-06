# 26-spawn-workers

## Task

Define a worker process that receives an integer as its input and logs that integer multiplied by 10. From a main process, start three independent worker instances with inputs 1, 2 and 3, in that order. Run it on a scheduler.

## Reply 0

```shards
@wire(worker {
  Math.Multiply(10) | Log
})

@wire(launcher {
  1 | Spawn(worker)
  2 | Spawn(worker)
  3 | Spawn(worker)
})

@mesh(main)
@schedule(main launcher)
@run(main Iterations: 5)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["10","20","30"],"outcomes":[{"wire":"launcher","outcome":"completed","value":"3"}],"spawned_failures":[]}
stderr:
```
