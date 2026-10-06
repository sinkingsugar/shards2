# 26-spawn-workers

## Task

Define a worker process that receives an integer as its input and logs that integer multiplied by 10. From a main process, start three independent worker instances with inputs 1, 2 and 3, in that order. Run it on a scheduler.

## Reply 0

```shards
@wire(worker {
  Math.Multiply(10) | Log
})

@wire(main-wire {
  1 | Spawn(worker)
  2 | Spawn(worker)
  3 | Spawn(worker)
})

@mesh(main)
@schedule(main main-wire)
@run(main Iterations: 3)
```

## Run (pass)

```
exit Some(0)
stdout:
10
20
30
main-wire: 3
stderr:
```
