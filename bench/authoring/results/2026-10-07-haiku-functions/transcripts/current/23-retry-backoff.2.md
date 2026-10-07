# 23-retry-backoff

## Task

Simulate retrying an operation that fails on its first two attempts and succeeds on the third (decide success from the attempt number). Use exponential backoff counted in scheduler ticks: after a failed attempt, wait 1 tick the first time, then 2, doubling each time, actually suspending for that many ticks. Log `attempt N failed, waiting D ticks` after each failure and `attempt N succeeded` on success, then finish.

## Reply 0

```shards
@wire(retry {
  0 >= attempt
  1 >= wait-time
  
  Repeat({
    Inc(attempt) | 
    If(IsAny([1 2])
      Then: {
        f"attempt {attempt} failed, waiting {wait-time} ticks" | Log
        Repeat({
          Pause
        } Times: wait-time)
        wait-time | Math.Multiply(2) > wait-time
      }
      Else: {
        f"attempt {attempt} succeeded" | Log
        Stop
      }
    )
  } Forever: true)
} Looped: true)

@mesh(main)
@schedule(main retry)
@run(main)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt 1 failed, waiting 1 ticks","attempt 2 failed, waiting 2 ticks","attempt 3 succeeded"],"outcomes":[{"wire":"retry","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
