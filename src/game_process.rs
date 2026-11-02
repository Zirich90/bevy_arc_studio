//! Separate game process owned by the editor. The child exposes the usual BRP API.
use bevy::prelude::*;
use crate::mcp_commands::CustomCommandAppExt;
use std::process::{Child, Command};

#[derive(Resource, Default)]
pub struct GameProcess { child: Option<Child>, pub port: u16 }
impl Drop for GameProcess {
    fn drop(&mut self) { if let Some(child) = &mut self.child { let _ = child.kill(); let _ = child.wait(); } }
}

fn start(world: &mut World, args: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Args { path: String, port: u16 }
    let args: Args = serde_json::from_str(args).map_err(|e| e.to_string())?;
    if args.port == 0 { return Err("port must be nonzero".into()); }
    if !crate::world_io::is_reflect_path(&args.path) || !std::path::Path::new(&args.path).is_file() {
        return Err("save a .scn.json scene before launching".into());
    }
    let mut game = world.resource_mut::<GameProcess>();
    if let Some(child) = &mut game.child {
        if child.try_wait().map_err(|e| e.to_string())?.is_none() { return Err("game already running".into()); }
    }
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, args.port)).map_err(|e| format!("BRP port {} unavailable: {e}", args.port))?;
    drop(listener);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = Command::new(exe);
    command.arg("--game-scene").arg(&args.path).env("BRP_EXTRAS_PORT", args.port.to_string());
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
    let child = command.spawn().map_err(|e| e.to_string())?;
    let pid = child.id();
    game.child = Some(child); game.port = args.port;
    Ok(format!("game pid={pid}, BRP http://127.0.0.1:{}", args.port))
}

fn stop(world: &mut World, _: &str) -> Result<String, String> {
    let mut game = world.resource_mut::<GameProcess>();
    if let Some(child) = &mut game.child {
        if child.try_wait().map_err(|e| e.to_string())?.is_none() { child.kill().map_err(|e| e.to_string())?; }
        child.wait().map_err(|e| e.to_string())?;
    }
    game.child = None;
    Ok("game stopped; editor scene unchanged".into())
}

pub struct GameProcessPlugin;
impl Plugin for GameProcessPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameProcess>().init_resource::<GamePanel>()
            .add_systems(bevy_egui::EguiPrimaryContextPass, panel)
            .register_custom_command("game.start", start)
            .register_custom_command("game.stop", stop)
            .register_custom_command("scene.switch", |world, args| {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Args { path: String }
                let args: Args = serde_json::from_str(args).map_err(|e| e.to_string())?;
                if !crate::world_io::is_reflect_path(&args.path) { return Err("expected .scn.json".into()); }
                std::fs::metadata(&args.path).map_err(|e| e.to_string())?;
                std::fs::metadata(format!("{}.heights", args.path)).map_err(|e| e.to_string())?;
                crate::play_mode::cmd_set_play_mode(world, crate::studio::StudioMode::Edit)?;
                world.resource_mut::<crate::world_io::ResumeAfterScene>().0 = true;
                world.resource_mut::<crate::world_io::SceneIoQueue>().0.push(crate::world_io::SceneIo::Load(args.path));
                Ok("scene transition queued".into())
            });
    }
}

/// Loopback BRP transport, used on a worker thread so an unresponsive game cannot freeze egui.
fn rpc(port: u16, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    use std::io::{Read, Write};
    let timeout = std::time::Duration::from_secs(2);
    let mut socket = std::net::TcpStream::connect_timeout(&([127,0,0,1], port).into(), timeout).map_err(|e| e.to_string())?;
    socket.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    socket.set_write_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    let body = serde_json::json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params}).to_string();
    write!(socket, "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}", body.len(), body).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    socket.take(8 * 1024 * 1024).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let split = bytes.windows(4).position(|v| v == b"\r\n\r\n").ok_or("invalid HTTP response")?;
    let header = String::from_utf8_lossy(&bytes[..split]).to_lowercase();
    let mut body = bytes[split+4..].to_vec();
    if header.contains("transfer-encoding: chunked") {
        let mut decoded = Vec::new(); let mut rest = body.as_slice();
        loop {
            let end = rest.windows(2).position(|v| v == b"\r\n").ok_or("invalid chunk")?;
            let size = usize::from_str_radix(std::str::from_utf8(&rest[..end]).map_err(|e| e.to_string())?.split(';').next().unwrap(),16).map_err(|e| e.to_string())?;
            rest = &rest[end+2..];
            if size == 0 { break; }
            if rest.len() < size+2 { return Err("truncated chunk".into()); }
            decoded.extend_from_slice(&rest[..size]); rest = &rest[size+2..];
        }
        body = decoded;
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if let Some(error) = value.get("error") { return Err(error.to_string()); }
    value.get("result").cloned().ok_or("missing RPC result".into())
}

