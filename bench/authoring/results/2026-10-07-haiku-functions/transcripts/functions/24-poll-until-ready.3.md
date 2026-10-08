# 24-poll-until-ready

## Task

Write a looped process (one that repeats every scheduler tick) that polls a simulated sensor. The reading starts at 0 and increases by 1 on every poll. On each poll, log `waiting` while the reading is below 3; when the reading reaches 3, log `ready` and stop the process. The process must yield to the scheduler between polls. Run it on a scheduler.

## Reply 0

```shards
@wire(sensor-poll {
  Keep(reading 0)
  Inc(reading)
  If(predicate: { reading | IsLess(3) }
     then: { Log("waiting") }
     else: { Log("ready") Stop })
  Pause
} looped: true)

@mesh(main)
@schedule(main sensor-poll)
@run(main)
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["waiting: 1","waiting: 2","ready: 3"],"outcomes":[{"wire":"sensor-poll","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
