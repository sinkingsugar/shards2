# 23-retry-backoff

## Task

Simulate retrying an operation that fails on its first two attempts and succeeds on the third (decide success from the attempt number). Use exponential backoff counted in scheduler ticks: after a failed attempt, wait 1 tick the first time, then 2, doubling each time, actually suspending for that many ticks. Log `attempt N failed, waiting D ticks` after each failure and `attempt N succeeded` on success, then finish.

## Reply 0

```shards
@wire(retry {
  1 >= attempt
  1 >= delay
  false >= done
  Repeat({
    If({attempt | IsLess(3)} {
      f"attempt {attempt} failed, waiting {delay} ticks" | Log
      Repeat({Pause} Times: delay)
      delay | Math.Multiply(2) > delay
      Inc(attempt)
    } {
      f"attempt {attempt} succeeded" | Log
      true > done
    })
  } Until: {done})
})
@mesh(main)
@schedule(main retry)
@run(main)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt 1 failed, waiting 1 ticks","attempt 2 failed, waiting 2 ticks","attempt 3 succeeded"],"outcomes":[{"wire":"retry","outcome":"completed","value":"false"}],"spawned_failures":[]}
stderr:
```