#[derive(Resource)]
struct GamePanel {
    path: String, component: String, value: String, entity: u64,
    last_refresh: std::time::Instant, auto_refresh: bool, reply: String, rows: Vec<serde_json::Value>, pending: Option<std::sync::Mutex<std::sync::mpsc::Receiver<Result<serde_json::Value,String>>>>,
}
impl Default for GamePanel {
    fn default() -> Self { Self { path: "scenes/behavior_pilot.scn.json".into(), component: "bevy_arc_studio::behavior::TimedVisibility".into(), value: "{\"delay_seconds\":10.0,\"visible\":false}".into(), entity: 0, last_refresh: std::time::Instant::now(), auto_refresh: true, reply: String::new(), rows: Vec::new(), pending: None } }
}
impl GamePanel {
    fn request(&mut self, port: u16, method: &str, params: serde_json::Value) {
        if self.pending.is_some() { return; }
        let (tx, rx) = std::sync::mpsc::channel(); let method = method.to_owned();
        std::thread::spawn(move || { let _ = tx.send(rpc(port, &method, params)); });
        self.pending = Some(std::sync::Mutex::new(rx));
    }
}
fn panel(mut contexts: bevy_egui::EguiContexts, mut panel: ResMut<GamePanel>, game: Res<GameProcess>, mut queue: ResMut<crate::mcp_commands::StudioCommandQueue>, studio: Res<crate::studio::StudioState>) {
    if studio.tool == "node_graph" { return; }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let result = panel.pending.as_ref().and_then(|rx| rx.lock().ok()?.try_recv().ok());
    if let Some(result) = result {
        panel.reply = match result { Ok(v) => { if let Some(value) = v.get("components").and_then(|c| c.get(&panel.component)) { panel.value = serde_json::to_string_pretty(value).unwrap_or_default(); } if let Some(rows) = v.as_array() { panel.rows = rows.clone(); } serde_json::to_string_pretty(&v).unwrap_or_default() }, Err(e) => e }; panel.pending = None;
    }
    bevy_egui::egui::Window::new("Separate game").default_pos([780.0, 100.0]).default_open(true).default_width(430.0).show(ctx, |ui| {
        ui.label("Saved scene"); ui.text_edit_singleline(&mut panel.path);
        ui.horizontal(|ui| {
            if ui.button("Launch").clicked() { queue.commands.push(crate::mcp_commands::StudioCommand::Custom { name:"game.start".into(),args:serde_json::json!({"path":panel.path,"port":15704}).to_string() }); }
            if ui.button("Stop").clicked() { queue.commands.push(crate::mcp_commands::StudioCommand::Custom { name:"game.stop".into(),args:"{}".into() }); }
            for (label, mode) in [("Pause","Paused"),("Resume","Playing")] { if ui.button(label).clicked() {
                panel.request(game.port,"world.insert_resources",serde_json::json!({"resource":"bevy_arc_studio::mcp_commands::StudioCommandQueue","value":{"commands":[{"SetPlayMode":{"mode":mode}}],"log":[]}}));
            } }
        });
        if ui.button("Switch game to scene").clicked() {
            let params = serde_json::json!({"resource":"bevy_arc_studio::mcp_commands::StudioCommandQueue","value":{"commands":[{"Custom":{"name":"scene.switch","args":serde_json::json!({"path":panel.path}).to_string()}}],"log":[]}});
            panel.request(game.port,"world.insert_resources",params);
        }
        ui.checkbox(&mut panel.auto_refresh, "Refresh tree every second");
        let refresh = ui.button("Refresh live entities").clicked() || (game.child.is_some() && panel.auto_refresh && panel.pending.is_none() && panel.last_refresh.elapsed().as_secs_f32() >= 1.0);
        if refresh { panel.last_refresh = std::time::Instant::now(); panel.request(game.port,"world.query",serde_json::json!({"data":{"components":["bevy_arc_studio::props::SceneObject"],"option":["bevy_ecs::hierarchy::ChildOf","bevy_transform::components::transform::Transform"]}})); }
        let rows = ordered_rows(&panel.rows);
        bevy_egui::egui::ScrollArea::vertical().id_salt("game_tree").max_height(160.0).show(ui, |ui| {
            for (depth, row) in rows {
                let id = row["entity"].as_u64().unwrap_or(0);
                let name = row["components"]["bevy_arc_studio::props::SceneObject"]["name"].as_str().unwrap_or("Entity");
                let label = format!("{}{name} ({id})", "    ".repeat(depth));
                if ui.selectable_label(panel.entity == id, label).clicked() { panel.entity = id; }
            }
        });
        ui.horizontal(|ui| { ui.label("Entity"); ui.add(bevy_egui::egui::DragValue::new(&mut panel.entity)); });
        ui.text_edit_singleline(&mut panel.component); ui.text_edit_multiline(&mut panel.value);
        if ui.button("Read component from game").clicked() {
            let params = serde_json::json!({"entity":panel.entity,"components":[panel.component.clone()]});
            panel.request(game.port,"world.get_components",params);
        }
        if ui.button("Apply component to game").clicked() {
            match serde_json::from_str::<serde_json::Value>(&panel.value) {
                Ok(value) => { let params=serde_json::json!({"entity":panel.entity,"components":{panel.component.clone():value}}); panel.request(game.port,"world.insert_components",params); }
                Err(e) => panel.reply=e.to_string(),
            }
        }
        if panel.pending.is_some() { ui.spinner(); }
        bevy_egui::egui::ScrollArea::vertical().max_height(300.0).show(ui,|ui| { ui.monospace(&panel.reply); });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_orders_parents_and_survives_cycles() {
        let rows = vec![
            serde_json::json!({"entity":2,"components":{"bevy_ecs::hierarchy::ChildOf":1}}),
            serde_json::json!({"entity":1,"components":{}}),
            serde_json::json!({"entity":3,"components":{"bevy_ecs::hierarchy::ChildOf":3}}),
        ];
        let ordered = ordered_rows(&rows);
        assert_eq!(ordered.iter().map(|(d,r)| (*d,r["entity"].as_u64().unwrap())).collect::<Vec<_>>(), vec![(0,1),(1,2),(0,3)]);
    }
    #[test]
    fn launch_rejects_invalid_input_and_stop_is_idempotent() {
        let mut world = World::new(); world.init_resource::<GameProcess>();
        assert!(start(&mut world, r#"{"path":"missing.scn.json","port":0}"#).is_err());
        assert!(start(&mut world, r#"{"path":"missing.scn.json","port":15704}"#).is_err());
        assert!(start(&mut world, r#"{"path":"missing.scn.json","port":15704,"extra":true}"#).is_err());
        assert!(stop(&mut world,"{}").is_ok()); assert!(stop(&mut world,"{}").is_ok());
    }
}

fn ordered_rows(rows: &[serde_json::Value]) -> Vec<(usize, serde_json::Value)> {
    fn visit(id: u64, depth: usize, rows: &[serde_json::Value], seen: &mut std::collections::HashSet<u64>, out: &mut Vec<(usize,serde_json::Value)>) {
        if !seen.insert(id) { return; }
        if let Some(row) = rows.iter().find(|r| r["entity"].as_u64() == Some(id)) { out.push((depth,row.clone())); }
        for row in rows {
            if row["components"]["bevy_ecs::hierarchy::ChildOf"].as_u64() == Some(id) {
                if let Some(child) = row["entity"].as_u64() { visit(child,depth+1,rows,seen,out); }
            }
        }
    }
    let mut out = Vec::new(); let mut seen = std::collections::HashSet::new();
    for row in rows {
        let parent = row["components"]["bevy_ecs::hierarchy::ChildOf"].as_u64();
        if !rows.iter().any(|r| r["entity"].as_u64().is_some() && r["entity"].as_u64() == parent) {
            if let Some(id) = row["entity"].as_u64() { visit(id,0,rows,&mut seen,&mut out); }
        }
    }
    for row in rows { if let Some(id) = row["entity"].as_u64() { visit(id,0,rows,&mut seen,&mut out); } }
    out
}
