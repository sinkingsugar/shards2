# 39-recursive-tree-fold

## Task

Represent a tree by parallel sequences of node values [5 -2 7 4 3 -1] and child-index lists [[1 2] [3 4] [5] [] [] []]. Define a recursive named function that takes a node index and these sequences as parameters and returns the sum of that node and all its descendants. Do not hardcode subtree totals or replace recursion with an explicit traversal stack. Call the same function for nodes 0, 1, 2 and 5 and log each result. Each invocation must keep its own partial sum while it visits its children.

## Reply 0

```shards
@fn(SubtreeSum input: None output: Int params: {index: Int values: [Int] children: [[Int]]} {
  values | Take(index) = current-value
  current-value | Var(sum)
  children | Take(index) = my-children
  my-children | Count = child-count
  0 | Var(i)
  Repeat({
    my-children | Take(i) = child-index
    SubtreeSum(index: child-index values: values children: children) | Add(sum) | Update(sum)
    Inc(i)
  } times: child-count)
  sum
})

values = [5 -2 7 4 3 -1]
children = [[1 2] [3 4] [5] [] [] []]

SubtreeSum(index: 0 values: values children: children) | Log("Node 0")
SubtreeSum(index: 1 values: values children: children) | Log("Node 1")
SubtreeSum(index: 2 values: values children: children) | Log("Node 2")
SubtreeSum(index: 5 values: values children: children) | Log("Node 5")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"parse","severity":"error","kind":"syntax","code":"expected-variable","message":"`=` must be followed by a variable name, found `[`; `value = x` binds the immutable name x to the value","file":"solution.shs","line":15,"column":10},{"phase":"parse","severity":"error","kind":"syntax","code":"expected-variable","message":"`=` must be followed by a variable name, found `[`; `value = x` binds the immutable name x to the value","file":"solution.shs","line":16,"column":12}],"wires":[],"functions":[]}
```

## Reply 1

```shards
@fn(SubtreeSum input: None output: Int params: {index: Int values: [Int] children: [[Int]]} {
  values | Take(index) = current-value
  current-value | Var(sum)
  children | Take(index) = my-children
  my-children | Count = child-count
  0 | Var(i)
  Repeat({
    my-children | Take(i) = child-index
    SubtreeSum(index: child-index values: values children: children) | Add(sum) | Update(sum)
    Inc(i)
  } times: child-count)
  sum
})

[5 -2 7 4 3 -1] = values
[[1 2] [3 4] [5] [] [] []] = children

SubtreeSum(index: 0 values: values children: children) | Log("Node 0")
SubtreeSum(index: 1 values: values children: children) | Log("Node 1")
SubtreeSum(index: 2 values: values children: children) | Log("Node 2")
SubtreeSum(index: 5 values: values children: children) | Log("Node 5")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"wrong-variable-type","message":"SubtreeSum: children must be [[Int]], got [([Any] | [Int])]","file":"solution.shs","line":18,"column":1,"shard":"SubtreeSum","actual":{"name":"[([Any] | [Int])]","basic_type":56},"expected":[{"name":"[[Int]]","basic_type":56}],"param_index":2,"param":"children","path":[{"wire":"root"},{"shard":4,"name":"SubtreeSum"}]}],"wires":[],"functions":[{"signature":{"name":"SubtreeSum","revision":1,"input":{"kind":"type","type":"None"},"output":{"kind":"type","type":"Int"},"params":[{"name":"index","type":"Int","forms":["literal","variable"],"requirement":"required","help":""},{"name":"values","type":"[Int]","forms":["literal","variable"],"requirement":"required","help":""},{"name":"children","type":"[[Int]]","forms":["literal","variable"],"requirement":"required","help":""}],"lifetime":"stateless","effects":{"suspends":false,"io":false,"time":false,"random":false,"unknown":false},"uses":[],"mutates":[],"source":null,"summary":"","help":""},"line":1,"column":1}]}
```

## Reply 2

```shards
@fn(SubtreeSum input: None output: Int params: {index: Int values: [Int] children: [Any]} {
  values | Take(index) = current-value
  current-value | Var(sum)
  children | Take(index) | ExpectSeq = my-children
  my-children | Count = child-count
  0 | Var(i)
  Repeat({
    my-children | Take(i) | ExpectInt = child-index
    SubtreeSum(index: child-index values: values children: children) | Add(sum) | Update(sum)
    Inc(i)
  } times: child-count)
  sum
})

[5 -2 7 4 3 -1] = values
[[1 2] [3 4] [5] [] [] []] = children

SubtreeSum(index: 0 values: values children: children) | Log("Node 0")
SubtreeSum(index: 1 values: values children: children) | Log("Node 1")
SubtreeSum(index: 2 values: values children: children) | Log("Node 2")
SubtreeSum(index: 5 values: values children: children) | Log("Node 5")
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["Node 0: 16","Node 1: 5","Node 2: 6","Node 5: -1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
