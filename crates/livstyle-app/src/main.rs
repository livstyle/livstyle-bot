pub use makepad_widgets;

mod board_view;
mod turns;
mod worker;

use std::sync::{mpsc, Mutex};

use makepad_widgets::*;
use turns::CodeAgent;
use worker::{Cmd, Snapshot, TaskLine};

static LINES: Mutex<Vec<TaskLine>> = Mutex::new(Vec::new());

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    let PrimaryButton = Button{
        height: 32
        padding: Inset{left: 14.0 right: 14.0 top: 0.0 bottom: 0.0}
        draw_bg +: {
            color: #x1677FF
            color_hover: #x2b86ff
            color_down: #x0f5ed4
            border_color: #x1677FF
            border_color_hover: #x00C8FF
            border_color_down: #x0f5ed4
            border_radius: 6.0
            border_size: 0.0
        }
        draw_text +: {
            color: #xE6F1FF
            color_hover: #xffffffff
            color_down: #xE6F1FF
            color_focus: #xE6F1FF
            text_style: theme.font_bold{font_size: 13.0}
        }
    }
    let QuietButton = Button{
        height: 30
        padding: Inset{left: 10.0 right: 10.0 top: 0.0 bottom: 0.0}
        draw_bg +: {
            color: #x0A2038
            color_hover: #x0e2a48
            color_down: #x07182C
            border_color: #x173858
            border_color_hover: #x00C8FF
            border_color_down: #x173858
            border_radius: 6.0
            border_size: 1.0
        }
        draw_text +: {
            color: #xE6F1FF
            color_hover: #xffffffff
            color_down: #xE6F1FF
            color_focus: #xE6F1FF
            text_style: theme.font_bold{font_size: 12.0}
        }
    }
    let DangerButton = Button{
        height: 30
        padding: Inset{left: 10.0 right: 10.0 top: 0.0 bottom: 0.0}
        draw_bg +: {
            color: #x2a1520
            color_hover: #x3a1c28
            color_down: #x1a1016
            border_color: #xFF5C75
            border_color_hover: #xFF5C75
            border_color_down: #xFF5C75
            border_radius: 6.0
            border_size: 1.0
        }
        draw_text +: {
            color: #xFF5C75
            color_hover: #xfffff8
            color_down: #xFF5C75
            color_focus: #xFF5C75
            text_style: theme.font_bold{font_size: 12.0}
        }
    }
    let Section = Label{
        draw_text.color: #x8EA8C7
        draw_text.text_style: theme.font_bold{font_size: 11.0}
    }
    let Title = Label{
        draw_text.color: #xE6F1FF
        draw_text.text_style: theme.font_bold{font_size: 15.0}
    }
    let Muted = Label{
        draw_text.color: #x8EA8C7
        draw_text.text_style.font_size: 12.0
    }
    let TaskFace = Button{
        width: Fill height: 36
        align: Align{x: 0.0, y: 0.5}
        padding: Inset{left: 8.0 right: 6.0 top: 0.0 bottom: 0.0}
        draw_bg +: {
            color: #x07182C
            color_hover: #x0A2038
            color_down: #x061326
            color_focus: #x07182C
            border_color: #x07182C
            border_color_hover: #x173858
            border_color_down: #x07182C
            border_color_focus: #x07182C
            border_radius: 6.0
            border_size: 0.0
        }
        draw_text +: {
            color: #xE6F1FF
            color_hover: #xffffffff
            color_down: #xffffffff
            color_focus: #xE6F1FF
            text_style: theme.font_bold{font_size: 12.0}
        }
    }
    let TaskFaceOn = Button{
        width: Fill height: 36
        align: Align{x: 0.0, y: 0.5}
        padding: Inset{left: 8.0 right: 6.0 top: 0.0 bottom: 0.0}
        draw_bg +: {
            color: #x0A2038
            color_hover: #x0e2c4c
            color_down: #x0e2c4c
            color_focus: #x0A2038
            border_color: #x00C8FF
            border_color_hover: #x00C8FF
            border_color_down: #x00C8FF
            border_color_focus: #x00C8FF
            border_radius: 6.0
            border_size: 1.0
        }
        draw_text +: {
            color: #xE6F1FF
            color_hover: #xffffffff
            color_down: #xffffffff
            color_focus: #xE6F1FF
            text_style: theme.font_bold{font_size: 12.0}
        }
    }
    let TaskRow = View{
        width: Fill height: Fit
        flow: Right spacing: 4
        open := TaskFace{text: "任务"}
        delete := DangerButton{text: "删除" width: 48 height: 36}
    }
    let TaskRowOn = View{
        width: Fill height: Fit
        flow: Right spacing: 4
        open := TaskFaceOn{text: "任务"}
        delete := DangerButton{text: "删除" width: 48 height: 36}
    }
    mod.widgets.TaskListBase = #(TaskList::register_widget(vm))
    mod.widgets.TaskList = set_type_default() do mod.widgets.TaskListBase{
        width: Fill height: Fill
        list := PortalList{
            width: Fill height: Fill
            spacing: 4.0
            scroll_bar: ScrollBar{}
            Item := CachedView{TaskRow{}}
            ItemOn := CachedView{TaskRowOn{}}
        }
    }
    let Field = TextInput{
        width: Fill height: 32
        draw_cursor.color: #x00C8FF
        draw_selection.color: #x1677FF
        draw_bg +: {
            color: #x030B18
            color_hover: #x061326
            color_focus: #x061326
            color_empty: #x030B18
            border_color: #x173858
            border_color_hover: #x173858
            border_color_focus: #x00C8FF
            border_color_empty: #x173858
            border_radius: 6.0
            border_size: 1.0
        }
        draw_text +: {
            color: #xE6F1FF
            color_hover: #xE6F1FF
            color_focus: #xE6F1FF
            color_down: #xE6F1FF
            color_empty: #x8EA8C7
            color_empty_hover: #x8EA8C7
            color_empty_focus: #xE6F1FF
            text_style: theme.font_label{font_size: 13.0}
        }
    }
    let Log = TextInput{
        width: Fill height: Fill
        is_multiline: true
        is_read_only: true
        padding: Inset{left: 8.0 right: 8.0 top: 6.0 bottom: 6.0}
        draw_cursor.color: #x00C8FF
        draw_selection.color: #x1677FF
        draw_bg +: {
            color: #x030B18
            color_hover: #x030B18
            color_focus: #x030B18
            color_empty: #x030B18
            border_color: #x030B18
            border_color_hover: #x030B18
            border_color_focus: #x030B18
            border_color_empty: #x030B18
            border_radius: 6.0
            border_size: 0.0
        }
        draw_text +: {
            color: #x00D6A0
            color_hover: #x00D6A0
            color_focus: #x00D6A0
            color_down: #x00D6A0
            color_empty: #x3d6b5c
            color_empty_hover: #x3d6b5c
            color_empty_focus: #x3d6b5c
            text_style: theme.font_code{font_size: 11.0}
        }
    }

    let app = startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                pass.clear_color: #x030B18
                window.inner_size: vec2(1440, 900)
                caption_bar +: { draw_bg.color: #x030B18 }
                body +: {
                    width: Fill height: Fill
                    flow: Down spacing: 0

                    SolidView{
                        width: Fill height: 56
                        flow: Right spacing: 8
                        align: Align{y: 0.5}
                        padding: Inset{left: 16.0 right: 12.0 top: 0.0 bottom: 0.0}
                        draw_bg.color: #x061326
                        SolidView{
                            width: 22 height: 22
                            align: Center
                            draw_bg.color: #x755CFF
                            draw_bg.border_radius: 4.0
                            Label{
                                text: "L"
                                draw_text.color: #xE6F1FF
                                draw_text.text_style: theme.font_bold{font_size: 12.0}
                            }
                        }
                        View{
                            width: Fit height: Fit
                            flow: Down spacing: 0
                            Title{text: "LIVSTYLE"}
                            Muted{text: "AI Agent for IoT Development"}
                        }
                        nav_work := QuietButton{text: "工作台" height: 28}
                        nav_tasks := QuietButton{text: "任务" height: 28}
                        nav_devices := QuietButton{text: "设备" height: 28}
                        nav_library := QuietButton{text: "知识库" height: 28}
                        nav_plugins := QuietButton{text: "插件" height: 28}
                        Filler{}
                        nav_current := Muted{text: "当前：工作台"}
                        octos_status := Muted{text: "Octos"}
                        cursor_status := Muted{text: "Cursor"}
                        settings := QuietButton{text: "设置" height: 28}
                    }

                    card_panel := RoundedView{
                        width: Fill height: Fit
                        visible: false
                        margin: Inset{left: 8.0 right: 8.0 top: 8.0 bottom: 0.0}
                        padding: 12
                        flow: Down spacing: 8
                        draw_bg.color: #x1c160c
                        draw_bg.border_radius: 8.0
                        draw_bg.border_size: 1.0
                        draw_bg.border_color: #xFFB547
                        card_title := Label{
                            text: ""
                            draw_text.text_style: theme.font_bold{font_size: 16.0}
                            draw_text.color: #xFFB547
                        }
                        card_body := Label{
                            text: ""
                            draw_text.color: #xE6F1FF
                            draw_text.text_style.font_size: 13.0
                        }
                        View{
                            width: Fill height: Fit
                            flow: Right spacing: 8
                            card_primary := PrimaryButton{text: "确认"}
                            card_secondary := QuietButton{text: "另一种"}
                            card_dismiss := DangerButton{text: "关闭"}
                            Filler{}
                        }
                    }

                    View{
                        width: Fill height: Fill
                        flow: Right spacing: 8
                        padding: 8

                        task_pane := RoundedView{
                            width: 280 height: Fill
                            flow: Down spacing: 8
                            padding: 12
                            draw_bg.color: #x061326
                            draw_bg.border_radius: 10.0
                            draw_bg.border_size: 1.0
                            draw_bg.border_color: #x173858
                            Section{text: "当前项目"}
                            workspace := Field{empty_text: "项目文件夹路径"}
                            View{
                                width: Fill height: Fit
                                flow: Right spacing: 6
                                browse := QuietButton{text: "浏览"}
                                use_ws := PrimaryButton{text: "使用"}
                            }
                            Muted{width: Fill height: Fit text: "搜索未接入。"}
                            Section{text: "工作区"}
                            Muted{width: Fill height: Fit text: "目录以上面的路径为准。"}
                            Section{text: "设备"}
                            board_toggle := QuietButton{text: "设备栏  ▾" width: Fill}
                            refresh := QuietButton{text: "刷新串口" width: Fill}
                            identify := QuietButton{text: "识别芯片" width: Fill}
                            flash := QuietButton{text: "记录烧录批准" width: Fill}
                            unknown := QuietButton{text: "结果未知" width: Fill}
                            Section{text: "Agent Runtime"}
                            use_octos := QuietButton{text: "Octos" width: Fill}
                            use_cursor := QuietButton{text: "Cursor" width: Fill}
                            Section{text: "后台任务"}
                            task_list := mod.widgets.TaskList{}
                            new_task := QuietButton{text: "新任务" width: Fill}
                        }

                        chat_pane := RoundedView{
                            width: Fill height: Fill
                            flow: Down spacing: 8
                            padding: 12
                            draw_bg.color: #x07182C
                            draw_bg.border_radius: 10.0
                            draw_bg.border_size: 1.0
                            draw_bg.border_color: #x173858
                            View{
                                width: Fill height: Fit
                                flow: Right spacing: 8
                                align: Align{y: 0.5}
                                Title{text: "当前任务"}
                                Filler{}
                                pause := QuietButton{text: "暂停" height: 28}
                                stop := DangerButton{text: "取消" height: 28}
                                copy_chat := QuietButton{text: "复制" height: 28}
                            }
                            Muted{
                                width: Fill height: Fit
                                text: "规划    环境检查    构建    烧录审批    设备连接    验证    完成"
                            }
                            Muted{
                                width: Fill height: Fit
                                text: "这些阶段没有节点数据，不会显示打勾或百分比。"
                            }
                            Section{text: "任务计划"}
                            plan_summary := Muted{
                                width: Fill height: Fit
                                text: "还没有保存的计划。"
                            }
                            transcript := Log{
                                empty_text: "对话会出现在这里。选中文字后按 Ctrl+C，或点「复制」。"
                                draw_text +: {
                                    color: #xE6F1FF
                                    color_hover: #xE6F1FF
                                    color_focus: #xE6F1FF
                                    color_down: #xE6F1FF
                                    color_empty: #x8EA8C7
                                    color_empty_hover: #x8EA8C7
                                    color_empty_focus: #x8EA8C7
                                    text_style: theme.font_label{font_size: 13.0}
                                }
                            }
                            composer_grip := View{
                                width: Fill height: 10
                                align: Center
                                cursor: MouseCursor.NsResize
                                View{
                                    width: Fill height: 1
                                    show_bg: true
                                    draw_bg.color: #x173858
                                }
                            }
                            composer := Field{
                                height: 72
                                is_multiline: true
                                padding: Inset{left: 10.0 right: 10.0 top: 8.0 bottom: 8.0}
                                empty_text: "输入你的任务需求。Shift+Enter 发送。"
                                return_key_type: Done
                            }
                            View{
                                width: Fill height: Fit
                                flow: Right spacing: 8
                                align: Align{y: 0.5}
                                attach := QuietButton{text: "附件未接入" height: 28}
                                hint := Muted{width: Fill height: Fit text: ""}
                                send := PrimaryButton{text: "发送" height: 28}
                            }
                        }

                        board_body := RoundedView{
                            width: 320 height: Fill
                            flow: Down spacing: 8
                            padding: 12
                            draw_bg.color: #x061326
                            draw_bg.border_radius: 10.0
                            draw_bg.border_size: 1.0
                            draw_bg.border_color: #x173858
                            Title{text: "任务详情"}
                            Section{text: "运行时"}
                            runtime := Muted{width: Fill height: Fit text: "Runtime"}
                            Section{text: "设备信息"}
                            board_status := Muted{width: Fill height: Fit text: "开发板"}
                            board_live := Log{
                                height: 64
                                empty_text: "等待串口扫描。"
                                draw_text +: {
                                    color: #xE6F1FF
                                    color_hover: #xE6F1FF
                                    color_focus: #xE6F1FF
                                    color_down: #xE6F1FF
                                    color_empty: #x8EA8C7
                                    color_empty_hover: #x8EA8C7
                                    color_empty_focus: #x8EA8C7
                                    text_style: theme.font_code{font_size: 11.0}
                                }
                            }
                            Section{text: "实时日志"}
                            board_log := Log{empty_text: "串口和芯片识别会写在这里。"}
                            build_pane := View{
                                width: Fill height: Fit
                                visible: false
                                flow: Down spacing: 4
                                build_head := Section{text: "产物与构建日志"}
                                build_log := Log{height: 88 empty_text: ""}
                            }
                            View{
                                width: Fill height: 110
                                flow: Down
                                board_stage := mod.widgets.BoardStage{}
                            }
                            Muted{
                                width: Fill height: Fit
                                text: "没有烧录验证通过时，这里没有固件名，也没有证据目录。"
                            }
                        }
                    }
                }
            }
        }
    }
    app
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    cmd: Option<mpsc::Sender<Cmd>>,
    #[rust]
    events: Option<mpsc::Receiver<Snapshot>>,
    #[rust(96.0)]
    composer_h: f64,
    #[rust]
    dragging: bool,
    #[rust(96.0)]
    drag_h: f64,
    #[rust(0.0)]
    drag_y: f64,
    #[rust(true)]
    board_open: bool,
    #[rust]
    transcript: String,
    #[rust]
    board_log: String,
}

