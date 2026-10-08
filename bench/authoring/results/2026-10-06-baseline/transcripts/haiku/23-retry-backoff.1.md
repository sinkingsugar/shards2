# 23-retry-backoff

## Task

Simulate retrying an operation that fails on its first two attempts and succeeds on the third (decide success from the attempt number). Use exponential backoff counted in scheduler ticks: after a failed attempt, wait 1 tick the first time, then 2, doubling each time, actually suspending for that many ticks. Log `attempt N failed, waiting D ticks` after each failure and `attempt N succeeded` on success, then finish.

## Reply 0

```shards
@wire(retry-with-backoff {
  Once({
    0 >= attempt
    1 >= wait-time
  })
  
  Inc(attempt)
  
  If(
    Predicate: { attempt | IsLess(3) }
    Then: {
      f"attempt {attempt} failed, waiting {wait-time} ticks" | Log
      0 >= ticks-waited
      Repeat({
        Pause(0) | Inc(ticks-waited)
      } Until: { ticks-waited | IsMoreEqual(wait-time) })
      wait-time | Math.Multiply(2) > wait-time
    }
    Else: {
      f"attempt {attempt} succeeded" | Log
      Stop
    }
  )
} Looped: true)

@mesh(main)
@schedule(main retry-with-backoff)
@run(main Iterations: 20)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"wrong-argument-type","message":"Pause: Seconds must be Float, got Int","file":"solution.shs","line":15,"column":9,"shard":"Pause","actual":{"name":"Int","basic_type":4},"expected":[{"name":"Float","basic_type":10}],"param_index":0,"param":"Seconds","path":[{"wire":"retry-with-backoff"},{"shard":2,"name":"If"},{"param":"Then"},{"shard":5,"name":"Repeat"},{"param":"Action"},{"shard":0,"name":"Pause"}]}]}
```

## Reply 1

```shards
@wire(retry-with-backoff {
  Once({
    0 >= attempt
    1 >= wait-time
  })
  
  Inc(attempt)
  
  If(
    Predicate: { attempt | IsLess(3) }
    Then: {
      f"attempt {attempt} failed, waiting {wait-time} ticks" | Log
      0 >= ticks-waited
      Repeat({
        Pause(0.0) | Inc(ticks-waited)
      } Until: { ticks-waited | IsMoreEqual(wait-time) })
      wait-time | Math.Multiply(2) > wait-time
    }
    Else: {
      f"attempt {attempt} succeeded" | Log
      Stop
    }
  )
} Looped: true)

@mesh(main)
@schedule(main retry-with-backoff)
@run(main Iterations: 20)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt 1 failed, waiting 1 ticks","attempt 2 failed, waiting 2 ticks","attempt 3 succeeded"],"outcomes":[{"wire":"retry-with-backoff","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
