// commands/squad.rs — Squad CRUD Tauri commands.

use crate::commands::state::EngineState;
use crate::engine::types::{Squad, SquadMember};
use log::info;
use tauri::State;

#[tauri::command]
pub fn engine_squads_list(
    state: State<'_, EngineState>,
) -> Result<Vec<Squad>, String> {
    state.store.list_squads().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_squad_create(
    state: State<'_, EngineState>,
    squad: Squad,
) -> Result<(), String> {
    info!("[engine] Creating squad: {} ({})", squad.name, squad.id);
    state.store.create_squad(&squad).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_squad_update(
    state: State<'_, EngineState>,
    squad: Squad,
) -> Result<(), String> {
    info!("[engine] Updating squad: {}", squad.id);
    state.store.update_squad(&squad).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_squad_delete(
    state: State<'_, EngineState>,
    squad_id: String,
) -> Result<(), String> {
    info!("[engine] Deleting squad: {}", squad_id);
    state.store.delete_squad(&squad_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_squad_add_member(
    state: State<'_, EngineState>,
    squad_id: String,
    member: SquadMember,
) -> Result<(), String> {
    info!("[engine] Adding {} to squad {}", member.agent_id, squad_id);
    state.store.add_squad_member(&squad_id, &member).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_squad_remove_member(
    state: State<'_, EngineState>,
    squad_id: String,
    agent_id: String,
) -> Result<(), String> {
    info!("[engine] Removing {} from squad {}", agent_id, squad_id);
    state.store.remove_squad_member(&squad_id, &agent_id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use crate::engine::types::{Squad, SquadMember};

    fn test_squad() -> Squad {
        Squad {
            id: "sq-test".into(),
            name: "Test Squad".into(),
            goal: "Unit test coverage".into(),
            status: "active".into(),
            members: vec![
                SquadMember { agent_id: "a1".into(), role: "coordinator".into() },
            ],
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn squad_struct_fields() {
        let s = test_squad();
        assert_eq!(s.id, "sq-test");
        assert_eq!(s.name, "Test Squad");
        assert_eq!(s.status, "active");
        assert_eq!(s.members.len(), 1);
        assert_eq!(s.members[0].agent_id, "a1");
        assert_eq!(s.members[0].role, "coordinator");
    }

    #[test]
    fn squad_member_clone() {
        let m = SquadMember { agent_id: "x".into(), role: "member".into() };
        let m2 = m.clone();
        assert_eq!(m.agent_id, m2.agent_id);
        assert_eq!(m.role, m2.role);
    }

    #[test]
    fn squad_serde_roundtrip() {
        let squad = test_squad();
        let json = serde_json::to_string(&squad).unwrap();
        let parsed: Squad = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, squad.id);
        assert_eq!(parsed.name, squad.name);
        assert_eq!(parsed.members.len(), 1);
    }
}
