# 43-retry-block

## Task

Write a function `Retry` with an Int parameter `times` and a block parameter `action`. It runs `action` until a run succeeds, at most `times` times, and stops at the first success; if every run fails, `Retry` fails with the error of the last run. (`"x" | ParseInt` fails.)

Use it twice. First, with `times: 5`, an action that adds 1 to a counter starting at 0, logs it with the label `attempt`, and fails while the counter is below 3; log `done` after the call. Second, with a new counter starting at 0 and `times: 2`, an action that adds 1 to that counter, logs it with the label `attempt`, and always fails; the call must not stop the program: log the counter with the label `failed after` afterwards.

## Reply 0

```shards
@fn(Retry input: None output: None params: {times: Int action: Flow(input: None output: None)} {
  0 | Var(attempt)
  Repeat({
    Maybe(action: { Run(action) Return(none) } else: { Inc(attempt) } silent: true)
  } times: times)
  "x" | ParseInt
})

0 | Var(counter1)
Retry(times: 5 action: {
  Inc(counter1)
  counter1 | Log("attempt")
  counter1 | IsLess(3) | When({ "x" | ParseInt })
  none
})
"done" | Log

0 | Var(counter2)
Maybe(action: { Retry(times: 2 action: {
  Inc(counter2)
  counter2 | Log("attempt")
  "x" | ParseInt
  none
}) } silent: true)
counter2 | Log("failed after")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"missing-argument","message":"When: missing required parameter action","file":"solution.shs","line":13,"column":26,"shard":"When","param_index":1,"param":"action","path":[{"wire":"root"},{"shard":2,"name":"Retry"},{"param":"action"},{"shard":5,"name":"When"}]},{"phase":"construct","severity":"error","kind":"generic","code":"too-many-arguments","message":"Return takes at most 0 arguments, got 1","file":"solution.shs","line":4,"column":33,"shard":"Return","path":[{"function":"Retry"},{"shard":2,"name":"Repeat"},{"param":"action"},{"shard":0,"name":"Maybe"},{"param":"action"},{"shard":1,"name":"Return"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
@fn(Retry input: None output: None params: {times: Int action: Flow(input: None output: None)} {
  false | Var(succeeded)
  Repeat({
    Maybe(
      action: { Run(action) true | Update(succeeded) }
      else: { }
      silent: true
    )
  } times: times until: { succeeded })
  succeeded | If(
    predicate: { }
    then: { }
    else: { "x" | ParseInt }
  )
})

0 | Var(counter1)
Retry(times: 5 action: {
  Inc(counter1)
  counter1 | Log("attempt")
  counter1 | When(predicate: { IsLess(3) } action: { "x" | ParseInt })
  none
})
"done" | Log

0 | Var(counter2)
Maybe(action: { Retry(times: 2 action: {
  Inc(counter2)
  counter2 | Log("attempt")
  "x" | ParseInt
  none
}) } silent: true)
counter2 | Log("failed after")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"Retry declares output None but its body outputs Bool | Int","file":"solution.shs","line":1,"column":1,"shard":"Retry","actual":{"name":"Bool | Int","basic_type":-1},"expected":[{"name":"None","basic_type":0}],"path":[{"wire":"root"},{"shard":2,"name":"Retry"},{"function":"Retry"}]},{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"Retry declares output None but its body outputs Bool | Int","file":"solution.shs","line":1,"column":1,"shard":"Retry","actual":{"name":"Bool | Int","basic_type":-1},"expected":[{"name":"None","basic_type":0}],"path":[{"function":"Retry"}]}],"wires":[],"functions":[]}
```

## Reply 2

```shards
@fn(Retry input: None output: None params: {times: Int action: Flow(input: None output: None)} {
  false | Var(succeeded)
  Repeat({
    Maybe(
      action: { Run(action) true | Update(succeeded) }
      else: { }
      silent: true
    )
  } times: times until: { succeeded })
  succeeded | If(
    predicate: { }
    then: { none }
    else: { "x" | ParseInt }
  )
})

0 | Var(counter1)
Retry(times: 5 action: {
  Inc(counter1)
  counter1 | Log("attempt")
  counter1 | When(predicate: { IsLess(3) } action: { "x" | ParseInt })
  none
})
"done" | Log

0 | Var(counter2)
Maybe(action: { Retry(times: 2 action: {
  Inc(counter2)
  counter2 | Log("attempt")
  "x" | ParseInt
  none
}) } silent: true)
counter2 | Log("failed after")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"Retry declares output None but its body outputs Int | None","file":"solution.shs","line":1,"column":1,"shard":"Retry","actual":{"name":"Int | None","basic_type":-1},"expected":[{"name":"None","basic_type":0}],"path":[{"wire":"root"},{"shard":2,"name":"Retry"},{"function":"Retry"}]},{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"Retry declares output None but its body outputs Int | None","file":"solution.shs","line":1,"column":1,"shard":"Retry","actual":{"name":"Int | None","basic_type":-1},"expected":[{"name":"None","basic_type":0}],"path":[{"function":"Retry"}]}],"wires":[],"functions":[]}
```

## Reply 3

```shards
@fn(Retry input: None output: None params: {times: Int action: Flow(input: None output: None)} {
  false | Var(succeeded)
  Repeat({
    Maybe(
      action: { Run(action) true | Update(succeeded) }
      else: { }
      silent: true
    )
  } times: times until: { succeeded })
  succeeded | When(predicate: { Not } action: { "x" | ParseInt })
  none
})

0 | Var(counter1)
Retry(times: 5 action: {
  Inc(counter1)
  counter1 | Log("attempt")
  counter1 | When(predicate: { IsLess(3) } action: { "x" | ParseInt })
  none
})
"done" | Log

0 | Var(counter2)
Maybe(action: { Retry(times: 2 action: {
  Inc(counter2)
  counter2 | Log("attempt")
  "x" | ParseInt
  none
}) } silent: true)
counter2 | Log("failed after")
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt: 1","attempt: 2","attempt: 3","done","attempt: 1","attempt: 2","failed after: 2"],"outcomes":[{"wire":"root","outcome":"completed","value":"2"}],"spawned_failures":[]}
stderr:
```
