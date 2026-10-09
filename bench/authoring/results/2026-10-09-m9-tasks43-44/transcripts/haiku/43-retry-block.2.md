# 43-retry-block

## Task

Write a function `Retry` with an Int parameter `times` and a block parameter `action`. It runs `action` until a run succeeds, at most `times` times, and stops at the first success; if every run fails, `Retry` fails with the error of the last run. (`"x" | ParseInt` fails.)

Use it twice. First, with `times: 5`, an action that adds 1 to a counter starting at 0, logs it with the label `attempt`, and fails while the counter is below 3; log `done` after the call. Second, with a new counter starting at 0 and `times: 2`, an action that adds 1 to that counter, logs it with the label `attempt`, and always fails; the call must not stop the program: log the counter with the label `failed after` afterwards.

## Reply 0

```shards
@fn(Retry input: None output: None params: {times: Int action: Flow(input: None output: None)} {
  false | Var(succeeded)
  0 | Var(attempt)
  While({
    All(
      attempt | IsLess(times)
      succeeded | Not
    )
  } {
    Inc(attempt)
    If({
      attempt | IsLess(times)
    } then: {
      Maybe({
        Run(action)
        true | Update(succeeded)
      } silent: true)
    } else: {
      Run(action)
      true | Update(succeeded)
    })
  })
})

0 | Var(counter)
Retry(times: 5 action: {
  Inc(counter)
  counter | Log("attempt")
  If({ counter | IsLess(3) } then: { "x" | ParseInt })
})
"done" | Log

0 | Var(counter2)
Maybe({
  Retry(times: 2 action: {
    Inc(counter2)
    counter2 | Log("attempt")
    "x" | ParseInt
  })
})
counter2 | Log("failed after")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"Retry declares output None but its body outputs Int","file":"solution.shs","line":1,"column":1,"shard":"Retry","actual":{"name":"Int","basic_type":4},"expected":[{"name":"None","basic_type":0}],"path":[{"wire":"root"},{"shard":2,"name":"Retry"},{"function":"Retry"}]},{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"Retry declares output None but its body outputs Int","file":"solution.shs","line":1,"column":1,"shard":"Retry","actual":{"name":"Int","basic_type":4},"expected":[{"name":"None","basic_type":0}],"path":[{"function":"Retry"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
@fn(Retry input: None output: None params: {times: Int action: Flow(input: None output: None)} {
  false | Var(succeeded)
  0 | Var(attempt)
  While({
    All(
      attempt | IsLess(times)
      succeeded | Not
    )
  } {
    Inc(attempt)
    If({
      attempt | IsLess(times)
    } then: {
      Maybe({
        Run(action)
        true | Update(succeeded)
      } silent: true)
    } else: {
      Run(action)
      true | Update(succeeded)
    })
  })
  none
})

0 | Var(counter)
Retry(times: 5 action: {
  Inc(counter)
  counter | Log("attempt")
  If({ counter | IsLess(3) } then: { "x" | ParseInt })
})
"done" | Log

0 | Var(counter2)
Maybe({
  Retry(times: 2 action: {
    Inc(counter2)
    counter2 | Log("attempt")
    "x" | ParseInt
  })
})
counter2 | Log("failed after")
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt: 1","attempt: 2","attempt: 3","done","attempt: 1","attempt: 2","Maybe: activation error: ParseInt: \"x\" is not an Int in base 10","failed after: 2"],"outcomes":[{"wire":"root","outcome":"completed","value":"2"}],"spawned_failures":[]}
stderr:
```