#[derive(Script, ScriptHook, Widget)]
struct TaskList {
    #[deref]
    view: View,
}

impl Widget for TaskList {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let lines = LINES.lock().expect("tasks");
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, lines.len());
                while let Some(item_id) = list.next_visible_item(cx) {
                    let Some(line) = lines.get(item_id) else {
                        continue;
                    };
                    let template = if line.mark == "●" {
                        id!(ItemOn)
                    } else {
                        id!(Item)
                    };
                    let item = list.item(cx, item_id, template);
                    item.button(cx, ids!(open))
                        .set_text(cx, &format!("{}\n{}", line.title, line.status));
                    item.draw_all_unscoped(cx);
                }
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
}

fn latest_board_entry(log: &str) -> String {
    log.split("\n\n").next().unwrap_or(log).to_string()
}

impl App {
    fn keep_readonly(&mut self, cx: &mut Cx, actions: &Actions) {
        let transcript = self.ui.text_input(cx, ids!(transcript));
        if transcript.changed(actions).is_some() && transcript.text() != self.transcript {
            transcript.set_text(cx, &self.transcript);
        }
        let board_log = self.ui.text_input(cx, ids!(board_log));
        if board_log.changed(actions).is_some() && board_log.text() != self.board_log {
            board_log.set_text(cx, &self.board_log);
        }
        let live = latest_board_entry(&self.board_log);
        let board_live = self.ui.text_input(cx, ids!(board_live));
        if board_live.changed(actions).is_some() && board_live.text() != live {
            board_live.set_text(cx, &live);
        }
    }

