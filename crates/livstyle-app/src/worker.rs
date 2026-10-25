//! Background owner of the product store. The UI thread only draws snapshots.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use livstyle_core::Service;
use livstyle_cursor::{
    account_status_line, locate_bridge, runtime_caps, Bridge, BridgeConfig, CursorClient, RunEvent,
    VersionInfo,
};
use livstyle_domain::TaskStatus;
use livstyle_esp::identify_chip;
use livstyle_http::{send as http_send, Url};
use livstyle_octos::{OctosClient, ServerCaps};
use livstyle_secrets::{CredentialStore, Secret, CURSOR_API_KEY};
use livstyle_steering::{Action, Resolution, RuleJudge, RuntimeCaps, Stage};
use makepad_widgets::makepad_platform::thread::SignalToUI;
use serde_json::Value;

use crate::turns::{
    busy_text, cancel_effect, cursor_agent_choice, cursor_beats, octos_beats, plan_switch,
    session_align, BusyText, CancelEffect, CodeAgent, CursorAgentChoice, SessionAlign, SwitchPlan,
    TurnBeat,
};

#[derive(Debug)]
pub enum Cmd {
    Boot,
    Send(String),
    Pause,
    Cancel,
    NewTask,
    Select(usize),
    Delete(usize),
    SetWorkspace(String),
    BrowseWorkspace,
    RefreshDevices,
    Identify,
    OpenSettings,
    OpenFlash,
    OpenUnknown,
    Primary,
    Secondary,
    Dismiss,
    UseAgent(CodeAgent),
}

#[derive(Clone, Debug)]
pub struct TaskLine {
    pub title: String,
    pub status: String,
    pub mark: String,
}

#[derive(Clone, Debug)]
pub struct Card {
    pub title: String,
    pub body: String,
    pub primary: String,
    pub secondary: String,
    pub dismiss: String,
    pub kind: CardKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardKind {
    Confirm,
    Flash,
    Unknown,
    Settings,
    Identify,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub runtime: String,
    pub tasks: Vec<TaskLine>,
    pub transcript: String,
    pub workspace: String,
    pub cursor_status: String,
    pub octos_status: String,
    pub board_status: String,
    pub board_log: String,
    pub build_log: String,
    pub plan: String,
    pub hint: String,
    pub card: Option<Card>,
}

struct State {
    svc: Service,
    tx: Sender<Snapshot>,
    selected: String,
    card: Option<Card>,
    hint: String,
    transcript: Vec<String>,
    workspace: String,
    cursor_status: String,
    octos_status: String,
    board_status: String,
    board_log: Vec<String>,
    build_log: Vec<String>,
    plan: String,
    turn_task: Option<String>,
    live: Vec<String>,
    pending_digest: Option<String>,
    chip: Option<String>,
    pending_id: Option<String>,
    pending_port: Option<std::path::PathBuf>,
    octos: Option<OctosClient>,
    session_id: Option<String>,
    active_turn: Option<String>,
    seq: u64,
    agent: CodeAgent,
    pending_agent: Option<CodeAgent>,
    cursor: Option<CursorLink>,
    busy: bool,
    followups: VecDeque<String>,
    deferred: Vec<Cmd>,
}

struct CursorLink {
    bridge: Bridge,
    client: CursorClient,
    secret: Secret,
    caps: RuntimeCaps,
    model: Option<String>,
    agent_id: Option<String>,
    agent_cwd: Option<String>,
    bridge_cwd: PathBuf,
}

pub fn run(rx: Receiver<Cmd>, tx: Sender<Snapshot>) {
    let path = db_path();
    let mut svc = match Service::open(&path) {
        Ok(svc) => svc,
        Err(e) => {
            publish(&tx, empty_snapshot(&format!("打不开工作台数据库：{e}")));
            return;
        }
    };
    if svc.is_empty().unwrap_or(true) {
        seed(&mut svc);
    }
    let workspace = load_workspace();
    let mut state = State {
        svc,
        tx: tx.clone(),
        selected: String::new(),
        card: None,
        hint: "先选定代码目录，再在下面输入。".into(),
        transcript: vec!["livstyle：在下面输入。问「检测到芯片了没」会扫描串口，不会复位。点「识别芯片」才会复位并读出型号。改代码前，先在左边填项目目录。".into()],
        workspace,
        cursor_status: "Cursor  正在检查".into(),
        octos_status: "Octos  正在检查".into(),
        board_status: board_line(),
        board_log: vec!["串口扫描和芯片识别会写在这里。".into()],
        build_log: Vec::new(),
        plan: String::new(),
        turn_task: None,
        live: Vec::new(),
        pending_digest: None,
        chip: None,
        pending_id: None,
        pending_port: None,
        octos: None,
        session_id: None,
        active_turn: None,
        seq: 1,
        agent: CodeAgent::Cursor,
        pending_agent: None,
        cursor: None,
        busy: false,
        followups: VecDeque::new(),
        deferred: Vec::new(),
    };
    if let Some(id) = state
        .svc
        .task_ids()
        .ok()
        .and_then(|ids| ids.into_iter().next())
    {
        state.selected = id.clone();
        if let Ok((_, task)) = state.svc.task(&id) {
            if !task.transcript.is_empty() {
                state.transcript = task.transcript;
            }
            state.plan = state.svc.plan_summary(&id).unwrap_or_default();
        }
    }
    publish(&tx, state.snapshot());
    while let Ok(cmd) = rx.recv() {
        state.handle(cmd, &rx);
        publish(&tx, state.snapshot());
    }
}

fn publish(tx: &Sender<Snapshot>, snap: Snapshot) {
    let _ = tx.send(snap);
    SignalToUI::set_ui_signal();
}

fn empty_snapshot(hint: &str) -> Snapshot {
    Snapshot {
        runtime: "Runtime: —".into(),
        tasks: Vec::new(),
        transcript: String::new(),
        workspace: String::new(),
        cursor_status: String::new(),
        octos_status: String::new(),
        board_status: String::new(),
        board_log: String::new(),
        build_log: String::new(),
        plan: String::new(),
        hint: hint.into(),
        card: None,
    }
}

impl State {
    fn handle(&mut self, cmd: Cmd, rx: &Receiver<Cmd>) {
        match cmd {
            Cmd::Boot => self.boot(),
            Cmd::Send(text) => self.send(&text, rx),
            Cmd::Pause => self.send("暂停", rx),
            Cmd::Cancel => self.cancel_selected(),
            Cmd::UseAgent(agent) => self.use_agent(agent),
            Cmd::NewTask => self.new_task(),
            Cmd::Select(index) => self.select_at(index),
            Cmd::Delete(index) => self.delete_at(index),
            Cmd::SetWorkspace(path) => self.set_workspace(path),
            Cmd::BrowseWorkspace => self.browse_workspace(),
            Cmd::RefreshDevices => self.refresh_boards(),
            Cmd::Identify => self.ask_identify(),
            Cmd::OpenSettings => self.card = Some(settings_card()),
            Cmd::OpenFlash => self.open_flash(),
            Cmd::OpenUnknown => self.card = Some(unknown_card()),
            Cmd::Primary => self.primary(),
            Cmd::Secondary => self.secondary(),
            Cmd::Dismiss => {
                if let Some(id) = self.pending_id.take() {
                    let _ = self.svc.settle(&id, "rejected");
                }
                self.card = None;
                self.hint = "已取消这次确认。".into();
            }
        }
    }

    fn say(&mut self, line: String) {
        self.write_turn_line(None, line);
    }

