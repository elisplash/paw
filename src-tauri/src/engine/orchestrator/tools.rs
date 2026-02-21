// Paw Agent Engine — Orchestrator Tool Definitions
//
// Boss and worker agents each get a distinct set of orchestrator-specific tools.
// Boss: delegate_task, check_agent_status, send_agent_message, project_complete, create_sub_agent
// Worker: report_progress

use crate::engine::types::*;

/// Orchestrator-specific tools that only the boss agent gets.
pub fn boss_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "delegate_task".into(),
                description: "Delegate a sub-task to a specialized sub-agent on this project. The sub-agent will work on the task and report back.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "The agent_id of the sub-agent to delegate to (must be assigned to this project)"
                        },
                        "task_description": {
                            "type": "string",
                            "description": "Clear, specific description of what the sub-agent should do"
                        },
                        "context": {
                            "type": "string",
                            "description": "Additional context, requirements, or constraints for the sub-task"
                        }
                    },
                    "required": ["agent_id", "task_description"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "check_agent_status".into(),
                description: "Check the current status and progress of all sub-agents on this project.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "send_agent_message".into(),
                description: "Send a message to a specific sub-agent or broadcast to all agents on this project.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to_agent": {
                            "type": "string",
                            "description": "The agent_id to send to, or 'all' for broadcast"
                        },
                        "message": {
                            "type": "string",
                            "description": "The message content"
                        }
                    },
                    "required": ["to_agent", "message"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "project_complete".into(),
                description: "Mark the project as completed with a final summary. Call this when all sub-tasks are done and the project goal has been achieved.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "summary": {
                            "type": "string",
                            "description": "Final summary of what was accomplished"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["completed", "failed"],
                            "description": "Final project status"
                        }
                    },
                    "required": ["summary", "status"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "create_sub_agent".into(),
                description: "Create and register a new sub-agent in the current project. The agent will be added to the database and available for task delegation immediately.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "A unique name/id for the agent (e.g. 'code-cat', 'research-owl'). Use lowercase with hyphens."
                        },
                        "role": {
                            "type": "string",
                            "enum": ["worker", "boss"],
                            "description": "The agent's role. Usually 'worker' for sub-agents."
                        },
                        "specialty": {
                            "type": "string",
                            "enum": ["coder", "researcher", "designer", "communicator", "security", "general"],
                            "description": "The agent's area of expertise"
                        },
                        "system_prompt": {
                            "type": "string",
                            "description": "Custom system prompt / personality instructions for this agent"
                        },
                        "capabilities": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of tool names this agent should have access to (e.g. ['exec', 'fetch', 'web_search']). Leave empty for all default tools."
                        },
                        "model": {
                            "type": "string",
                            "description": "Optional model override for this agent (e.g. 'gemini-2.5-flash'). Leave empty to use project defaults."
                        }
                    },
                    "required": ["name", "role", "specialty", "system_prompt"]
                }),
            },
        },
    ]
}

/// Worker-specific tools that sub-agents get.
pub fn worker_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "report_progress".into(),
                description: "Report your progress back to the boss agent. Call this when you have updates, results, or encounter issues.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["working", "done", "error", "blocked"],
                            "description": "Current status of your work"
                        },
                        "message": {
                            "type": "string",
                            "description": "Description of progress, results, or issues"
                        },
                        "output": {
                            "type": "string",
                            "description": "Any output or deliverables from your work"
                        }
                    },
                    "required": ["status", "message"]
                }),
            },
        },
    ]
}