    fn send(&self, cmd: Cmd) {
        if let Some(tx) = &self.cmd {
            let _ = tx.send(cmd);
            makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
        }
    }

    fn apply(&mut self, cx: &mut Cx, snap: Snapshot) {
        *LINES.lock().expect("tasks") = snap.tasks;
        self.ui.label(cx, ids!(runtime)).set_text(cx, &snap.runtime);
        self.transcript = snap.transcript.clone();
        let transcript = self.ui.text_input(cx, ids!(transcript));
        if transcript.text() != snap.transcript {
            transcript.set_text(cx, &snap.transcript);
        }
        self.board_log = snap.board_log.clone();
        board_view::set_board_visual(&snap.board_status);
        let board_log = self.ui.text_input(cx, ids!(board_log));
        if board_log.text() != snap.board_log {
            board_log.set_text(cx, &snap.board_log);
        }
        let live = latest_board_entry(&snap.board_log);
        let board_live = self.ui.text_input(cx, ids!(board_live));
        if board_live.text() != live {
            board_live.set_text(cx, &live);
        }
        self.ui
            .label(cx, ids!(cursor_status))
            .set_text(cx, &snap.cursor_status);
        self.ui
            .label(cx, ids!(octos_status))
            .set_text(cx, &snap.octos_status);
        self.ui
            .label(cx, ids!(board_status))
            .set_text(cx, &snap.board_status);
        let workspace = self.ui.text_input(cx, ids!(workspace));
        if workspace.text() != snap.workspace {
            workspace.set_text(cx, &snap.workspace);
        }
        self.ui.label(cx, ids!(hint)).set_text(cx, &snap.hint);
        let plan = if snap.plan.is_empty() {
            "还没有保存的计划。".to_string()
        } else {
            snap.plan.clone()
        };
        self.ui.label(cx, ids!(plan_summary)).set_text(cx, &plan);
        let build = self.ui.text_input(cx, ids!(build_log));
        if build.text() != snap.build_log {
            build.set_text(cx, &snap.build_log);
        }
        self.ui
            .view(cx, ids!(build_pane))
            .set_visible(cx, !snap.build_log.is_empty());
        let open = snap.card.is_some();
        self.ui.view(cx, ids!(card_panel)).set_visible(cx, open);
        if let Some(card) = snap.card {
            self.ui
                .label(cx, ids!(card_title))
                .set_text(cx, &card.title);
            self.ui.label(cx, ids!(card_body)).set_text(cx, &card.body);
            self.ui
                .button(cx, ids!(card_primary))
                .set_text(cx, &card.primary);
            self.ui
                .button(cx, ids!(card_secondary))
                .set_text(cx, &card.secondary);
            self.ui
                .button(cx, ids!(card_dismiss))
                .set_text(cx, &card.dismiss);
        }
        self.ui.redraw(cx);
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (snap_tx, snap_rx) = mpsc::channel();
        std::thread::spawn(move || worker::run(cmd_rx, snap_tx));
        self.cmd = Some(cmd_tx);
        self.events = Some(snap_rx);
        self.send(Cmd::Boot);
        if cfg!(target_os = "android") {
            self.ui.button(cx, ids!(flash)).set_visible(cx, false);
            self.ui.button(cx, ids!(identify)).set_visible(cx, false);
            self.ui.button(cx, ids!(refresh)).set_visible(cx, false);
        }
        self.ui.redraw(cx);
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if let Some((text, _)) = self.ui.text_input(cx, ids!(composer)).returned(actions) {
            if !text.trim().is_empty() {
                self.ui.text_input(cx, ids!(composer)).set_text(cx, "");
                self.send(Cmd::Send(text));
            }
        }
        self.keep_readonly(cx, actions);
        if self.ui.button(cx, ids!(copy_chat)).clicked(actions) {
            cx.copy_to_clipboard(&self.transcript);
            self.ui.label(cx, ids!(hint)).set_text(cx, "已复制对话。");
        }
        if self.ui.button(cx, ids!(board_toggle)).clicked(actions) {
            self.board_open = !self.board_open;
            self.ui
                .view(cx, ids!(board_body))
                .set_visible(cx, self.board_open);
            self.ui.button(cx, ids!(board_toggle)).set_text(
                cx,
                if self.board_open {
                    "设备栏  ▾"
                } else {
                    "设备栏  ▸"
                },
            );
        }
        if self.ui.button(cx, ids!(send)).clicked(actions) {
            let text = self.ui.text_input(cx, ids!(composer)).text();
            if !text.trim().is_empty() {
                self.ui.text_input(cx, ids!(composer)).set_text(cx, "");
                self.send(Cmd::Send(text));
            }
        }
        if self.ui.button(cx, ids!(pause)).clicked(actions) {
            self.send(Cmd::Pause);
        }
        if self.ui.button(cx, ids!(stop)).clicked(actions) {
            self.send(Cmd::Cancel);
        }
        if self.ui.button(cx, ids!(new_task)).clicked(actions) {
            self.send(Cmd::NewTask);
        }
        if self.ui.button(cx, ids!(browse)).clicked(actions) {
            self.send(Cmd::BrowseWorkspace);
        }
        if self.ui.button(cx, ids!(use_ws)).clicked(actions) {
            let path = self.ui.text_input(cx, ids!(workspace)).text();
            self.send(Cmd::SetWorkspace(path));
        }
        if self.ui.button(cx, ids!(refresh)).clicked(actions) {
            self.send(Cmd::RefreshDevices);
        }
        if self.ui.button(cx, ids!(identify)).clicked(actions) {
            self.send(Cmd::Identify);
        }
        if self.ui.button(cx, ids!(nav_work)).clicked(actions) {
            self.ui.view(cx, ids!(task_pane)).set_visible(cx, true);
            self.ui.view(cx, ids!(chat_pane)).set_visible(cx, true);
            self.ui.view(cx, ids!(board_body)).set_visible(cx, true);
            self.ui
                .label(cx, ids!(nav_current))
                .set_text(cx, "当前：工作台");
        }
        if self.ui.button(cx, ids!(attach)).clicked(actions) {
            self.ui
                .label(cx, ids!(hint))
                .set_text(cx, "附件还没有接入，不会上传文件。");
        }
        if self.ui.button(cx, ids!(nav_tasks)).clicked(actions) {
            self.ui.view(cx, ids!(task_pane)).set_visible(cx, true);
            self.ui.view(cx, ids!(chat_pane)).set_visible(cx, true);
            self.ui.view(cx, ids!(board_body)).set_visible(cx, false);
            self.ui
                .label(cx, ids!(nav_current))
                .set_text(cx, "当前：任务");
        }
        if self.ui.button(cx, ids!(nav_devices)).clicked(actions) {
            self.ui.view(cx, ids!(board_body)).set_visible(cx, true);
            self.ui.view(cx, ids!(chat_pane)).set_visible(cx, false);
            self.ui
                .label(cx, ids!(nav_current))
                .set_text(cx, "当前：设备");
        }
        if self.ui.button(cx, ids!(nav_library)).clicked(actions)
            || self.ui.button(cx, ids!(nav_plugins)).clicked(actions)
        {
            self.ui
                .label(cx, ids!(hint))
                .set_text(cx, "知识库和插件还没有接入，没有可打开的页面。");
        }
        if self.ui.button(cx, ids!(settings)).clicked(actions) {
            self.send(Cmd::OpenSettings);
        }
        if self.ui.button(cx, ids!(use_cursor)).clicked(actions) {
            self.send(Cmd::UseAgent(CodeAgent::Cursor));
        }
        if self.ui.button(cx, ids!(use_octos)).clicked(actions) {
            self.send(Cmd::UseAgent(CodeAgent::Octos));
        }
        if self.ui.button(cx, ids!(flash)).clicked(actions) {
            self.send(Cmd::OpenFlash);
        }
        if self.ui.button(cx, ids!(unknown)).clicked(actions) {
            self.send(Cmd::OpenUnknown);
        }
        if self.ui.button(cx, ids!(card_primary)).clicked(actions) {
            self.send(Cmd::Primary);
        }
        if self.ui.button(cx, ids!(card_secondary)).clicked(actions) {
            self.send(Cmd::Secondary);
        }
        if self.ui.button(cx, ids!(card_dismiss)).clicked(actions) {
            self.send(Cmd::Dismiss);
        }
        let list = self
            .ui
            .widget(cx, ids!(task_list))
            .portal_list(cx, ids!(list));
        for (item_id, item) in list.items_with_actions(actions) {
            if item.button(cx, ids!(delete)).clicked(actions) {
                self.send(Cmd::Delete(item_id));
            } else if item.button(cx, ids!(open)).clicked(actions) {
                self.send(Cmd::Select(item_id));
            }
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        crate::makepad_widgets::script_mod(vm);
        board_view::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        let grip = self.ui.view(cx, ids!(composer_grip)).area();
        match event.hits(cx, grip) {
            Hit::FingerDown(e) => {
                self.dragging = true;
                self.drag_y = e.abs.y;
                self.drag_h = self.composer_h;
            }
            Hit::FingerUp(_) => self.dragging = false,
            Hit::FingerMove(e) if self.dragging => {
                self.composer_h = (self.drag_h - (e.abs.y - self.drag_y)).clamp(72.0, 280.0);
                self.ui
                    .text_input(cx, ids!(composer))
                    .set_height(cx, Size::Fixed(self.composer_h));
                self.ui.redraw(cx);
            }
            _ => {}
        }
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        let snaps: Vec<Snapshot> = self
            .events
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for snap in snaps {
            self.apply(cx, snap);
        }
    }
}