    fn write_turn_line(&mut self, slot: Option<usize>, line: String) {
        let running_here = self.turn_task.as_deref() == Some(self.selected.as_str());
        let idle = self.turn_task.is_none();
        if running_here || idle {
            if let Some(index) = slot {
                if let Some(existing) = self.transcript.get_mut(index) {
                    *existing = line;
                }
            } else {
                self.transcript.push(line);
                if self.transcript.len() > 2000 {
                    let extra = self.transcript.len() - 2000;
                    self.transcript.drain(0..extra);
                }
            }
            self.live = self.transcript.clone();
            return;
        }
        if slot.is_none() && self.live.is_empty() {
            self.live = self
                .turn_task
                .as_deref()
                .and_then(|id| self.svc.task(id).ok())
                .map(|(_, task)| task.transcript)
                .unwrap_or_default();
        }
        if let Some(index) = slot {
            if let Some(existing) = self.live.get_mut(index) {
                *existing = line;
            }
        } else {
            self.live.push(line);
            if self.live.len() > 2000 {
                let extra = self.live.len() - 2000;
                self.live.drain(0..extra);
            }
        }
    }

    fn select_at(&mut self, index: usize) {
        let Some(id) = self
            .svc
            .task_ids()
            .ok()
            .and_then(|ids| ids.into_iter().nth(index))
        else {
            return;
        };
        if !self.selected.is_empty() && self.selected != id {
            let lines = if self.turn_task.as_deref() == Some(self.selected.as_str()) {
                self.live.clone()
            } else {
                self.transcript.clone()
            };
            let _ = self.svc.set_transcript(&self.selected, &lines);
        }
        self.selected = id.clone();
        if self.turn_task.as_deref() == Some(id.as_str()) {
            self.transcript = self.live.clone();
        } else {
            self.transcript = self
                .svc
                .task(&id)
                .map(|(_, task)| task.transcript)
                .unwrap_or_default();
        }
        self.plan = self.svc.plan_summary(&id).unwrap_or_default();
        if self.turn_task.as_deref() == Some(id.as_str()) {
            self.hint = "这一轮还在当前任务上。".into();
        } else if self.turn_task.is_some() {
            self.hint = "已切换任务。另一条任务仍在运行，它的新输出不会写进这里。".into();
        } else {
            self.hint = format!("当前任务 {id}。");
        }
    }

    fn note_board(&mut self, line: impl Into<String>) {
        self.board_log.insert(0, line.into());
        if self.board_log.len() > 400 {
            self.board_log.truncate(400);
        }
    }

    fn boot(&mut self) {
        self.open_cursor();
        self.board_status = board_line();
        self.note_board(scan_reply());
        self.say(format!(
            "livstyle：当前用 {} 写代码。连接器里可以换成另一个，这一轮结束后生效。",
            self.agent.label()
        ));
        let snap = self.snapshot();
        publish(&self.tx, snap);
        self.attach_octos();
        self.restore_octos_sessions();
        self.observe_unfinished_runs();
    }

    fn send(&mut self, text: &str, rx: &Receiver<Cmd>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.say(format!("你：{text}"));
        if wants_identify(text) {
            self.ask_identify();
            return;
        }
        if asks_board(text) {
            self.refresh_boards();
            return;
        }
        if self.workspace_dir().is_none() {
            self.say("livstyle：还没有可用的代码目录。在左边填一个已经存在的项目文件夹，再点「使用」。开发会写在这个目录里。".into());
            self.hint = "先选定代码目录。".into();
            return;
        }
        if self.record_intervention(text) {
            self.hint = "已取消。没有再发给 Runtime。".into();
            self.say("livstyle：这句话取消了任务，没有开始新的一轮。".into());
            return;
        }
        match self.agent {
            CodeAgent::Cursor => self.ask_cursor(text, rx),
            CodeAgent::Octos => self.ask_octos(text, rx),
        }
    }

    fn record_intervention(&mut self, text: &str) -> bool {
        if self.selected.is_empty() {
            self.new_task();
        }
        if self.selected.is_empty() {
            return false;
        }
        self.seq += 1;
        let id = format!("ui-{}-{}", std::process::id(), self.seq);
        let caps = RuntimeCaps {
            steer: self.agent.can_steer(),
            interrupt: true,
        };
        if let Ok(out) =
            self.svc
                .intervene(&self.selected, &id, text, &RuleJudge, caps, Stage::First)
        {
            if matches!(out.resolution, Resolution::Apply(Action::QueueForNextSend)) {
                let _ = self.svc.settle(&id, "queued");
            }
            return matches!(out.resolution, Resolution::Apply(Action::CancelTask));
        }
        false
    }

    fn cancel_selected(&mut self) {
        self.followups.clear();
        let id = self
            .turn_task
            .clone()
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| self.selected.clone());
        if id.is_empty() {
            self.hint = "没有选中的任务，没有执行。".into();
            return;
        }
        let Ok((_, task)) = self.svc.task(&id) else {
            self.hint = "没有选中的任务，没有执行。".into();
            return;
        };
        match cancel_effect(task.status) {
            CancelEffect::AlreadyTerminal => {
                self.hint = "任务已经结束，没有重新发给 Runtime。".into();
                self.say("livstyle：取消没有再执行这一轮。".into());
            }
            CancelEffect::MarkCancelled => match self.svc.transition(&id, TaskStatus::Cancelled) {
                Ok(_) => {
                    self.hint = "已取消。没有把它记成成功，也没有重新发送。".into();
                    self.say("livstyle：任务已取消。".into());
                }
                Err(err) => self.hint = format!("没有取消：{err}"),
            },
        }
    }

    fn open_flash(&mut self) {
        let Some(port) = self.pending_port.clone() else {
            self.pending_digest = None;
            self.card = Some(notice(
                "不能烧录",
                "还没有识别过的串口。先点「识别芯片」。没有调用烧录器。",
            ));
            return;
        };
        let Some(chip) = self.chip.clone() else {
            self.pending_digest = None;
            self.card = Some(notice(
                "不能烧录",
                "还没有芯片型号。先点「识别芯片」。没有调用烧录器。",
            ));
            return;
        };
        let build = std::path::Path::new(&self.workspace).join("build");
        match livstyle_esp::flash_offer(&build, &port.display().to_string(), &chip) {
            Ok(offer) => {
                let short = offer.digest.chars().take(16).collect::<String>();
                self.pending_digest = Some(offer.digest);
                self.card = Some(Card {
                    title: "按这个产物烧录".into(),
                    body: format!(
                        "端口  {}\n芯片  {chip}\n摘要  {short}\n确认后才会写这块板。摘要对不上就不会写。",
                        port.display()
                    ),
                    primary: "按此摘要烧录".into(),
                    secondary: "不烧".into(),
                    dismiss: "关闭".into(),
                    kind: CardKind::Flash,
                });
            }
            Err(err) => {
                self.pending_digest = None;
                self.card = Some(notice("不能烧录", &format!("{err}\n没有调用烧录器。")));
            }
        }
    }

    fn run_flash(&mut self) {
        let Some(approved) = self.pending_digest.clone() else {
            self.hint = "没有可核对的产物摘要，没有烧录。".into();
            return;
        };
        let (Some(port), Some(chip)) = (self.pending_port.clone(), self.chip.clone()) else {
            self.hint = "串口或芯片不在了，没有烧录。".into();
            return;
        };
        let build = std::path::Path::new(&self.workspace).join("build");
        let offer = match livstyle_esp::flash_offer(&build, &port.display().to_string(), &chip) {
            Ok(offer) => offer,
            Err(err) => {
                self.hint = format!("没有烧录：{err}");
                return;
            }
        };
        let toolchain = livstyle_esp::Toolchain::detect(None);
        let Some(install) = toolchain.selected() else {
            self.hint = "没有找到 ESP-IDF，没有烧录。".into();
            return;
        };
        let mut runner = livstyle_esp::EspRunner::for_install(
            "esp",
            install,
            Box::new(livstyle_esp::SystemExec),
        );
        let mut ports = livstyle_esp::PortMap::new();
        ports.insert(
            livstyle_hardware::DeviceId::new(port.display().to_string()),
            port.clone(),
        );
        runner.set_ports(ports);
        let task = if self.selected.is_empty() {
            "unscoped".to_string()
        } else {
            self.selected.clone()
        };
        match livstyle_esp::commit_flash(&mut runner, &offer, &approved, &task) {
            Ok(digest) => {
                self.build_log.push(format!("烧录写入完成，摘要 {digest}"));
                self.finish_flash_log(&port, &task, &digest);
            }
            Err(err) => {
                self.hint = err.clone();
                self.build_log.push(err);
            }
        }
    }

