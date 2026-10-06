# 27-persistent-total

## Task

Write a looped process, run on a scheduler for exactly 3 iterations (one per tick). It keeps a running total that starts at 100 and survives across iterations. Each iteration adds 5 to the total and logs it.

## Reply 0

```shards
@wire(counter {
  Once({100 >= total})
  total | Math.Add(5) > total | Log
  Pause
} Looped: true)
@mesh(main)
@schedule(main counter)
@run(main Iterations: 3)
```

## Run (wrong-output)

```
exit Some(0)
stdout:
105
110
counter: still running after 3 ticks
stderr:
```
