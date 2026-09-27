use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{json, Value};

const TIMEOUT: Duration = Duration::from_millis(500);
const ENTER_DELAY: Duration = Duration::from_millis(60);

pub struct Herdr {
    socket: String,
    pane: String,
}

impl Herdr {
    pub fn from_env() -> Result<Self, String> {
        let not_inside = || "not running inside herdr".to_string();
        if cfg!(test) || std::env::var("HERDR_ENV").ok().as_deref() != Some("1") {
            return Err(not_inside());
        }
        Ok(Self {
            socket: std::env::var("HERDR_SOCKET_PATH").map_err(|_| not_inside())?,
            pane: std::env::var("HERDR_PANE_ID").map_err(|_| not_inside())?,
        })
    }

    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let mut stream =
            UnixStream::connect(&self.socket).map_err(|e| format!("herdr is not reachable: {}", e))?;
        let _ = stream.set_read_timeout(Some(TIMEOUT));
        let _ = stream.set_write_timeout(Some(TIMEOUT));
        let request = json!({ "id": format!("ee-{}", method), "method": method, "params": params });
        writeln!(stream, "{}", request).map_err(|e| format!("herdr {}: {}", method, e))?;
        let mut line = String::new();
        BufReader::new(stream)
            .read_line(&mut line)
            .map_err(|e| format!("herdr {}: {}", method, e))?;
        let reply: Value =
            serde_json::from_str(&line).map_err(|_| format!("herdr {}: unreadable reply", method))?;
        if let Some(error) = reply.get("error") {
            let message = error["message"].as_str().unwrap_or("unknown error");
            return Err(format!("herdr: {}", message));
        }
        Ok(reply["result"].clone())
    }

    fn target(&self) -> Result<Value, String> {
        let me = self.call("pane.get", json!({ "pane_id": self.pane }))?["pane"].clone();
        let panes = self.call("pane.list", json!({ "workspace_id": me["workspace_id"] }))?["panes"].clone();
        let others: Vec<Value> = panes
            .as_array()
            .map(|all| {
                all.iter()
                    .filter(|p| p["tab_id"] == me["tab_id"] && p["pane_id"] != me["pane_id"])
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if others.len() <= 1 {
            return others.into_iter().next().ok_or_else(|| "no other pane in this tab".to_string());
        }
        for direction in ["right", "left", "down", "up"] {
            let reply = self.call("pane.neighbor", json!({ "pane_id": me["pane_id"], "direction": direction }));
            if let Ok(reply) = reply {
                let id = &reply["neighbor"]["neighbor_pane_id"];
                if let Some(pane) = others.iter().find(|p| &p["pane_id"] == id) {
                    return Ok(pane.clone());
                }
            }
        }
        Ok(others[0].clone())
    }

    pub fn send(&self, text: &str) -> Result<String, String> {
        let target = self.target()?;
        let id = target["pane_id"].clone();
        if target["agent"].is_string() {
            self.call("agent.prompt", json!({ "target": id, "text": text }))?;
        } else {
            self.call("pane.send_input", json!({ "pane_id": id, "text": text }))?;
            std::thread::sleep(ENTER_DELAY);
            self.call("pane.send_keys", json!({ "pane_id": id, "keys": ["enter"] }))?;
        }
        let name = ["label", "agent", "title"]
            .iter()
            .find_map(|k| target[*k].as_str().filter(|s| !s.is_empty()))
            .or(id.as_str())
            .unwrap_or("pane");
        Ok(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::{Arc, Mutex};

    fn pane(id: &str, tab: &str, agent: Option<&str>) -> Value {
        let mut p = json!({ "pane_id": id, "workspace_id": "w1", "tab_id": tab, "focused": false });
        if let Some(a) = agent {
            p["agent"] = json!(a);
        }
        p
    }

    fn fake_herdr(panes: Vec<Value>, neighbor: Option<&str>) -> (Herdr, Arc<Mutex<Vec<Value>>>) {
        let dir = std::env::temp_dir().join(format!("ee-herdr-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("herdr.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let neighbor = neighbor.map(|n| n.to_string());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let stream = stream.unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let result = match req["method"].as_str().unwrap() {
                    "pane.get" => json!({ "type": "pane_info", "pane": panes[0] }),
                    "pane.list" => json!({ "type": "pane_list", "panes": panes }),
                    "pane.neighbor" if req["params"]["direction"] == "left" => {
                        json!({ "type": "pane_neighbor", "neighbor": { "neighbor_pane_id": neighbor } })
                    }
                    "pane.neighbor" => json!({ "type": "pane_neighbor", "neighbor": { "neighbor_pane_id": null } }),
                    _ => json!({ "type": "ok" }),
                };
                log.lock().unwrap().push(req.clone());
                writeln!(&stream, "{}", json!({ "id": req["id"], "result": result })).unwrap();
            }
        });
        (Herdr { socket: path.to_string_lossy().to_string(), pane: "w1:p1".to_string() }, seen)
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    fn methods(seen: &Arc<Mutex<Vec<Value>>>) -> Vec<String> {
        seen.lock().unwrap().iter().map(|r| r["method"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn pastes_into_the_other_pane_of_the_tab_then_presses_enter() {
        let panes = vec![pane("w1:p1", "t1", None), pane("w1:p2", "t1", None), pane("w1:p3", "t2", None)];
        let (herdr, seen) = fake_herdr(panes, None);
        assert_eq!(herdr.send("ls\npwd"), Ok("w1:p2".to_string()));
        assert_eq!(methods(&seen), ["pane.get", "pane.list", "pane.send_input", "pane.send_keys"]);
        let log = seen.lock().unwrap();
        assert_eq!(log[2]["params"], json!({ "pane_id": "w1:p2", "text": "ls\npwd" }));
        assert_eq!(log[3]["params"], json!({ "pane_id": "w1:p2", "keys": ["enter"] }));
    }

    #[test]
    fn agent_panes_get_a_prompt_and_neighbors_break_ties() {
        let panes = vec![
            pane("w1:p1", "t1", None),
            pane("w1:p2", "t1", None),
            pane("w1:p3", "t1", Some("claude")),
        ];
        let (herdr, seen) = fake_herdr(panes, Some("w1:p3"));
        assert_eq!(herdr.send("fix it"), Ok("claude".to_string()));
        let log = seen.lock().unwrap();
        let prompt = log.iter().find(|r| r["method"] == "agent.prompt").unwrap();
        assert_eq!(prompt["params"], json!({ "target": "w1:p3", "text": "fix it" }));
        assert!(log.iter().all(|r| r["method"] != "pane.send_input"));
    }

    #[test]
    fn reports_a_tab_with_no_other_pane_and_never_sends_to_itself() {
        let (herdr, seen) = fake_herdr(vec![pane("w1:p1", "t1", None), pane("w1:p9", "t2", None)], None);
        assert_eq!(herdr.send("ls"), Err("no other pane in this tab".to_string()));
        assert_eq!(methods(&seen), ["pane.get", "pane.list"]);
        assert!(Herdr::from_env().is_err(), "tests never reach the real herdr");
    }
}
