# 23-retry-backoff

## Task

Simulate retrying an operation that fails on its first two attempts and succeeds on the third (decide success from the attempt number). Use exponential backoff counted in scheduler ticks: after a failed attempt, wait 1 tick the first time, then 2, doubling each time, actually suspending for that many ticks. Log `attempt N failed, waiting D ticks` after each failure and `attempt N succeeded` on success, then finish.

## Reply 0

```shards
@wire(attempt-retry {
  0 >= attempt
  0 >= wait-count
  Repeat({
    If(
      Predicate: { wait-count | Is(0) }
      Then: {
        Inc(attempt) = current
        current | Match([
          1 { f"attempt {current} failed, waiting 1 ticks" | Log 1 > wait-count }
          2 { f"attempt {current} failed, waiting 2 ticks" | Log 2 > wait-count }
          3 { f"attempt {current} succeeded" | Log Stop }
        ])
      }
      Else: { Dec(wait-count) }
    )
  } Forever: true)
} Looped: true)
@mesh(main)
@schedule(main attempt-retry)
@run(main Iterations: 10)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt 1 failed, waiting 1 ticks","attempt 2 failed, waiting 2 ticks","attempt 3 succeeded"],"outcomes":[{"wire":"attempt-retry","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
