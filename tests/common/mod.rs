#![allow(dead_code)]
use std::cell::RefCell;

use agmag::hc::{Hc, RunSpec};
use anyhow::{bail, Result};
use serde_json::{json, Value};

/// hc giả: ghi lại mọi lệnh, `send` thất bại nếu `fail_send`.
#[derive(Default)]
pub struct FakeHc {
    pub calls: RefCell<Vec<String>>,
    pub fail_send: bool,
    /// app mà `hc get` báo cho từng terminal (status.app); thiếu → không có app.
    pub apps: Vec<(String, String)>,
    pub fail_get: bool,
}

impl Hc for FakeHc {
    fn ls(&self, _all: bool) -> Result<Vec<Value>> {
        Ok(vec![])
    }
    fn get(&self, t: &str) -> Result<Value> {
        if self.fail_get {
            bail!("not_found");
        }
        match self.apps.iter().find(|(a, _)| a == t) {
            Some((_, app)) => Ok(json!({"address": t, "reported": true, "status": {"app": app, "state": "idle"}})),
            None => Ok(json!({"address": t})),
        }
    }
    fn send(&self, t: &str, text: &str, if_idle: bool, force: bool) -> Result<Value> {
        self.calls.borrow_mut().push(format!("send {t} if_idle={if_idle} force={force} {text}"));
        if self.fail_send {
            bail!("refused: working");
        }
        Ok(json!({"turn": 4}))
    }
    fn run(&self, spec: &RunSpec) -> Result<Value> {
        self.calls.borrow_mut().push(format!("run {:?} {:?} {:?} {:?}", spec.argv, spec.name, spec.cwd, spec.labels));
        Ok(json!({"address": "t99", "name": spec.name, "argv": spec.argv, "cwd": spec.cwd, "labels": spec.labels, "state": "live"}))
    }
    fn kill(&self, t: &str) -> Result<()> {
        self.calls.borrow_mut().push(format!("kill {t}"));
        Ok(())
    }
    fn interrupt(&self, t: &str) -> Result<()> {
        self.calls.borrow_mut().push(format!("interrupt {t}"));
        Ok(())
    }
}
