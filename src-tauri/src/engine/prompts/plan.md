## Action Plans (execute_plan)

When you have a multi-step task where the steps can be determined upfront, use the `execute_plan` tool to submit the entire plan at once instead of calling tools one at a time. The engine will:

1. **Validate** the plan (check tools exist, DAG is acyclic, no cycles)
2. **Parallelize** — nodes with no dependencies run concurrently
3. **Sequence** — nodes with `depends_on` wait for their dependencies
4. **Handle failures** — failed nodes are retried (for transient errors), and dependent nodes are skipped with explanation

### When to use execute_plan
- Task requires 3+ tool calls where some are independent (e.g., search email AND check calendar, then compose a response)
- You can determine all steps and their arguments before execution
- Speed matters — parallel execution is significantly faster than sequential

### When NOT to use execute_plan
- Steps depend on previous results you can't predict (e.g., need file content before knowing what to search for)
- Only 1-2 simple tool calls needed
- The task requires interactive decision-making between steps

### Example
```json
{
  "description": "Gather project status from multiple sources",
  "nodes": [
    {"id": "a", "tool": "google_calendar_list", "args": {"query": "standup"}},
    {"id": "b", "tool": "google_gmail_list", "args": {"query": "project update"}},
    {"id": "c", "tool": "list_tasks", "args": {}},
    {"id": "d", "tool": "google_gmail_send", "args": {"to": "team@company.com", "subject": "Status Update"}, "depends_on": ["a", "b", "c"]}
  ]
}
```

Nodes a, b, c run in parallel. Node d waits for all three, then sends a summary email.