    fn finish_flash_log(&mut self, port: &std::path::Path, task: &str, digest: &str) {
        #[cfg(unix)]
        let captured = livstyle_esp::read_log(port, 115_200, Duration::from_secs(3), 64 * 1024)
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
            .unwrap_or_else(|err| format!("没有采到启动日志：{err}"));
        #[cfg(not(unix))]
        let captured = "这个平台还不能采启动日志。".to_string();
        self.build_log.push(captured.clone());
        if livstyle_esp::boot_log_acceptable(&captured).is_ok() {
            let root = data_dir().join("evidence");
            let run = digest.chars().take(12).collect::<String>();
            match livstyle_esp::write_evidence(&root, task, &run, &captured, digest) {
                Ok(dir) => self.hint = format!("烧录完成，证据在 {}", dir.display()),
                Err(err) => self.hint = format!("日志通过了，证据没有写上：{err}"),
            }
            let _ = self.svc.transition(task, TaskStatus::Verifying);
            let _ = self.svc.transition(task, TaskStatus::Succeeded);
        } else {
            self.hint = "烧录已写入，启动日志没有通过验证。".into();
            let _ = self.svc.transition(task, TaskStatus::Failed);
        }
    }

    fn primary(&mut self) {
        let kind = self.card.as_ref().map(|c| c.kind);
        match kind {
            Some(CardKind::Confirm) => {
                if let Some(id) = self.pending_id.take() {
                    let _ = self.svc.settle(&id, "applied");
                }
                if let Ok(task) = self.svc.force_replan(&self.selected) {
                    self.hint = format!(
                        "已按确认执行，任务状态 {}。",
                        status_zh(&format!("{:?}", task.status))
                    );
                }
                self.card = None;
            }
            Some(CardKind::Flash) => {
                self.card = None;
                self.run_flash();
            }
            Some(CardKind::Unknown) => {
                self.hint = "回读校验需要开发板在线，并且不会自动重烧。".into();
                self.card = None;
            }
            Some(CardKind::Identify) => self.run_identify(),
            _ => self.card = None,
        }
    }

    fn secondary(&mut self) {
        let kind = self.card.as_ref().map(|c| c.kind);
        match kind {
            Some(CardKind::Confirm) => {
                if let Some(id) = self.pending_id.take() {
                    let _ = self.svc.settle(&id, "queued");
                }
                self.hint = "已改成补充上下文，排队到下一次发送。".into();
                self.card = None;
            }
            Some(CardKind::Flash) => {
                self.hint = "已拒绝这次烧录。".into();
                self.card = None;
            }
            Some(CardKind::Unknown) => {
                self.hint = "已记为成功。这不会触发新的烧录。".into();
                self.card = None;
            }
            Some(CardKind::Identify) => {
                self.card = None;
                self.pending_port = None;
                self.refresh_boards();
            }
            _ => self.card = None,
        }
    }

    fn attach_octos(&mut self) {
        let url = std::env::var("LIVSTYLE_OCTOS_URL")
            .unwrap_or_else(|_| "ws://127.0.0.1:50080/api/ui-protocol/ws".into());
        let token = std::env::var("LIVSTYLE_OCTOS_TOKEN").unwrap_or_else(|_| "123456".into());
        let client = match OctosClient::connect(&url, &token) {
            Ok(client) => client,
            Err(err) => {
                self.octos_status = format!("Octos  没连上 {url}");
                self.say(format!(
                    "Octos：没有连上 {url}：{err}。先在本机跑 octos serve，对话才能交给 agent。"
                ));
                return;
            }
        };
        match client.hello() {
            Ok(hello) => {
                let caps = ServerCaps::from_hello(&hello);
                self.octos_status = format!("Octos  {} 已连接", hello.server_version);
                self.say(format!(
                    "Octos：已连接 {}。steer={}。之后的对话发给这个 agent，不再只做启动探测。",
                    hello.server_version, caps.steer
                ));
                self.octos = Some(client);
                self.ensure_session();
            }
            Err(err) => {
                self.octos_status = "Octos  握手失败".into();
                self.say(format!("Octos：连上了，握手失败：{err}"));
            }
        }
    }

