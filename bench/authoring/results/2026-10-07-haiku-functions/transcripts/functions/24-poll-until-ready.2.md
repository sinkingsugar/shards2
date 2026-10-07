# 24-poll-until-ready

## Task

Write a looped process (one that repeats every scheduler tick) that polls a simulated sensor. The reading starts at 0 and increases by 1 on every poll. On each poll, log `waiting` while the reading is below 3; when the reading reaches 3, log `ready` and stop the process. The process must yield to the scheduler between polls. Run it on a scheduler.

## Reply 0

```shards
@wire(sensor-wire {
  Keep(reading 0)
  Inc(reading) | If({ IsLess(3) } { "waiting" | Log } { "ready" | Log Stop })
} looped: true)

@mesh(main)
@schedule(main sensor-wire)
@run(main)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["waiting","waiting","ready"],"outcomes":[{"wire":"sensor-wire","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
