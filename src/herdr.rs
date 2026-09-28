use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{json, Value};

const TIMEOUT: Duration = Duration::from_millis(500);
const TITLE_POLLS: u32 = 10;
const TITLE_POLL: Duration = Duration::from_millis(40);

pub trait Desktop {
    fn windows_titled(&self, token: &str) -> Vec<String>;
    fn focus(&self, address: &str) -> bool;
}

pub struct Sent {
    pub name: String,
    pub pane_id: String,
}

pub struct Herdr {
    socket: String,
    pane: Option<String>,
}

impl Herdr {
    pub fn from_env() -> Result<Self, String> {
        if cfg!(test) {
            return Err("herdr is disabled in tests".to_string());
        }
        let inside = std::env::var("HERDR_ENV").ok().as_deref() == Some("1");
        let pane = std::env::var("HERDR_PANE_ID").ok().filter(|_| inside);
        let socket = std::env::var("HERDR_SOCKET_PATH").ok().or_else(|| {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
            Some(config.join("herdr").join("herdr.sock").to_string_lossy().to_string())
        });
        match socket {
            Some(socket) if inside || std::path::Path::new(&socket).exists() => Ok(Self { socket, pane }),
            _ => Err("herdr is not running".to_string()),
        }
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
        let Some(pane) = &self.pane else {
            let focused = self.call("pane.current", json!({}))?["pane"].clone();
            return if focused["pane_id"].is_string() { Ok(focused) } else { Err("herdr has no focused pane".to_string()) };
        };
        let me = self.call("pane.get", json!({ "pane_id": pane }))?["pane"].clone();
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

    pub fn send(&self, text: &str) -> Result<Sent, String> {
        let target = self.target()?;
        let id = target["pane_id"].clone();
        self.call("pane.send_input", json!({ "pane_id": id, "text": text }))?;
        let name = ["label", "agent", "title"]
            .iter()
            .find_map(|k| target[*k].as_str().filter(|s| !s.is_empty()))
            .or(id.as_str())
            .unwrap_or("pane");
        Ok(Sent { name: name.to_string(), pane_id: id.as_str().unwrap_or_default().to_string() })
    }

    pub fn bring_forward(&self, pane_id: &str, desktop: &dyn Desktop) -> bool {
        if self.call("pane.focus", json!({ "pane_id": pane_id })).is_err() {
            return false;
        }
        if self.pane.is_some() {
            return true;
        }
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let token = format!("ee-focus-{}-{}", std::process::id(), nanos);
        let marked = self.call("client.window_title.set", json!({ "title": token })).is_ok_and(|r| r["reason"] == "set");
        let mut windows = Vec::new();
        for _ in 0..TITLE_POLLS {
            if !marked {
                break;
            }
            windows = desktop.windows_titled(&token);
            if !windows.is_empty() {
                break;
            }
            std::thread::sleep(TITLE_POLL);
        }
        let _ = self.call("client.window_title.clear", json!({}));
        match windows.as_slice() {
            [only] => desktop.focus(only),
            _ => false,
        }
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
        fake_herdr_titled(panes, neighbor, "set")
    }

    fn fake_herdr_titled(panes: Vec<Value>, neighbor: Option<&str>, title_reason: &'static str) -> (Herdr, Arc<Mutex<Vec<Value>>>) {
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
                let known = |id: &Value| panes.iter().any(|p| p["pane_id"] == *id);
                if req["method"] == "pane.focus" && !known(&req["params"]["pane_id"]) {
                    log.lock().unwrap().push(req.clone());
                    writeln!(&stream, "{}", json!({ "id": req["id"], "error": { "code": "pane_not_found", "message": "pane not found" } })).unwrap();
                    continue;
                }
                let result = match req["method"].as_str().unwrap() {
                    "client.window_title.set" => json!({ "type": "client_window_title", "changed": title_reason == "set", "reason": title_reason }),
                    "pane.get" => json!({ "type": "pane_info", "pane": panes[0] }),
                    "pane.current" => json!({ "type": "pane_current", "pane": panes.iter().find(|p| p["focused"] == true).cloned().unwrap_or(Value::Null) }),
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
        (Herdr { socket: path.to_string_lossy().to_string(), pane: Some("w1:p1".to_string()) }, seen)
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    fn methods(seen: &Arc<Mutex<Vec<Value>>>) -> Vec<String> {
        seen.lock().unwrap().iter().map(|r| r["method"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn pastes_into_the_other_pane_of_the_tab_without_pressing_enter() {
        let panes = vec![pane("w1:p1", "t1", None), pane("w1:p2", "t1", None), pane("w1:p3", "t2", None)];
        let (herdr, seen) = fake_herdr(panes, None);
        assert_eq!(herdr.send("ls\npwd").map(|s| (s.name, s.pane_id)), Ok(("w1:p2".to_string(), "w1:p2".to_string())));
        assert_eq!(methods(&seen), ["pane.get", "pane.list", "pane.send_input"]);
        let log = seen.lock().unwrap();
        assert_eq!(log[0]["params"], json!({ "pane_id": "w1:p1" }));
        assert_eq!(log[1]["params"], json!({ "workspace_id": "w1" }));
        assert_eq!(log[2]["params"], json!({ "pane_id": "w1:p2", "text": "ls\npwd" }));
    }

    #[test]
    fn agent_panes_get_the_same_paste_and_neighbors_break_ties() {
        let panes = vec![
            pane("w1:p1", "t1", None),
            pane("w1:p2", "t1", None),
            pane("w1:p3", "t1", Some("claude")),
        ];
        let (herdr, seen) = fake_herdr(panes, Some("w1:p3"));
        assert_eq!(herdr.send("fix it").map(|s| s.name), Ok("claude".to_string()));
        let log = seen.lock().unwrap();
        assert!(log.iter().filter(|r| r["method"] == "pane.neighbor").all(|r| r["params"]["pane_id"] == "w1:p1"));
        let sent = log.iter().find(|r| r["method"] == "pane.send_input").unwrap();
        assert_eq!(sent["params"], json!({ "pane_id": "w1:p3", "text": "fix it" }));
        assert!(log.iter().all(|r| r["method"] != "agent.prompt"), "agent.prompt would submit it");
    }

    #[test]
    fn outside_herdr_it_sends_to_the_focused_pane() {
        let mut focused = pane("w2:p4", "t9", None);
        focused["focused"] = json!(true);
        let (mut herdr, seen) = fake_herdr(vec![pane("w1:p1", "t1", None), focused], None);
        herdr.pane = None;
        assert_eq!(herdr.send("make test").map(|s| s.name), Ok("w2:p4".to_string()));
        assert_eq!(methods(&seen), ["pane.current", "pane.send_input"]);
        assert_eq!(seen.lock().unwrap()[1]["params"], json!({ "pane_id": "w2:p4", "text": "make test" }));
    }

    #[test]
    fn outside_herdr_with_nothing_focused_sends_nothing() {
        let (mut herdr, seen) = fake_herdr(vec![pane("w1:p1", "t1", None)], None);
        herdr.pane = None;
        assert_eq!(herdr.send("ls").map(|s| s.name), Err("herdr has no focused pane".to_string()));
        assert_eq!(methods(&seen), ["pane.current"]);
    }

    #[test]
    fn reports_a_tab_with_no_other_pane_and_never_sends_to_itself() {
        let (herdr, seen) = fake_herdr(vec![pane("w1:p1", "t1", None), pane("w1:p9", "t2", None)], None);
        assert_eq!(herdr.send("ls").map(|s| s.name), Err("no other pane in this tab".to_string()));
        assert_eq!(methods(&seen), ["pane.get", "pane.list"]);
        assert_eq!(Herdr::from_env().err().as_deref(), Some("herdr is disabled in tests"));
    }

    struct FakeDesktop {
        matching: usize,
        asked: std::cell::RefCell<Vec<String>>,
        focused: std::cell::RefCell<Vec<String>>,
    }

    impl FakeDesktop {
        fn with(matching: usize) -> Self {
            Self { matching, asked: Default::default(), focused: Default::default() }
        }
    }

    impl Desktop for FakeDesktop {
        fn windows_titled(&self, token: &str) -> Vec<String> {
            self.asked.borrow_mut().push(token.to_string());
            (0..self.matching).map(|i| format!("0xw{}", i)).collect()
        }
        fn focus(&self, address: &str) -> bool {
            self.focused.borrow_mut().push(address.to_string());
            true
        }
    }

    #[test]
    fn inside_herdr_a_move_focuses_the_target_pane_and_nothing_else() {
        let (herdr, seen) = fake_herdr(vec![pane("w1:p1", "t1", None), pane("w1:p2", "t1", None)], None);
        let desktop = FakeDesktop::with(1);
        assert!(herdr.bring_forward("w1:p2", &desktop));
        assert_eq!(methods(&seen), ["pane.focus"]);
        assert_eq!(seen.lock().unwrap()[0]["params"], json!({ "pane_id": "w1:p2" }));
        assert!(desktop.asked.borrow().is_empty(), "ee's own window is already the active one");
    }

    #[test]
    fn outside_herdr_the_window_is_focused_only_when_exactly_one_shows_the_token() {
        let mut focused = pane("w2:p4", "t9", None);
        focused["focused"] = json!(true);
        for (matching, expected) in [(1, true), (2, false), (0, false)] {
            let (mut herdr, seen) = fake_herdr(vec![focused.clone()], None);
            herdr.pane = None;
            let desktop = FakeDesktop::with(matching);
            assert_eq!(herdr.bring_forward("w2:p4", &desktop), expected, "{} matching windows", matching);
            let log = seen.lock().unwrap();
            let names: Vec<&str> = log.iter().map(|r| r["method"].as_str().unwrap()).collect();
            assert_eq!(names, ["pane.focus", "client.window_title.set", "client.window_title.clear"], "the title is always put back");
            let token = log[1]["params"]["title"].as_str().unwrap().to_string();
            assert!(token.starts_with("ee-focus-"));
            assert!(desktop.asked.borrow().iter().all(|t| *t == token), "the window is found by the title herdr was given");
            assert_eq!(*desktop.focused.borrow(), if expected { vec!["0xw0".to_string()] } else { vec![] });
        }
    }

    #[test]
    fn anything_uncertain_leaves_the_windows_alone() {
        let mut focused = pane("w2:p4", "t9", None);
        focused["focused"] = json!(true);
        let (mut herdr, _) = fake_herdr_titled(vec![focused.clone()], None, "no_foreground_client");
        herdr.pane = None;
        let desktop = FakeDesktop::with(1);
        assert!(!herdr.bring_forward("w2:p4", &desktop), "no herdr window in front");
        assert!(desktop.asked.borrow().is_empty() && desktop.focused.borrow().is_empty());

        let (herdr, seen) = fake_herdr(vec![pane("w1:p1", "t1", None)], None);
        assert!(!herdr.bring_forward("w1:p9", &desktop), "the pane is gone");
        assert_eq!(methods(&seen), ["pane.focus"]);
        assert!(desktop.focused.borrow().is_empty());
    }
}