    fn session_cwd(&self) -> String {
        if let Some(dir) = self.workspace_dir() {
            return dir.to_string();
        }
        std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| ".".into())
    }

    fn ensure_session(&mut self) -> bool {
        if self.octos.is_none() {
            self.attach_octos();
        }
        if self.octos.is_none() {
            return false;
        }
        let profile = "livstyle";
        let create_err = self
            .octos
            .as_ref()
            .and_then(|client| client.create_profile(profile).err());
        if let Some(err) = create_err {
            self.say(format!("Octos：建 profile 没有成功：{err}"));
        }
        let session = self.octos_session_id();
        let cwd = self.session_cwd();
        let opened = self
            .octos
            .as_ref()
            .expect("octos")
            .open_session(&session, profile, &cwd);
        match opened {
            Ok(_) => {
                self.session_id = Some(session.clone());
                self.bind_selected("octos", "session", &session);
                self.octos_status = format!("Octos  会话已开  {cwd}");
                self.say(format!("Octos：会话 {session} 已打开，工作目录是 {cwd}。"));
                true
            }
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("\"missing\":\"llm\"") || msg.contains("profile_unconfigured") {
                    self.bind_local_model(profile, &session, &cwd)
                } else {
                    self.octos_status = "Octos  会话没打开".into();
                    self.say(format!("Octos：session/open 失败：{msg}"));
                    false
                }
            }
        }
    }

    fn bind_local_model(&mut self, profile: &str, session: &str, cwd: &str) -> bool {
        let model = match pick_ollama_model() {
            Ok(model) => model,
            Err(err) => {
                self.octos_status = "Octos  还没有可用模型".into();
                self.say(format!("Octos：profile 还没有 LLM。{err}"));
                return false;
            }
        };
        let upserted = self.octos.as_ref().and_then(|client| {
            client
                .upsert_primary_llm(profile, "ollama", &model, "http://127.0.0.1:11434/v1")
                .err()
        });
        if let Some(err) = upserted {
            self.say(format!(
                "Octos：没有把 {model} 写成 profile 的主模型：{err}"
            ));
            return false;
        }
        self.say(format!(
            "Octos：profile livstyle 的主模型设为 Ollama {model}。这个大小能放进当前内存。"
        ));
        let open_err = self
            .octos
            .as_ref()
            .and_then(|client| client.open_session(session, profile, cwd).err());
        if let Some(err) = open_err {
            self.octos_status = "Octos  模型已保存，会话仍没打开".into();
            self.say(format!(
                "Octos：模型已写入，session/open 仍然失败：{err}。如果服务缓存了旧 profile，重启一次 octos serve 后再发一句。"
            ));
            false
        } else {
            self.session_id = Some(session.to_string());
            self.bind_selected("octos", "session", session);
            self.octos_status = format!("Octos  {model}");
            self.say(format!("Octos：会话 {session} 已打开，工作目录是 {cwd}。"));
            true
        }
    }

    fn ask_octos(&mut self, text: &str, rx: &Receiver<Cmd>) {
        self.busy = true;
        self.turn_task = Some(self.selected.clone());
        self.live = self.transcript.clone();
        let wanted = self.octos_session_id();
        if session_align(self.session_id.as_deref(), &wanted) == SessionAlign::Reopen {
            self.active_turn = None;
            self.session_id = None;
        }
        if self.session_id.is_none() && !self.ensure_session() {
            self.hint = "Octos 会话没有打开。".into();
            self.finish_turn(rx);
            return;
        }
        let Some(session) = self.session_id.clone() else {
            self.finish_turn(rx);
            return;
        };
        let turn_id = new_turn_id(self.seq);
        let mut started = self.dispatch_turn(&session, &turn_id, text);
        if started
            .as_ref()
            .err()
            .is_some_and(|err| link_failure_text(err))
        {
            self.say("Octos：连接断了，重连一次。".into());
            self.octos = None;
            self.attach_octos();
            if self.ensure_session() {
                if let Some(session) = self.session_id.clone() {
                    started = self.dispatch_turn(&session, &turn_id, text);
                }
            }
        }
        if let Err(err) = started {
            self.active_turn = None;
            self.say(format!("Octos：turn 没有发出去：{err}"));
            self.hint = "没有交给 Octos。".into();
            self.finish_turn(rx);
            return;
        }
        self.advance_to_running();
        self.say("Octos：".into());
        let line = self.transcript.len() - 1;
        let mut acc = String::new();
        let wait_started = Instant::now();
        self.hint = "正在等 Octos。".into();
        let snap = self.snapshot();
        publish(&self.tx, snap);
        loop {
            let notes = self
                .octos
                .as_ref()
                .map(OctosClient::drain)
                .unwrap_or_default();
            let mut finished = false;
            for note in notes {
                let beats = octos_beats(&note);
                if self.apply_beats("Octos", &beats, line, &mut acc) {
                    self.active_turn = None;
                    finished = true;
                    break;
                }
            }
            let snap = self.snapshot();
            publish(&self.tx, snap);
            if finished {
                break;
            }
            if self.poll_turn(rx) == TurnCtrl::Cancel {
                if let (Some(client), Some(session), Some(turn)) = (
                    self.octos.as_ref(),
                    self.session_id.as_deref(),
                    self.active_turn.as_deref(),
                ) {
                    if let Err(err) = client.interrupt(session, turn) {
                        self.hint = format!("停止 Octos 没有发出去：{err}");
                    }
                }
                self.active_turn = None;
                let interrupt_hint = self.hint.clone();
                self.cancel_selected();
                if interrupt_hint.starts_with("停止 Octos") {
                    self.hint = interrupt_hint;
                }
                self.say("livstyle：这一轮已取消，没有记成成功。".into());
                break;
            }
            if wait_started.elapsed() > Duration::from_secs(180) {
                self.active_turn = None;
                self.mark_unsettled();
                self.say(
                    "Octos：超过 3 分钟还没有结束这一轮。任务标成未完成，没有另开会话。".into(),
                );
                self.hint = "这一轮已停下。".into();
                break;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        self.finish_turn(rx);
    }

    fn ask_cursor(&mut self, text: &str, rx: &Receiver<Cmd>) {
        self.busy = true;
        self.turn_task = Some(self.selected.clone());
        self.live = self.transcript.clone();
        self.hint = "正在连接 Cursor。".into();
        let snap = self.snapshot();
        publish(&self.tx, snap);
        let agent_id = match self.ensure_cursor_agent() {
            Ok(id) => id,
            Err(err) => {
                self.say(format!("Cursor：没有开始这一轮：{err}"));
                self.hint = "没有交给 Cursor。".into();
                self.finish_turn(rx);
                return;
            }
        };
        let stream = self
            .cursor
            .as_ref()
            .ok_or_else(|| "Cursor 连接不在了".to_string())
            .and_then(|link| {
                link.client
                    .send(&agent_id, text, None)
                    .map_err(|e| e.to_string())
            });
        let stream = match stream {
            Ok(stream) => stream,
            Err(err) => {
                self.say(format!("Cursor：发送失败：{err}"));
                self.hint = "没有交给 Cursor。".into();
                self.finish_turn(rx);
                return;
            }
        };
        self.advance_to_running();
        self.say("Cursor：".into());
        let line = self.transcript.len() - 1;
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for event in stream {
                if event_tx.send(event).is_err() {
                    break;
                }
            }
        });
        let mut acc = String::new();
        let mut run_id = None;
        let wait_started = Instant::now();
        self.hint = "Cursor 正在写代码。".into();
        let snap = self.snapshot();
        publish(&self.tx, snap);
        loop {
            match event_rx.recv_timeout(Duration::from_millis(150)) {
                Ok(Ok(event)) => {
                    if run_id.is_none() {
                        run_id = event_run_id(&event);
                        if let Some(id) = &run_id {
                            self.record_run(id);
                        }
                    }
                    let beats = cursor_beats(&event);
                    if self.apply_beats("Cursor", &beats, line, &mut acc) {
                        break;
                    }
                }
                Ok(Err(err)) => {
                    self.say(format!("Cursor：这一轮失败：{err}"));
                    self.hint = "Cursor 返回了错误。".into();
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    if acc.trim().is_empty() {
                        self.transcript[line] = "Cursor：这一轮结束了，上面是执行步骤。".into();
                    }
                    self.hint = "Cursor 已回复。".into();
                    break;
                }
            }
            if self.poll_turn(rx) == TurnCtrl::Cancel {
                if let Some(id) = &run_id {
                    if let Some(link) = self.cursor.as_ref().filter(|link| link.caps.interrupt) {
                        if let Err(err) = link.client.cancel_run(id, Some(&agent_id)) {
                            self.hint = format!("取消 Cursor 没有成功：{err}");
                        }
                    }
                }
                let interrupt_hint = self.hint.clone();
                self.cancel_selected();
                if interrupt_hint.starts_with("取消 Cursor") {
                    self.hint = interrupt_hint;
                }
                self.say("livstyle：这一轮已取消，没有记成成功。".into());
                break;
            }
            let snap = self.snapshot();
            publish(&self.tx, snap);
            if wait_started.elapsed() > Duration::from_secs(20 * 60) {
                if let Some(id) = &run_id {
                    if let Some(link) = self.cursor.as_ref() {
                        if let Err(err) = link.client.cancel_run(id, Some(&agent_id)) {
                            self.hint = format!("取消 Cursor 没有成功：{err}");
                        }
                    }
                }
                self.mark_unsettled();
                self.say("Cursor：超过 20 分钟还没有结束这一轮。任务标成未完成。".into());
                self.hint = "这一轮已停下。".into();
                break;
            }
        }
        self.finish_turn(rx);
    }

    fn apply_beats(
        &mut self,
        who: &str,
        beats: &[TurnBeat],
        line: usize,
        acc: &mut String,
    ) -> bool {
        let mut terminal = false;
        for beat in beats {
            match beat {
                TurnBeat::Append(piece) => {
                    acc.push_str(piece);
                    self.write_turn_line(Some(line), format!("{who}：{acc}"));
                }
                TurnBeat::Replace(text) => {
                    if text.starts_with(acc.as_str()) || acc.is_empty() {
                        *acc = text.clone();
                    } else if !acc.contains(text.as_str()) {
                        acc.push_str(text);
                    }
                    self.write_turn_line(Some(line), format!("{who}：{acc}"));
                }
                TurnBeat::Step(step) => self.say(format!("{who} · {step}")),
                TurnBeat::Done => {
                    if acc.trim().is_empty() {
                        self.write_turn_line(
                            Some(line),
                            format!("{who}：这一轮结束了，上面是执行步骤。"),
                        );
                    }
                    self.hint = format!("{who} 已回复。");
                    terminal = true;
                }
                TurnBeat::Fail(message) => {
                    self.say(format!("{who}：这一轮失败：{message}"));
                    self.hint = format!("{who} 返回了错误。");
                    terminal = true;
                }
            }
        }
        terminal
    }

    fn poll_turn(&mut self, rx: &Receiver<Cmd>) -> TurnCtrl {
        loop {
            match rx.try_recv() {
                Ok(Cmd::Cancel) | Ok(Cmd::Pause) => return TurnCtrl::Cancel,
                Ok(Cmd::UseAgent(agent)) => self.use_agent(agent),
                Ok(Cmd::Select(index)) => self.select_at(index),
                Ok(Cmd::Send(text)) => {
                    if self.note_busy_text(text) {
                        return TurnCtrl::Cancel;
                    }
                }
                Ok(other) => self.deferred.push(other),
                Err(TryRecvError::Empty) => return TurnCtrl::Continue,
                Err(TryRecvError::Disconnected) => return TurnCtrl::Cancel,
            }
        }
    }

    fn note_busy_text(&mut self, text: String) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        self.say(format!("你：{text}"));
        match busy_text(self.agent) {
            BusyText::Queue => {
                self.followups.push_back(text.to_string());
                self.say("livstyle：这一轮还在进行，这句话排到下一轮。".into());
                self.hint = "已排到下一轮。".into();
                false
            }
            BusyText::Steer => {
                if self.record_intervention(text) {
                    self.hint = "已取消。没有把它记成成功。".into();
                    self.say("livstyle：这句话取消了任务，没有插进当前轮。".into());
                    return true;
                }
                let Some(session) = self.session_id.clone() else {
                    self.followups.push_back(text.to_string());
                    self.say("livstyle：会话不在，这句话排到下一轮。".into());
                    return false;
                };
                self.seq += 1;
                let turn_id = new_turn_id(self.seq);
                if let Err(err) = self.dispatch_turn(&session, &turn_id, text) {
                    self.followups.push_back(text.to_string());
                    self.say(format!("livstyle：没有插进这一轮：{err}。已排到下一轮。"));
                }
                false
            }
        }
    }

    fn octos_session_id(&self) -> String {
        if let Some(stored) = self
            .selected_task()
            .and_then(|task| task.runtime_session_id)
            .filter(|id| id.starts_with("livstyle:local:") && id.matches(':').count() >= 2)
        {
            return stored;
        }
        let chat: String = self
            .selected
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let chat = if chat.is_empty() {
            "desk".to_string()
        } else {
            chat
        };
        format!("livstyle:local:{chat}")
    }

    fn selected_task(&self) -> Option<livstyle_domain::ProductTask> {
        if self.selected.is_empty() {
            return None;
        }
        self.svc.task(&self.selected).ok().map(|(_, task)| task)
    }

    fn bind_selected(&mut self, runtime: &str, kind: &str, external_id: &str) {
        if self.selected.is_empty() {
            return;
        }
        if let Err(err) = self
            .svc
            .bind_runtime(&self.selected, runtime, kind, external_id)
        {
            self.hint = format!("运行时 id 没有记下：{err}");
        }
    }

    fn record_run(&mut self, run_id: &str) {
        if self.selected.is_empty() {
            return;
        }
        if let Err(err) = self
            .svc
            .record_mapping(&self.selected, "cursor", "run", run_id)
        {
            self.hint = format!("Cursor run 没有记下：{err}");
        }
    }

    fn advance_to_running(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        if task.status == TaskStatus::Queued {
            let _ = self.svc.transition(&self.selected, TaskStatus::Planning);
        }
        if let Err(err) = self.svc.transition(&self.selected, TaskStatus::Running) {
            self.hint = format!("任务没有进入运行中：{err}");
        }
    }

    fn mark_unsettled(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let next = if task.status.can_transition_to(TaskStatus::OutcomeUnknown) {
            TaskStatus::OutcomeUnknown
        } else if task.status.can_transition_to(TaskStatus::Failed) {
            TaskStatus::Failed
        } else {
            self.hint = format!(
                "任务停在 {}，不能标成未完成。",
                status_zh(&format!("{:?}", task.status))
            );
            return;
        };
        match self.svc.transition(&self.selected, next) {
            Ok(task) => {
                self.hint = format!("任务状态 {}。", status_zh(&format!("{:?}", task.status)));
            }
            Err(err) => self.hint = format!("任务状态没有更新：{err}"),
        }
    }

    fn restore_octos_sessions(&mut self) {
        if self.octos.is_none() {
            return;
        }
        let Ok(ids) = self.svc.task_ids() else {
            return;
        };
        for id in ids {
            let Ok((_, task)) = self.svc.task(&id) else {
                continue;
            };
            if task.status.is_terminal() {
                continue;
            }
            let Some(session) = task
                .runtime_session_id
                .filter(|id| id.starts_with("livstyle:local:") && id.matches(':').count() >= 2)
            else {
                continue;
            };
            match self.octos.as_ref().map(|client| client.hydrate(&session)) {
                Some(Ok(_)) => {
                    self.say(format!("Octos：已恢复会话 {session}。"));
                }
                Some(Err(err)) => {
                    self.octos_status = format!("Octos  会话 {session} 没有恢复");
                    self.say(format!("Octos：hydrate 失败，没有另开会话：{err}"));
                }
                None => return,
            }
        }
    }

    fn observe_unfinished_runs(&mut self) {
        let Ok(ids) = self.svc.task_ids() else {
            return;
        };
        for id in ids {
            let Ok((_, task)) = self.svc.task(&id) else {
                continue;
            };
            if task.status.is_terminal() {
                continue;
            }
            let Ok(Some(run_id)) = self.svc.mapping(&id, "cursor", "run") else {
                continue;
            };
            let Some(link) = self.cursor.as_ref() else {
                return;
            };
            match link.client.observe_run(&run_id, None) {
                Ok(stream) => {
                    self.say(format!("Cursor：继续观察未完成的 run {run_id}。"));
                    self.fold_observe(stream);
                }
                Err(err) => {
                    self.hint = format!("没有接上 Cursor run {run_id}：{err}");
                }
            }
        }
    }

    fn fold_observe(&mut self, stream: livstyle_cursor::RunStream) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for event in stream {
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
        let start = Instant::now();
        let mut acc = String::new();
        self.say("Cursor：".into());
        let line = self.transcript.len() - 1;
        while start.elapsed() < Duration::from_secs(2) {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(Ok(event)) => {
                    let beats = cursor_beats(&event);
                    if self.apply_beats("Cursor", &beats, line, &mut acc) {
                        break;
                    }
                }
                Ok(Err(err)) => {
                    self.hint = format!("观察 Cursor run 失败：{err}");
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn use_agent(&mut self, next: CodeAgent) {
        match plan_switch(self.busy, self.agent, next) {
            SwitchPlan::Already => {
                self.hint = format!("已经在用 {}。", next.label());
            }
            SwitchPlan::Now => self.apply_agent(next),
            SwitchPlan::AfterTurn => {
                self.pending_agent = Some(next);
                self.say(format!(
                    "livstyle：这一轮还在进行。结束后改用 {}。",
                    next.label()
                ));
                self.hint = format!("结束后改用 {}。", next.label());
            }
        }
    }

    fn apply_agent(&mut self, agent: CodeAgent) {
        self.agent = agent;
        self.pending_agent = None;
        if !self.selected.is_empty() {
            let external = match agent {
                CodeAgent::Cursor => self.cursor.as_ref().and_then(|link| link.agent_id.clone()),
                CodeAgent::Octos => self.session_id.clone(),
            };
            if let Some(external) = external {
                let kind = if agent == CodeAgent::Cursor {
                    "agent"
                } else {
                    "session"
                };
                let _ = self
                    .svc
                    .bind_runtime(&self.selected, agent.runtime_id(), kind, &external);
            }
        }
        self.say(format!("livstyle：之后的对话交给 {}。", agent.label()));
        self.hint = if agent.can_steer() {
            "进行中的新一句会插进当前轮。".into()
        } else {
            "进行中的新一句会排到这一轮结束。".into()
        };
    }

    fn finish_turn(&mut self, rx: &Receiver<Cmd>) {
        if let Some(id) = self.turn_task.clone() {
            let lines = if self.selected == id {
                self.transcript.clone()
            } else {
                self.live.clone()
            };
            let _ = self.svc.set_transcript(&id, &lines);
        } else if !self.selected.is_empty() {
            let _ = self.svc.set_transcript(&self.selected, &self.transcript);
        }
        let cancelled = self
            .turn_task
            .as_deref()
            .and_then(|id| self.svc.task(id).ok())
            .is_some_and(|(_, task)| task.status == TaskStatus::Cancelled);
        self.turn_task = None;
        self.busy = false;
        if let Some(agent) = self.pending_agent.take() {
            self.apply_agent(agent);
        }
        let followups = std::mem::take(&mut self.followups);
        if cancelled {
            self.hint = "已取消。排队的下一句没有发出去。".into();
        } else {
            for text in followups {
                self.send(&text, rx);
            }
        }
        let deferred = std::mem::take(&mut self.deferred);
        for cmd in deferred {
            self.handle(cmd, rx);
        }
    }

    fn dispatch_turn(&mut self, session: &str, turn_id: &str, text: &str) -> Result<(), String> {
        let Some(client) = self.octos.as_ref() else {
            return Err("连接不在了".into());
        };
        let result = if let Some(current) = self.active_turn.clone() {
            client.steer(session, Some(&current), text)
        } else {
            client.turn_start(session, turn_id, text)
        };
        match result {
            Ok(_) => {
                if self.active_turn.is_none() {
                    self.active_turn = Some(turn_id.to_string());
                }
                Ok(())
            }
            Err(err) if self.active_turn.is_some() => {
                self.active_turn = None;
                let client = self.octos.as_ref().ok_or_else(|| err.to_string())?;
                client
                    .turn_start(session, turn_id, text)
                    .map_err(|e| e.to_string())?;
                self.active_turn = Some(turn_id.to_string());
                Ok(())
            }
            Err(err) => Err(err.to_string()),
        }
    }

    fn workspace_dir(&self) -> Option<&str> {
        let path = std::path::Path::new(&self.workspace);
        if path.is_dir() {
            Some(&self.workspace)
        } else {
            None
        }
    }

    fn set_workspace(&mut self, path: String) {
        let path = path.trim().to_string();
        if std::path::Path::new(&path).is_dir() {
            self.workspace = path.clone();
            save_workspace(&path);
            self.say(format!(
                "livstyle：代码目录已设为 {path}。之后的修改写在这里。"
            ));
            self.hint = "代码目录已更新。".into();
            self.session_id = None;
            self.active_turn = None;
            self.ensure_session();
        } else {
            self.say(
                "livstyle：这个路径不是已有目录。请填一个存在的项目文件夹，或点「浏览」。".into(),
            );
            self.hint = "目录不存在。".into();
        }
    }

    fn browse_workspace(&mut self) {
        match pick_directory() {
            Some(path) => self.set_workspace(path),
            None => {
                self.say(
                    "livstyle：没有选到目录。可以直接把路径填进左边的输入框，再点「使用」。".into(),
                );
            }
        }
    }

    fn refresh_boards(&mut self) {
        let text = scan_reply();
        self.board_status = board_line();
        self.note_board(text);
        self.say("livstyle：串口扫描写在板子调试里，没有复位开发板。".into());
        self.hint = "已扫描串口，没有复位开发板。".into();
    }

    fn ask_identify(&mut self) {
        let found = livstyle_esp::enumerate();
        if found.is_empty() {
            self.board_status = "开发板  没有串口".into();
            self.note_board("没有发现串口，没有打开端口，开发板不会复位。请换一根数据线后再识别。");
            self.say("livstyle：没有发现串口。详情在板子调试里。".into());
            self.hint = "没有串口。".into();
            return;
        }
        let dev = &found[0];
        self.pending_port = Some(dev.port.clone());
        self.card = Some(Card {
            title: "识别芯片会复位开发板".into(),
            body: format!(
                "将打开 {}（{:04x}:{:04x}），进入下载模式再启动。正在运行的固件会重启。\nUSB 描述符不能区分 C3、C5、C6 和 S3，只有这一步能读出型号。",
                dev.port.display(),
                dev.vid,
                dev.pid
            ),
            primary: "复位并识别".into(),
            secondary: "只看串口".into(),
            dismiss: "取消".into(),
            kind: CardKind::Identify,
        });
        self.note_board(format!(
            "等待确认：打开 {}（{:04x}:{:04x}）会复位开发板。",
            dev.port.display(),
            dev.vid,
            dev.pid
        ));
        self.hint = "确认后才会复位。".into();
    }

    fn run_identify(&mut self) {
        let Some(port) = self.pending_port.clone() else {
            self.card = None;
            return;
        };
        self.card = None;
        self.say(format!(
            "livstyle：正在复位 {} 并读取芯片，请稍等。",
            port.display()
        ));
        self.hint = "正在识别，开发板会重启。".into();
        publish(&self.tx, self.snapshot());
        match identify_chip(&port) {
            Ok(chip) => {
                self.board_status = format!("开发板  {chip}");
                self.chip = Some(chip.clone());
                self.note_board(format!("{} 是 {chip}。固件已重新启动。", port.display()));
                self.say(format!(
                    "livstyle：{} 是 {chip}。详情在板子调试里。",
                    port.display()
                ));
                self.hint = "已读出芯片。".into();
            }
            Err(err) => {
                let brief = err
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or(err.as_str());
                self.note_board(format!("没有读出芯片。{brief}"));
                self.say("livstyle：没有读出芯片。详情在板子调试里。".into());
                self.hint = "识别没有完成。".into();
            }
        }
    }

    fn delete_at(&mut self, index: usize) {
        let Some(id) = self
            .svc
            .task_ids()
            .ok()
            .and_then(|ids| ids.into_iter().nth(index))
        else {
            return;
        };
        match self.svc.delete_task(&id) {
            Ok(()) => {
                if self.selected == id {
                    self.selected = self
                        .svc
                        .task_ids()
                        .ok()
                        .and_then(|ids| ids.into_iter().next())
                        .unwrap_or_default();
                }
                self.hint = format!("已删除任务 {id}。");
            }
            Err(e) => self.hint = format!("没有删掉：{e}"),
        }
    }

    fn new_task(&mut self) {
        self.seq += 1;
        let id = format!("task-{}", self.seq);
        if self
            .svc
            .create_task(&id, "未命名任务", self.agent.runtime_id())
            .is_ok()
        {
            self.selected = id;
            self.hint = "已新建草稿任务。".into();
        }
    }

    fn snapshot(&self) -> Snapshot {
        let ids = self.svc.task_ids().unwrap_or_default();
        let mut tasks = Vec::new();
        for id in &ids {
            let Ok((_, task)) = self.svc.task(id) else {
                continue;
            };
            let status = format!("{:?}", task.status);
            tasks.push(TaskLine {
                title: format!("{}  {}", task.workspace_id, task.task_id),
                status: status_zh(&status).into(),
                mark: if *id == self.selected { "●" } else { "○" }.into(),
            });
        }
        Snapshot {
            runtime: self.runtime_label(),
            tasks,
            transcript: self.transcript.join("\n\n"),
            workspace: self.workspace.clone(),
            cursor_status: self.cursor_status.clone(),
            octos_status: self.octos_status.clone(),
            board_status: self.board_status.clone(),
            board_log: self.board_log.join("\n\n"),
            build_log: self.build_log.join("\n"),
            plan: self.plan.clone(),
            hint: self.hint.clone(),
            card: self.card.clone(),
        }
    }
}

fn seed(svc: &mut Service) {
    let _ = svc.create_task("wifi-rssi", "WiFi RSSI", "cursor");
    let _ = svc.transition("wifi-rssi", TaskStatus::Planning);
    let _ = svc.transition("wifi-rssi", TaskStatus::Running);
    let _ = svc.bind_runtime("wifi-rssi", "cursor", "agent", "未连接");
    let _ = svc.save_plan(
        "wifi-rssi",
        &vec![
            "[完成] 分析需求",
            "[完成] 生成 sdkconfig.defaults",
            "[进行] 构建 (idf.py build)   free",
            "[待做] 审批烧录            R2",
            "[待做] 烧录                不可中断",
            "[待做] 启动日志采集",
            "[待做] 断言 & 报告",
        ],
        "改为 5GHz",
    );
    let _ = svc.create_task("psram", "PSRAM 修复", "cursor");
    let _ = svc.transition("psram", TaskStatus::Planning);
    let _ = svc.transition("psram", TaskStatus::Running);
    let _ = svc.transition("psram", TaskStatus::Paused);
    let _ = svc.create_task("zigbee", "Zigbee 传感", "cursor");
    let caps = RuntimeCaps {
        steer: true,
        interrupt: true,
    };
    if let Ok(out) = svc.intervene(
        "wifi-rssi",
        "seed-5ghz",
        "改为 5GHz",
        &RuleJudge,
        caps,
        Stage::First,
    ) {
        if matches!(
            out.resolution,
            Resolution::EscalateToSystemTwo | Resolution::AskUser { .. }
        ) {
            let _ = svc.settle("seed-5ghz", "applied");
        }
    }
    let _ = svc.intervene(
        "wifi-rssi",
        "seed-port",
        "串口是 ACM1",
        &RuleJudge,
        caps,
        Stage::First,
    );
    let _ = svc.settle("seed-port", "queued");
}

fn confirm_card(text: &str) -> Card {
    Card {
        title: "需要确认".into(),
        body: format!(
            "“{text}”\n判断置信度不够，或碰到了硬件参数。\n按此执行会取消当前步骤并重编排；不会自动烧录。"
        ),
        primary: "按此执行".into(),
        secondary: "仅补充为上下文".into(),
        dismiss: "取消".into(),
        kind: CardKind::Confirm,
    }
}

fn notice(title: &str, body: &str) -> Card {
    Card {
        title: title.into(),
        body: body.into(),
        primary: "关闭".into(),
        secondary: "关闭".into(),
        dismiss: "关闭".into(),
        kind: CardKind::Settings,
    }
}

fn flash_card() -> Card {
    Card {
        title: "只记录烧录批准，不写设备".into(),
        body: "这一步不会调用烧录器，也不会复位开发板。\n记下的摘要是占位值，还不能对上一次真实产物。\n真正烧录要等设备和产物哈希绑在同一次审批上。".into(),
        primary: "只记账".into(),
        secondary: "拒绝".into(),
        dismiss: "关闭".into(),
        kind: CardKind::Flash,
    }
}

fn unknown_card() -> Card {
    Card {
        title: "烧录结果未知".into(),
        body: "写入过程中如果断开，设备状态无法确定。\n系统不会自动重烧。".into(),
        primary: "回读校验 (R0)".into(),
        secondary: "我确认已成功".into(),
        dismiss: "标记失败并重新审批".into(),
        kind: CardKind::Unknown,
    }
}

fn settings_card() -> Card {
    let key = CredentialStore::default_location()
        .ok()
        .and_then(|store| store.get(CURSOR_API_KEY).ok().flatten())
        .map(|secret| format!("已配置  指纹 {}", secret.fingerprint()))
        .unwrap_or_else(|| "未配置。请在命令行执行 livstyle auth cursor".into());
    Card {
        title: "设置".into(),
        body: format!(
            "Cursor API Key   {key}\n写入 .env 的 CURSOR_API_KEY= ，不要贴到聊天里\nCursor Bridge    v1.0.38  sdk.v1\nJudge            规则判断（界面内）；Ollama 用 livstyle judge eval\nESP-IDF          livstyle toolchain\n应用迭代         底座刷过之后走局域网热替换，不每次 USB 重烧"
        ),
        primary: "关闭".into(),
        secondary: "关闭".into(),
        dismiss: "关闭".into(),
        kind: CardKind::Settings,
    }
}

impl State {
    fn runtime_label(&self) -> String {
        match self.pending_agent {
            Some(next) => format!(
                "当前 agent：{}，结束后改用 {}",
                self.agent.label(),
                next.label()
            ),
            None => format!("当前 agent：{}", self.agent.label()),
        }
    }
}

fn board_line() -> String {
    let n = livstyle_esp::enumerate().len();
    if n == 0 {
        "开发板  没有串口".into()
    } else {
        format!("开发板  看到 {n} 个串口")
    }
}

fn scan_reply() -> String {
    let found = livstyle_esp::enumerate();
    if found.is_empty() {
        return "扫过 USB，没有打开串口，开发板不会复位。没有看到 ttyACM 或 ttyUSB。请确认是数据线，并且当前用户在 dialout 组。".into();
    }
    let rows = found
        .iter()
        .map(|d| {
            format!(
                "{}  {:04x}:{:04x}  {}",
                d.port.display(),
                d.vid,
                d.pid,
                d.bridge.profile_key()
            )
        })
        .collect::<Vec<_>>()
        .join("；");
    format!("扫过 USB，没有打开串口，开发板不会复位。看到：{rows}。USB 描述符不能区分 C3、C5、C6 和 S3。要读型号请点「识别芯片」，那一步会复位。")
}

fn link_failure_text(message: &str) -> bool {
    message.contains("connection closed") || message.starts_with("timed out waiting")
}

fn wants_identify(text: &str) -> bool {
    text.contains("识别") && (text.contains("芯片") || text.contains("型号"))
}

fn asks_board(text: &str) -> bool {
    let chip = text.contains("芯片") || text.contains("开发板") || text.contains("串口");
    let ask = text.contains("检测")
        || text.contains("了没")
        || text.contains("有没有")
        || text.contains("看到")
        || text.contains("连上")
        || text.contains("在不在");
    chip && ask
}

fn data_dir() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let dir = base.join("livstyle");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn load_workspace() -> String {
    std::fs::read_to_string(data_dir().join("workspace.txt"))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn save_workspace(path: &str) {
    let _ = std::fs::write(data_dir().join("workspace.txt"), path);
}

fn pick_directory() -> Option<String> {
    let zenity = std::process::Command::new("zenity")
        .args(["--file-selection", "--directory", "--title=选择代码目录"])
        .output()
        .ok()?;
    if !zenity.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&zenity.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TurnCtrl {
    Continue,
    Cancel,
}

fn event_run_id(event: &RunEvent) -> Option<String> {
    match event {
        RunEvent::Result(summary) if !summary.run_id.is_empty() => Some(summary.run_id.clone()),
        RunEvent::Message { payload, .. } => payload
            .get("run_id")
            .or_else(|| payload.get("runId"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

impl State {
    fn open_cursor(&mut self) {
        match CredentialStore::default_location().and_then(|store| store.get(CURSOR_API_KEY)) {
            Ok(Some(secret)) => {
                self.cursor_status = format!("Cursor  已配置 {}", secret.fingerprint());
                match locate_bridge() {
                    Ok(binary) => {
                        let workspace = self
                            .workspace_dir()
                            .map(PathBuf::from)
                            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                        match self.spawn_cursor(binary, workspace, secret) {
                            Ok(version) => {
                                self.cursor_status =
                                    format!("Cursor  Bridge {} 已接通", version.bridge_version);
                                self.say(format!(
                                    "livstyle：Bridge {} 已接通。发给 Cursor 的需求会在代码目录里执行，过程显示在这条对话里。",
                                    version.bridge_version
                                ));
                            }
                            Err(err) => {
                                self.say(format!("livstyle：找到 Bridge，但没有启动：{err}"))
                            }
                        }
                    }
                    Err(err) => self.say(format!("livstyle：Key 在，Bridge 程序不在：{err}")),
                }
            }
            Ok(None) => {
                self.cursor_status = "Cursor  还没有 Key".into();
                self.say("livstyle：Cursor Key 还没写进 .env。可以先改用 Octos。".into());
            }
            Err(err) => self.say(format!("livstyle：读 Cursor Key 失败：{err}")),
        }
    }

    fn spawn_cursor(
        &mut self,
        binary: PathBuf,
        workspace: PathBuf,
        secret: Secret,
    ) -> Result<VersionInfo, String> {
        self.cursor = None;
        let config = BridgeConfig {
            binary,
            workspace: workspace.clone(),
            state_root: None,
        };
        let bridge = Bridge::spawn(&config, Some(&secret)).map_err(|e| e.to_string())?;
        let client = CursorClient::new(bridge.endpoint().clone(), Some(secret.clone()));
        match client.version() {
            Ok(version) => {
                if let Err(err) = client.me() {
                    let line = account_status_line(&err);
                    self.cursor_status = line.clone();
                    self.say(line.clone());
                    if matches!(err, livstyle_cursor::CursorError::BridgeUnauthenticated) {
                        return Err(line);
                    }
                }
                self.cursor = Some(CursorLink {
                    bridge,
                    client,
                    secret,
                    caps: runtime_caps(&version),
                    model: None,
                    agent_id: None,
                    agent_cwd: None,
                    bridge_cwd: workspace,
                });
                Ok(version)
            }
            Err(err) => Err(err.to_string()),
        }
    }

    fn ensure_cursor_agent(&mut self) -> Result<String, String> {
        let cwd = self
            .workspace_dir()
            .ok_or_else(|| "还没有代码目录".to_string())?
            .to_string();
        let dead = self
            .cursor
            .as_mut()
            .is_some_and(|link| !link.bridge.is_running());
        if dead {
            self.cursor = None;
        }
        let secret = match self.cursor.as_ref().map(|link| link.secret.clone()) {
            Some(secret) => secret,
            None => CredentialStore::default_location()
                .and_then(|store| store.get(CURSOR_API_KEY))
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "Cursor Key 还没写进 .env".to_string())?,
        };
        let same_bridge = self
            .cursor
            .as_ref()
            .is_some_and(|link| link.bridge_cwd == PathBuf::from(&cwd));
        if !same_bridge {
            let binary = locate_bridge().map_err(|e| e.to_string())?;
            self.spawn_cursor(binary, PathBuf::from(&cwd), secret)?;
        }
        let model = {
            let link = self
                .cursor
                .as_mut()
                .ok_or_else(|| "Cursor 没有连上".to_string())?;
            if link.model.is_none() {
                let models = link.client.list_models().map_err(|e| e.to_string())?;
                let id = models
                    .first()
                    .map(|model| model.id.clone())
                    .ok_or_else(|| "Cursor 没有返回可用模型".to_string())?;
                link.model = Some(id);
            }
            link.model.clone().unwrap()
        };
        let stored = if self.selected.is_empty() {
            None
        } else {
            self.svc
                .mapping(&self.selected, "cursor", "agent")
                .map_err(|e| e.to_string())?
        };
        let (live_agent, live_cwd) = self
            .cursor
            .as_ref()
            .map(|link| (link.agent_id.clone(), link.agent_cwd.clone()))
            .unwrap_or((None, None));
        match cursor_agent_choice(
            stored.as_deref(),
            live_agent.as_deref(),
            live_cwd.as_deref(),
            &cwd,
        ) {
            CursorAgentChoice::Reuse(id) => return Ok(id),
            CursorAgentChoice::Resume(id) => {
                let handle = self
                    .cursor
                    .as_ref()
                    .ok_or_else(|| "Cursor 没有连上".to_string())?
                    .client
                    .resume_agent(&id, &model, std::path::Path::new(&cwd))
                    .map_err(|e| format!("没有恢复这个任务的 Cursor agent：{e}"))?;
                if let Some(link) = self.cursor.as_mut() {
                    link.agent_id = Some(handle.agent_id.clone());
                    link.agent_cwd = Some(cwd.clone());
                }
                self.cursor_status = format!("Cursor  {}  {}", model, cwd);
                return Ok(handle.agent_id);
            }
            CursorAgentChoice::Create => {}
        }
        let handle = self
            .cursor
            .as_ref()
            .ok_or_else(|| "Cursor 没有连上".to_string())?
            .client
            .create_agent(&model, std::path::Path::new(&cwd), Some("livstyle"))
            .map_err(|e| e.to_string())?;
        if let Some(link) = self.cursor.as_mut() {
            link.agent_id = Some(handle.agent_id.clone());
            link.agent_cwd = Some(cwd.clone());
        }
        self.cursor_status = format!("Cursor  {}  {}", model, cwd);
        if !self.selected.is_empty() {
            let _ = self
                .svc
                .bind_runtime(&self.selected, "cursor", "agent", &handle.agent_id);
        }
        self.say(format!("Cursor：使用模型 {model}，在 {cwd} 里改代码。"));
        Ok(handle.agent_id)
    }
}

fn new_turn_id(seq: u64) -> String {
    let ticks = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(seq);
    format!(
        "{:08x}-{:04x}-7{:03x}-{:04x}-{:012x}",
        (ticks >> 32) as u32,
        ((ticks >> 16) & 0xffff) as u16,
        (ticks & 0x0fff) as u16,
        0x8000 | ((seq as u16) & 0x0fff),
        seq
    )
}

fn pick_ollama_model() -> Result<String, String> {
    let url = Url::parse("http://127.0.0.1:11434/api/tags").map_err(|e| e.to_string())?;
    let response = http_send(livstyle_http::Request::get(&url)).map_err(|e| e.to_string())?;
    if !response.is_success() {
        return Err(format!("Ollama 返回 {}", response.status));
    }
    let bytes = response.read_all().map_err(|e| e.to_string())?;
    let body: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let models = body
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    const PREFER: &[&str] = &[
        "qwen2.5-coder:7b",
        "qwen2.5-coder:14b",
        "qwen3:8b",
        "qwen3:4b",
        "qwen3:14b",
        "llama3.1:8b",
        "llama3.2:3b",
    ];
    let limit = 12 * 1024 * 1024 * 1024u64;
    let fits = |size: Option<u64>| size.map(|n| n <= limit).unwrap_or(true);
    for wanted in PREFER {
        if models.iter().any(|model| {
            model.get("name").and_then(Value::as_str) == Some(*wanted)
                && fits(model.get("size").and_then(Value::as_u64))
        }) {
            return Ok((*wanted).to_string());
        }
    }
    let mut best: Option<(u64, String)> = None;
    for model in &models {
        let Some(name) = model.get("name").and_then(Value::as_str) else {
            continue;
        };
        let size = model
            .get("size")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        if size > limit {
            continue;
        }
        if best.as_ref().map(|(have, _)| size < *have).unwrap_or(true) {
            best = Some((size, name.to_string()));
        }
    }
    best.map(|(_, name)| name).ok_or_else(|| {
        "本机 Ollama 没有能放进约 12 GiB 的模型。可以执行 ollama pull qwen2.5-coder:7b，然后再发一句。"
            .into()
    })
}

fn status_zh(status: &str) -> &str {
    match status {
        "Running" => "运行中",
        "Paused" => "已暂停",
        "Queued" => "草稿",
        "Planning" => "规划中",
        "Replanning" => "重编排",
        "Cancelled" => "已取消",
        "Succeeded" => "已完成",
        "Failed" => "失败",
        "AwaitingApproval" => "待审批",
        "Blocked" => "受阻",
        "Verifying" => "验证中",
        "OutcomeUnknown" => "结果未知",
        other => other,
    }
}

fn db_path() -> std::path::PathBuf {
    data_dir().join("ui.db")
}
