pub use makepad_widgets;

mod board_view;
mod turns;
mod worker;

use std::sync::{mpsc, Mutex};

use makepad_widgets::*;
use turns::CodeAgent;
use worker::{Cmd, Snapshot, TaskLine};

static LINES: Mutex<Vec<TaskLine>> = Mutex::new(Vec::new());

struct ChatLine {
    kind: u8,
    who: String,
    body: String,
}

static CHAT: Mutex<Vec<ChatLine>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColumnDrag {
    Left,
    Right,
}

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    let PrimaryButton = Button{
        height: 32
        padding: Inset{left: 14.0 right: 14.0 top: 0.0 bottom: 0.0}
        draw_bg +: {
            color: #x1677FF
            color_2: #x00C8FF
            color_hover: #x2b86ff
            color_down: #x0f5ed4
            border_color: #x7FE7FF
            border_color_hover: #x00C8FF
            border_color_down: #x0f5ed4
            border_radius: 16.0
            border_size: 1.0
            gradient_fill_horizontal: 1.0
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(1.0 1.0 self.rect_size.x - 2.0 self.rect_size.y - 2.0 self.border_radius)
                sdf.glow_keep(vec4(0.0, 0.85, 1.0, 0.45) 5.0)
                let fill = mix(self.color self.color_2 self.pos.x * 0.85)
                sdf.fill_keep(fill)
                sdf.stroke(self.border_color 1.0)
                return sdf.result
            }
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
            border_radius: 10.0
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
    let NavOn = QuietButton{
        draw_bg +: {
            color: #x0C3A66
            color_hover: #x124878
            border_color: #x00C8FF
            border_radius: 14.0
            border_size: 1.0
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(1.0 1.0 self.rect_size.x - 2.0 self.rect_size.y - 2.0 self.border_radius)
                sdf.glow_keep(vec4(0.0, 0.78, 1.0, 0.55) 6.0)
                sdf.fill_keep(self.color)
                sdf.stroke(self.border_color 1.0)
                return sdf.result
            }
        }
    }
    let NavOff = QuietButton{
        draw_bg +: {
            color: #x00000000
            color_hover: #x0A2038
            border_color: #x00000000
            border_radius: 14.0
            border_size: 0.0
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
            border_radius: 10.0
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
    let Glyph = Label{
        width: Fit height: Fit
        draw_text.color: #x3EE0FF
        draw_text.text_style: theme.font_icons{font_size: 13.0}
        text: "●"
    }
    let Head = View{
        width: Fill height: Fit
        flow: Right spacing: 6
        align: Align{y: 0.5}
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
        draw_text.text_style: theme.font_label{font_size: 12.0}
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
            text_overflow: TextOverflow.Ellipsis
            max_lines: 1
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
            text_overflow: TextOverflow.Ellipsis
            max_lines: 1
        }
    }
    let TaskState = Label{
        width: 64 height: 32
        align: Align{x: 1.0, y: 0.5}
        padding: Inset{left: 0.0 right: 2.0 top: 0.0 bottom: 0.0}
        draw_text.color: #x8EA8C7
        draw_text.text_style: theme.font_label{font_size: 11.0}
        draw_text.text_overflow: TextOverflow.Ellipsis
        draw_text.max_lines: 1
    }
    let TaskRow = View{
        width: Fill height: Fit
        flow: Right spacing: 4
        align: Align{y: 0.5}
        open := TaskFace{text: "任务" height: 32}
        state := TaskState{text: ""}
        delete := DangerButton{text: "删除" width: 52 height: 32}
    }
    let TaskRowOn = View{
        width: Fill height: Fit
        flow: Right spacing: 4
        align: Align{y: 0.5}
        open := TaskFaceOn{text: "任务" height: 32}
        state := TaskState{text: ""}
        delete := DangerButton{text: "删除" width: 52 height: 32}
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
            text_overflow: TextOverflow.Ellipsis
            max_lines: 1
        }
    }
    let PanelSide = RoundedShadowView{
        clip_x: true
        clip_y: true
        draw_bg.color: #x061326
        draw_bg.color_2: #x0A2342
        draw_bg.border_color: #x1A6F96
        draw_bg.border_size: 1.0
        draw_bg.border_radius: 14.0
        draw_bg.shadow_color: #x00000055
        draw_bg.shadow_radius: 22.0
    }
    let PanelCenter = RoundedShadowView{
        clip_x: true
        clip_y: true
        draw_bg.color: #x07182C
        draw_bg.color_2: #x0B2848
        draw_bg.border_color: #x2078A8
        draw_bg.border_size: 1.0
        draw_bg.border_radius: 14.0
        draw_bg.shadow_color: #x00000066
        draw_bg.shadow_radius: 26.0
    }
    let ColGrip = View{
        width: 6 height: Fill
        align: Center
        cursor: MouseCursor.EwResize
        View{
            width: 1 height: Fill
            margin: Inset{top: 12.0 bottom: 12.0}
            show_bg: true
            draw_bg.color: #x173858
        }
    }
    let InfoCard = RoundedShadowView{
        width: Fill height: Fit
        flow: Down spacing: 6
        padding: 10
        clip_x: true
        clip_y: true
        draw_bg.color: #x071E36
        draw_bg.color_2: #x0C3054
        draw_bg.border_color: #x1A6F96
        draw_bg.border_radius: 12.0
        draw_bg.border_size: 1.0
        draw_bg.shadow_color: #x00000044
        draw_bg.shadow_radius: 14.0
    }
    let StatusDot = SolidView{
        width: 7 height: 7
        draw_bg.border_radius: 3.5
    }
    let StageDot = RoundedView{
        width: 16 height: 16
        align: Center
        draw_bg.color: #x0A2038
        draw_bg.border_color: #x3A5470
        draw_bg.border_size: 1.5
        draw_bg.border_radius: 8.0
    }
    let StageNow = RoundedView{
        width: 18 height: 18
        visible: false
        align: Center
        draw_bg.color: #x00C8FF
        draw_bg.border_color: #xB8F6FF
        draw_bg.border_size: 2.0
        draw_bg.border_radius: 9.0
    }
    let StageCheck = RoundedView{
        width: 18 height: 18
        visible: false
        align: Center
        draw_bg.color: #x00D6A0
        draw_bg.border_radius: 9.0
        draw_bg.border_size: 0.0
        Label{
            text: "✓"
            draw_text.color: #x042018
            draw_text.text_style: theme.font_bold{font_size: 12.0}
        }
    }
    let BubbleYou = View{
        width: Fill height: Fit
        flow: Right spacing: 8
        padding: Inset{left: 72.0 right: 0.0 top: 0.0 bottom: 0.0}
        Glyph{text: "" draw_text.text_style: theme.font_icons{font_size: 16.0}}
        RoundedView{
            width: Fill height: Fit
            flow: Down spacing: 2
            padding: 10
            draw_bg.color: #x0A2038
            draw_bg.border_radius: 12.0
            draw_bg.border_size: 1.0
            draw_bg.border_color: #x1677FF
            who := Muted{text: "你"}
            body := TextBox{
                draw_text.color: #xE6F1FF
                draw_text.text_style: theme.font_label{font_size: 13.0}
            }
        }
    }
    let BubbleOctos = View{
        width: Fill height: Fit
        flow: Right spacing: 8
        padding: Inset{left: 0.0 right: 48.0 top: 0.0 bottom: 0.0}
        Glyph{text: "" draw_text.text_style: theme.font_icons{font_size: 16.0} draw_text.color: #x3EE0FF}
        RoundedView{
            width: Fill height: Fit
            flow: Down spacing: 4
            padding: 10
            draw_bg.color: #x061326
            draw_bg.color_2: #x0A2848
            draw_bg.border_radius: 12.0
            draw_bg.border_size: 1.0
            draw_bg.border_color: #x00C8FF
            draw_bg +: {
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box(1.0 1.0 self.rect_size.x - 2.0 self.rect_size.y - 2.0 max(1.0 self.border_radius))
                    sdf.glow_keep(vec4(0.0, 0.78, 1.0, 0.32) 5.0)
                    sdf.fill_keep(self.color)
                    sdf.stroke(self.border_color 1.0)
                    return sdf.result
                }
            }
            who := Muted{text: "Octos" draw_text.color: #x00C8FF}
            body := TextBox{
                draw_text.color: #xE6F1FF
                draw_text.text_style: theme.font_label{font_size: 13.0}
            }
        }
    }
    let BubbleCursor = View{
        width: Fill height: Fit
        flow: Right spacing: 8
        padding: Inset{left: 0.0 right: 48.0 top: 0.0 bottom: 0.0}
        Glyph{text: "" draw_text.text_style: theme.font_icons{font_size: 16.0} draw_text.color: #xC4B5FF}
        RoundedView{
            width: Fill height: Fit
            flow: Down spacing: 4
            padding: 10
            draw_bg.color: #x061326
            draw_bg.border_radius: 12.0
            draw_bg.border_size: 1.0
            draw_bg.border_color: #x755CFF
            draw_bg +: {
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box(1.0 1.0 self.rect_size.x - 2.0 self.rect_size.y - 2.0 max(1.0 self.border_radius))
                    sdf.glow_keep(vec4(0.46, 0.36, 1.0, 0.40) 7.0)
                    sdf.fill_keep(self.color)
                    sdf.stroke(self.border_color 1.0)
                    return sdf.result
                }
            }
            who := Muted{text: "Cursor" draw_text.color: #xB9A6FF}
            body := TextBox{
                draw_text.color: #xE6F1FF
                draw_text.text_style: theme.font_label{font_size: 13.0}
            }
        }
    }
    let BubbleNote = View{
        width: Fill height: Fit
        padding: Inset{left: 0.0 right: 24.0 top: 0.0 bottom: 0.0}
        who := Muted{text: "livstyle"}
        body := TextBox{
            draw_text.color: #x8EA8C7
            draw_text.text_style: theme.font_label{font_size: 12.0}
        }
    }
    mod.widgets.ChatListBase = #(ChatList::register_widget(vm))
    mod.widgets.ChatList = set_type_default() do mod.widgets.ChatListBase{
        width: Fill height: Fill
        list := PortalList{
            width: Fill height: Fill
            spacing: 8.0
            scroll_bar: ScrollBar{}
            You := CachedView{BubbleYou{}}
            Octos := CachedView{BubbleOctos{}}
            Cursor := CachedView{BubbleCursor{}}
            Note := CachedView{BubbleNote{}}
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
            text_style: theme.font_label{font_size: 11.0}
        }
    }

    let app = startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                pass.clear_color: #x030B18
                window.inner_size: vec2(1440, 900)
                window.title: "LIVSTYLE"
                caption_bar +: {
                    draw_bg.color: #x030B18
                    caption_label := View{
                        width: Fill
                        height: Fill
                        align: Center
                        label := Label{visible: false text: ""}
                        RoundedView{
                            width: 22 height: 22
                            align: Center
                            draw_bg.color: #x08243C
                            draw_bg.border_color: #x7AF0FF
                            draw_bg.border_size: 1.0
                            draw_bg.border_radius: 6.0
                            draw_bg +: {
                                pixel: fn() {
                                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                    sdf.box(1.0 1.0 self.rect_size.x - 2.0 self.rect_size.y - 2.0 5.0)
                                    sdf.glow_keep(vec4(0.15, 0.85, 1.0, 0.85) 6.0)
                                    sdf.fill_keep(self.color)
                                    sdf.stroke(self.border_color 1.0)
                                    return sdf.result
                                }
                            }
                            Glyph{text: "" draw_text.text_style: theme.font_icons{font_size: 12.0} draw_text +: {color: #xE8FBFF}}
                        }
                    }
                }
                body +: {
                    width: Fill height: Fill
                    flow: Down spacing: 0

                    RoundedView{
                        width: Fill height: 58
                        flow: Right spacing: 8
                        align: Align{y: 0.5}
                        padding: Inset{left: 16.0 right: 12.0 top: 0.0 bottom: 0.0}
                        draw_bg.color: #x07182C
                        draw_bg.color_2: #x04101E
                        draw_bg.border_size: 0.0
                        draw_bg.border_radius: 0.0
                        draw_bg +: {
                            pixel: fn() {
                                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                let fill = mix(self.color self.color_2 self.pos.y)
                                sdf.fill_keep(fill)
                                sdf.rect(0.0 self.rect_size.y - 1.0 self.rect_size.x 1.0)
                                sdf.glow(vec4(0.0, 0.78, 1.0, 0.85) 2.0)
                                return sdf.result
                            }
                        }
                        View{
                            width: Fit height: Fit
                            flow: Down spacing: 0
                            Muted{text: "AI Agent for IoT Development"}
                        }
                        View{
                            width: Fit height: Fit
                            flow: Right spacing: 4
                            align: Align{y: 0.5}
                            Glyph{text: ""}
                            View{
                                width: Fit height: Fit
                                nav_work := NavOn{text: "工作台" height: 30}
                                nav_work_idle := NavOff{text: "工作台" height: 30 visible: false}
                            }
                        }
                        View{
                            width: Fit height: Fit
                            flow: Right spacing: 4
                            align: Align{y: 0.5}
                            Glyph{text: ""}
                            View{
                                width: Fit height: Fit
                                nav_tasks := NavOn{text: "任务" height: 30 visible: false}
                                nav_tasks_idle := NavOff{text: "任务" height: 30}
                            }
                        }
                        View{
                            width: Fit height: Fit
                            flow: Right spacing: 4
                            align: Align{y: 0.5}
                            Glyph{text: ""}
                            View{
                                width: Fit height: Fit
                                nav_devices := NavOn{text: "设备" height: 30 visible: false}
                                nav_devices_idle := NavOff{text: "设备" height: 30}
                            }
                        }
                        View{
                            width: Fit height: Fit
                            flow: Right spacing: 4
                            align: Align{y: 0.5}
                            Glyph{text: ""}
                            nav_library := NavOff{text: "知识库" height: 30}
                        }
                        View{
                            width: Fit height: Fit
                            flow: Right spacing: 4
                            align: Align{y: 0.5}
                            Glyph{text: ""}
                            nav_plugins := NavOff{text: "插件" height: 30}
                        }
                        Filler{}
                        RoundedView{
                            width: Fit height: 28
                            flow: Right spacing: 6
                            align: Align{y: 0.5}
                            padding: Inset{left: 8.0 right: 10.0 top: 0.0 bottom: 0.0}
                            draw_bg.color: #x062033
                            draw_bg.border_color: #x145A78
                            draw_bg.border_size: 1.0
                            draw_bg.border_radius: 14.0
                            octos_wait := StatusDot{draw_bg.color: #xFFB547}
                            octos_ok := StatusDot{visible: false draw_bg.color: #x00D6A0}
                            octos_bad := StatusDot{visible: false draw_bg.color: #xFF5C75}
                            octos_status := TextBox{
                                width: 132
                                draw_text.color: #xE6F1FF
                                draw_text.text_style: theme.font_label{font_size: 11.0}
                                draw_text.text_overflow: TextOverflow.Ellipsis
                                draw_text.max_lines: 1
                                text: "Octos"
                            }
                        }
                        RoundedView{
                            width: Fit height: 28
                            flow: Right spacing: 6
                            align: Align{y: 0.5}
                            padding: Inset{left: 8.0 right: 10.0 top: 0.0 bottom: 0.0}
                            draw_bg.color: #x062033
                            draw_bg.border_color: #x145A78
                            draw_bg.border_size: 1.0
                            draw_bg.border_radius: 14.0
                            cursor_wait := StatusDot{draw_bg.color: #xFFB547}
                            cursor_ok := StatusDot{visible: false draw_bg.color: #x00D6A0}
                            cursor_bad := StatusDot{visible: false draw_bg.color: #xFF5C75}
                            cursor_status := TextBox{
                                width: 148
                                draw_text.color: #xE6F1FF
                                draw_text.text_style: theme.font_label{font_size: 11.0}
                                draw_text.text_overflow: TextOverflow.Ellipsis
                                draw_text.max_lines: 1
                                text: "Cursor"
                            }
                        }
                        View{
                            width: Fit height: Fit
                            flow: Right spacing: 4
                            align: Align{y: 0.5}
                            Glyph{text: ""}
                            settings := QuietButton{text: "设置" height: 28}
                        }
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
                            draw_text.text_style: theme.font_label{font_size: 13.0}
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

                    workbench := View{
                        width: Fill height: Fill
                        flow: Overlay

                        Image{
                            width: Fill height: Fill
                            src: crate_resource("self:resources/livstyle-glow.jpg")
                            fit: ImageFit.CropToFill
                            draw_bg.opacity: 0.85
                        }

                        View{
                            width: Fill height: Fill
                            flow: Right spacing: 0
                            padding: 8

                        task_pane := PanelSide{
                            width: 300 height: Fill
                            flow: Down spacing: 8
                            padding: 12
                            nav_current := Muted{width: Fill height: Fit text: "当前：工作台"}
                            side_scroll := ScrollYView{
                                width: Fill height: Fill
                                flow: Down spacing: 8
                                Head{
                                    Glyph{text: ""}
                                    Section{text: "当前项目"}
                                }
                                workspace := Field{empty_text: "项目文件夹路径"}
                                path_full := TextBox{
                                    width: Fill
                                    draw_text.color: #x8EA8C7
                                    draw_text.text_style: theme.font_label{font_size: 11.0}
                                    text: "完整路径显示在这里，输入框只保留一行。"
                                }
                                View{
                                    width: Fill height: Fit
                                    flow: Right spacing: 6
                                    browse := QuietButton{text: "浏览" width: Fill}
                                    use_ws := PrimaryButton{text: "使用" width: Fill}
                                }
                                Head{
                                    Glyph{text: ""}
                                    search := Field{empty_text: "搜索未接入"}
                                }
                                Head{
                                    Glyph{text: ""}
                                    Section{text: "工作区"}
                                }
                                Muted{width: Fill height: Fit text: "只有上面这一个目录。"}
                                Head{
                                    Glyph{text: ""}
                                    Section{text: "设备"}
                                }
                                board_toggle := QuietButton{text: "设备栏开着" width: Fill}
                                refresh := QuietButton{text: "刷新串口" width: Fill}
                                identify := QuietButton{text: "识别芯片" width: Fill}
                                flash := QuietButton{text: "记录烧录批准" width: Fill}
                                unknown := QuietButton{text: "结果未知" width: Fill}
                                Head{
                                    Glyph{text: ""}
                                    Section{text: "Agent Runtime"}
                                }
                                use_octos := QuietButton{text: "Octos" width: Fill}
                                use_cursor := QuietButton{text: "Cursor" width: Fill}
                            }
                            View{
                                width: Fill height: Fit
                                flow: Right spacing: 6
                                align: Align{y: 0.5}
                                Glyph{text: ""}
                                Section{text: "后台任务"}
                                Filler{}
                                new_task := QuietButton{text: "新任务" height: 26}
                            }
                            empty_tasks := Muted{width: Fill height: Fit text: "还没有任务。"}
                            task_list := mod.widgets.TaskList{}
                        }

                        col_grip_l := ColGrip{}

                        chat_pane := PanelCenter{
                            width: Fill height: Fill
                            flow: Down spacing: 8
                            padding: 12
                            View{
                                width: Fill height: Fit
                                flow: Right spacing: 8
                                align: Align{y: 0.5}
                                View{
                                    width: Fill height: Fit
                                    flow: Down spacing: 2
                                    task_title := Title{text: "未选择任务"}
                                    task_state := Muted{text: ""}
                                }
                                pause := QuietButton{text: "暂停" height: 28}
                                stop := DangerButton{text: "取消" height: 28}
                                copy_chat := QuietButton{text: "复制" height: 28}
                            }
                            InfoCard{
                                flow: Down spacing: 6
                                stage_note := Label{
                                    width: Fill
                                    draw_text.color: #x8EA8C7
                                    draw_text.text_style: theme.font_bold{font_size: 11.0}
                                    text: ""
                                }
                                View{
                                    width: Fill height: Fit
                                    flow: Right spacing: 0
                                    align: Align{y: 0.5}
                                    View{width: 22 height: 22 flow: Overlay align: Center stage_plan_idle := StageDot{} stage_plan_on := StageNow{} stage_plan_done := StageCheck{}}
                                    SolidView{width: Fill height: 2 draw_bg.color: #x1A6F96}
                                    View{width: 22 height: 22 flow: Overlay align: Center stage_env_idle := StageDot{} stage_env_on := StageNow{} stage_env_done := StageCheck{}}
                                    SolidView{width: Fill height: 2 draw_bg.color: #x1A6F96}
                                    View{width: 22 height: 22 flow: Overlay align: Center stage_build_idle := StageDot{} stage_build_on := StageNow{} stage_build_done := StageCheck{}}
                                    SolidView{width: Fill height: 2 draw_bg.color: #x1A6F96}
                                    View{width: 22 height: 22 flow: Overlay align: Center stage_approve_idle := StageDot{} stage_approve_on := StageNow{} stage_approve_done := StageCheck{}}
                                    SolidView{width: Fill height: 2 draw_bg.color: #x1A6F96}
                                    View{width: 22 height: 22 flow: Overlay align: Center stage_link_idle := StageDot{} stage_link_on := StageNow{} stage_link_done := StageCheck{}}
                                    SolidView{width: Fill height: 2 draw_bg.color: #x1A6F96}
                                    View{width: 22 height: 22 flow: Overlay align: Center stage_verify_idle := StageDot{} stage_verify_on := StageNow{} stage_verify_done := StageCheck{}}
                                    SolidView{width: Fill height: 2 draw_bg.color: #x1A6F96}
                                    View{width: 22 height: 22 flow: Overlay align: Center stage_finish_idle := StageDot{} stage_finish_on := StageNow{} stage_finish_done := StageCheck{}}
                                }
                                View{
                                    width: Fill height: Fit
                                    flow: Right
                                    align: Align{x: 0.5}
                                    Muted{width: Fill align: Align{x: 0.5} text: "规划"}
                                    Muted{width: Fill align: Align{x: 0.5} text: "环境"}
                                    Muted{width: Fill align: Align{x: 0.5} text: "构建"}
                                    Muted{width: Fill align: Align{x: 0.5} text: "审批"}
                                    Muted{width: Fill align: Align{x: 0.5} text: "连接"}
                                    Muted{width: Fill align: Align{x: 0.5} text: "验证"}
                                    Muted{width: Fill align: Align{x: 0.5} text: "完成"}
                                }
                            }
                            InfoCard{
                                Head{
                                    Glyph{text: ""}
                                    Section{text: "任务计划"}
                                }
                                plan_summary := TextBox{
                                    width: Fill
                                    draw_text.color: #xE6F1FF
                                    draw_text.text_style: theme.font_label{font_size: 12.0}
                                    text: "还没有保存的计划。"
                                }
                            }
                            empty_chat := RoundedView{
                                width: Fill height: Fill
                                align: Center
                                flow: Down spacing: 6
                                padding: 20
                                draw_bg.color: #x030B18
                                draw_bg.border_radius: 8.0
                                Title{text: "从下面输入开始"}
                                Muted{
                                    width: 420
                                    text: "用户、Octos、Cursor 和 livstyle 的记录会按顺序出现在这里。工具调用没有单独列表，执行文字就在对话里。"
                                }
                            }
                            chat_slot := View{
                                width: Fill height: Fill
                                flow: Down
                                chat_list := mod.widgets.ChatList{}
                            }
                            InfoCard{
                                composer_grip := View{
                                    width: Fill height: 8
                                    align: Center
                                    cursor: MouseCursor.NsResize
                                    View{
                                        width: Fill height: 1
                                        show_bg: true
                                        draw_bg.color: #x173858
                                    }
                                }
                                View{
                                    width: Fill height: Fit
                                    flow: Right spacing: 8
                                    align: Align{y: 0.5}
                                    RoundedView{
                                        width: Fit height: 28
                                        flow: Right spacing: 6
                                        align: Align{y: 0.5}
                                        padding: Inset{left: 8.0 right: 10.0}
                                        draw_bg.color: #x0A2038
                                        draw_bg.border_color: #x1A6F96
                                        draw_bg.border_size: 1.0
                                        draw_bg.border_radius: 14.0
                                        Glyph{text: ""}
                                        composer_agent := TextBox{
                                            width: 120
                                            draw_text.color: #xE6F1FF
                                            draw_text.text_style: theme.font_label{font_size: 12.0}
                                            draw_text.text_overflow: TextOverflow.Ellipsis
                                            draw_text.max_lines: 1
                                            text: "Runtime"
                                        }
                                    }
                                    RoundedView{
                                        width: Fit height: 28
                                        flow: Right spacing: 6
                                        align: Align{y: 0.5}
                                        padding: Inset{left: 8.0 right: 10.0}
                                        draw_bg.color: #x0A2038
                                        draw_bg.border_color: #x1A6F96
                                        draw_bg.border_size: 1.0
                                        draw_bg.border_radius: 14.0
                                        Glyph{text: ""}
                                        composer_device := TextBox{
                                            width: 140
                                            draw_text.color: #xE6F1FF
                                            draw_text.text_style: theme.font_label{font_size: 12.0}
                                            draw_text.text_overflow: TextOverflow.Ellipsis
                                            draw_text.max_lines: 1
                                            text: "设备"
                                        }
                                    }
                                    View{
                                        width: Fit height: Fit
                                        flow: Right spacing: 4
                                        align: Align{y: 0.5}
                                        Glyph{text: ""}
                                        attach := QuietButton{text: "附件未接入" height: 28}
                                    }
                                }
                                composer := Field{
                                    height: 72
                                    is_multiline: true
                                    padding: Inset{left: 10.0 right: 10.0 top: 8.0 bottom: 8.0}
                                    empty_text: "输入你的任务需求。Enter 发送。空内容不会发出。"
                                    return_key_type: Done
                                }
                                View{
                                    width: Fill height: Fit
                                    flow: Right spacing: 8
                                    align: Align{y: 0.5}
                                    Glyph{text: "" draw_text +: {color: #x8EE7FF text_style: theme.font_icons{font_size: 15.0}}}
                                    Glyph{text: "" draw_text +: {color: #x8EE7FF text_style: theme.font_icons{font_size: 15.0}}}
                                    hint := TextBox{
                                        width: Fill
                                        draw_text.color: #x8EA8C7
                                        draw_text.text_style: theme.font_label{font_size: 12.0}
                                        draw_text.text_overflow: TextOverflow.Ellipsis
                                        draw_text.max_lines: 1
                                        text: "Enter 发送"
                                    }
                                    send := PrimaryButton{text: "发送  →" height: 40 width: 120}
                                }
                            }
                        }

                        col_grip_r := ColGrip{}

                        board_body := PanelSide{
                            width: 340 height: Fill
                            flow: Down spacing: 0
                            padding: 0
                            ScrollYView{
                                width: Fill height: Fill
                                flow: Down spacing: 8
                                padding: 12
                                InfoCard{
                                    Head{
                                        Glyph{text: ""}
                                        Section{text: "任务详情"}
                                    }
                                    detail_task := TextBox{
                                        width: Fill
                                        draw_text.color: #xE6F1FF
                                        draw_text.text_style: theme.font_label{font_size: 13.0}
                                        text: "未选择任务"
                                    }
                                    Head{
                                        Glyph{text: ""}
                                        Section{text: "运行时"}
                                    }
                                    runtime := TextBox{
                                        width: Fill
                                        draw_text.color: #x8EA8C7
                                        draw_text.text_style: theme.font_label{font_size: 12.0}
                                        text: "Runtime"
                                    }
                                    Head{
                                        Glyph{text: ""}
                                        Section{text: "工作目录"}
                                    }
                                    detail_workspace := TextBox{
                                        width: Fill
                                        draw_text.color: #x8EA8C7
                                        draw_text.text_style: theme.font_label{font_size: 11.0}
                                        text: ""
                                    }
                                }
                                InfoCard{
                                    Head{
                                        Glyph{text: ""}
                                        Section{text: "设备信息"}
                                    }
                                    board_status := TextBox{
                                        width: Fill
                                        draw_text.color: #xE6F1FF
                                        draw_text.text_style: theme.font_label{font_size: 12.0}
                                        text: "开发板"
                                    }
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
                                            text_style: theme.font_label{font_size: 11.0}
                                        }
                                    }
                                    View{
                                        width: Fill height: 72
                                        flow: Down
                                        board_stage := mod.widgets.BoardStage{}
                                    }
                                }
                                InfoCard{
                                    Head{
                                        Glyph{text: ""}
                                        Section{text: "执行进度"}
                                    }
                                    Muted{width: Fill height: Fit text: "没有步骤百分比。看任务状态，不看进度条。"}
                                }
                                InfoCard{
                                    Head{
                                        Glyph{text: ""}
                                        Section{text: "实时日志"}
                                    }
                                    board_log := Log{height: 140 empty_text: "串口和芯片识别会写在这里。"}
                                }
                                InfoCard{
                                    Head{
                                        Glyph{text: ""}
                                        Section{text: "产物与证据"}
                                    }
                                    build_pane := View{
                                        width: Fill height: Fit
                                        visible: false
                                        flow: Down spacing: 4
                                        build_head := Section{text: "构建日志"}
                                        build_log := Log{height: 100 empty_text: ""}
                                    }
                                    View{
                                        width: Fill height: Fit
                                        flow: Right spacing: 8
                                        View{width: Fit height: Fit flow: Down spacing: 4 align: Align{x: 0.5} Glyph{text: ""} Muted{text: "固件未产生"}}
                                        View{width: Fit height: Fit flow: Down spacing: 4 align: Align{x: 0.5} Glyph{text: ""} Muted{text: "构建未产生"}}
                                        View{width: Fit height: Fit flow: Down spacing: 4 align: Align{x: 0.5} Glyph{text: ""} Muted{text: "串口未产生"}}
                                    }
                                }
                            }
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
    composer_drag: bool,
    #[rust(96.0)]
    drag_h: f64,
    #[rust(0.0)]
    drag_y: f64,
    #[rust]
    col_drag: Option<ColumnDrag>,
    #[rust(0.0)]
    col_drag_x: f64,
    #[rust(300.0)]
    left_w: f64,
    #[rust(340.0)]
    right_w: f64,
    #[rust(1440.0)]
    window_w: f64,
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
                    item.button(cx, ids!(open)).set_text(cx, &line.title);
                    item.label(cx, ids!(state)).set_text(cx, &line.status);
                    item.label(cx, ids!(state))
                        .set_text_color(cx, status_color(&line.status));
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

#[derive(Script, ScriptHook, Widget)]
struct ChatList {
    #[deref]
    view: View,
}

impl Widget for ChatList {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let lines = CHAT.lock().expect("chat");
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, lines.len());
                while let Some(item_id) = list.next_visible_item(cx) {
                    let Some(line) = lines.get(item_id) else {
                        continue;
                    };
                    let template = match line.kind {
                        0 => id!(You),
                        1 => id!(Octos),
                        2 => id!(Cursor),
                        _ => id!(Note),
                    };
                    let item = list.item(cx, item_id, template);
                    item.label(cx, ids!(who)).set_text(cx, &line.who);
                    item.label(cx, ids!(body)).set_text(cx, &line.body);
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
        *LINES.lock().expect("tasks") = snap.tasks.clone();
        if let Some(line) = snap.tasks.iter().find(|line| line.mark == "●") {
            self.ui
                .label(cx, ids!(task_title))
                .set_text(cx, &line.title);
            self.ui
                .label(cx, ids!(task_state))
                .set_text(cx, &line.status);
            self.ui
                .label(cx, ids!(task_state))
                .set_text_color(cx, status_color(&line.status));
            self.ui
                .label(cx, ids!(detail_task))
                .set_text(cx, &format!("{}  {}", line.title, line.status));
            light_stage(cx, &self.ui, &line.status);
        } else {
            self.ui
                .label(cx, ids!(task_title))
                .set_text(cx, "未选择任务");
            self.ui.label(cx, ids!(task_state)).set_text(cx, "");
            self.ui
                .label(cx, ids!(detail_task))
                .set_text(cx, "未选择任务");
            light_stage(cx, &self.ui, "");
        }
        let (octos_name, cursor_name) = agent_button_text(&snap.runtime);
        self.ui.button(cx, ids!(use_octos)).set_text(cx, octos_name);
        self.ui
            .button(cx, ids!(use_cursor))
            .set_text(cx, cursor_name);
        self.ui.label(cx, ids!(runtime)).set_text(cx, &snap.runtime);
        self.ui
            .label(cx, ids!(composer_agent))
            .set_text(cx, &snap.runtime);
        self.ui
            .label(cx, ids!(composer_device))
            .set_text(cx, &snap.board_status);
        self.transcript = snap.transcript.clone();
        *CHAT.lock().expect("chat") = chat_lines(&snap.transcript);
        let chat_empty = snap.transcript.trim().is_empty();
        self.ui
            .widget(cx, ids!(empty_chat))
            .set_visible(cx, chat_empty);
        self.ui
            .widget(cx, ids!(chat_slot))
            .set_visible(cx, !chat_empty);
        self.ui
            .label(cx, ids!(path_full))
            .set_text(cx, &snap.workspace);
        let no_tasks = snap.tasks.is_empty();
        self.ui
            .widget(cx, ids!(empty_tasks))
            .set_visible(cx, no_tasks);
        self.ui
            .widget(cx, ids!(task_list))
            .set_visible(cx, !no_tasks);
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
        show_tone(cx, &self.ui, true, link_tone(&snap.octos_status));
        show_tone(cx, &self.ui, false, link_tone(&snap.cursor_status));
        self.ui
            .label(cx, ids!(board_status))
            .set_text(cx, &snap.board_status);
        let workspace = self.ui.text_input(cx, ids!(workspace));
        if workspace.text() != snap.workspace {
            workspace.set_text(cx, &snap.workspace);
        }
        self.ui
            .label(cx, ids!(detail_workspace))
            .set_text(cx, &snap.workspace);
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

    fn apply_columns(&self, cx: &mut Cx) {
        set_view_size(cx, &self.ui, ids!(task_pane), Some(self.left_w), None);
        set_view_size(cx, &self.ui, ids!(board_body), Some(self.right_w), None);
        self.ui.redraw(cx);
    }

    fn clamp_columns(&mut self) {
        const MIN_CENTER: f64 = 400.0;
        const CHROME: f64 = 36.0;
        let w = self.window_w.max(960.0);
        self.left_w = self.left_w.clamp(220.0, 420.0);
        self.right_w = self.right_w.clamp(260.0, 480.0);
        while self.left_w + self.right_w + MIN_CENTER + CHROME > w {
            if self.left_w > self.right_w && self.left_w > 220.0 {
                self.left_w -= 4.0;
            } else if self.right_w > 260.0 {
                self.right_w -= 4.0;
            } else {
                break;
            }
        }
    }

    fn on_window_width(&mut self, cx: &mut Cx, width: f64) {
        self.window_w = width.max(960.0);
        self.clamp_columns();
        self.apply_columns(cx);
    }

    fn sync_column_grips(&self, cx: &mut Cx) {
        let left_on = self.ui.view(cx, ids!(task_pane)).visible()
            && self.ui.view(cx, ids!(chat_pane)).visible();
        let right_on = self.ui.view(cx, ids!(chat_pane)).visible()
            && self.ui.view(cx, ids!(board_body)).visible();
        self.ui.view(cx, ids!(col_grip_l)).set_visible(cx, left_on);
        self.ui.view(cx, ids!(col_grip_r)).set_visible(cx, right_on);
    }

    fn handle_column_grips(&mut self, cx: &mut Cx, event: &Event) {
        for (grip, side) in [
            (ids!(col_grip_l), ColumnDrag::Left),
            (ids!(col_grip_r), ColumnDrag::Right),
        ] {
            if !self.ui.view(cx, grip).visible() {
                continue;
            }
            let area = self.ui.view(cx, grip).area();
            match event.hits(cx, area) {
                Hit::FingerDown(e) => {
                    self.col_drag = Some(side);
                    self.col_drag_x = e.abs.x;
                }
                Hit::FingerUp(_) => self.col_drag = None,
                Hit::FingerMove(e) if self.col_drag == Some(side) => {
                    let dx = e.abs.x - self.col_drag_x;
                    self.col_drag_x = e.abs.x;
                    match side {
                        ColumnDrag::Left => self.left_w += dx,
                        ColumnDrag::Right => self.right_w -= dx,
                    }
                    self.clamp_columns();
                    self.apply_columns(cx);
                }
                _ => {}
            }
        }
    }

    fn show_nav(&self, cx: &mut Cx, which: &str) {
        for (on_id, idle_id, selected) in [
            (ids!(nav_work), ids!(nav_work_idle), which == "work"),
            (ids!(nav_tasks), ids!(nav_tasks_idle), which == "tasks"),
            (
                ids!(nav_devices),
                ids!(nav_devices_idle),
                which == "devices",
            ),
        ] {
            self.ui.widget(cx, on_id).set_visible(cx, selected);
            self.ui.widget(cx, idle_id).set_visible(cx, !selected);
        }
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
        self.clamp_columns();
        self.apply_columns(cx);
        self.sync_column_grips(cx);
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
                    "设备栏开着"
                } else {
                    "设备栏关着"
                },
            );
            self.sync_column_grips(cx);
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
        if let Some(text) = self.ui.text_input(cx, ids!(search)).changed(actions) {
            if !text.trim().is_empty() {
                self.ui.text_input(cx, ids!(search)).set_text(cx, "");
                self.ui
                    .label(cx, ids!(hint))
                    .set_text(cx, "搜索还没有接入，不会过滤任务或文件。");
            }
        }
        if self.ui.button(cx, ids!(nav_work)).clicked(actions)
            || self.ui.button(cx, ids!(nav_work_idle)).clicked(actions)
        {
            self.ui.view(cx, ids!(task_pane)).set_visible(cx, true);
            self.ui.view(cx, ids!(chat_pane)).set_visible(cx, true);
            self.ui
                .view(cx, ids!(board_body))
                .set_visible(cx, self.board_open);
            self.show_nav(cx, "work");
            self.sync_column_grips(cx);
            self.ui
                .label(cx, ids!(nav_current))
                .set_text(cx, "当前：工作台");
        }
        if self.ui.button(cx, ids!(attach)).clicked(actions) {
            self.ui
                .label(cx, ids!(hint))
                .set_text(cx, "附件还没有接入，不会上传文件。");
        }
        if self.ui.button(cx, ids!(nav_tasks)).clicked(actions)
            || self.ui.button(cx, ids!(nav_tasks_idle)).clicked(actions)
        {
            self.ui.view(cx, ids!(task_pane)).set_visible(cx, true);
            self.ui.view(cx, ids!(chat_pane)).set_visible(cx, true);
            self.ui.view(cx, ids!(board_body)).set_visible(cx, false);
            self.show_nav(cx, "tasks");
            self.sync_column_grips(cx);
            self.ui
                .label(cx, ids!(nav_current))
                .set_text(cx, "当前：任务");
        }
        if self.ui.button(cx, ids!(nav_devices)).clicked(actions)
            || self.ui.button(cx, ids!(nav_devices_idle)).clicked(actions)
        {
            self.ui.view(cx, ids!(board_body)).set_visible(cx, true);
            self.ui.view(cx, ids!(chat_pane)).set_visible(cx, false);
            self.show_nav(cx, "devices");
            self.sync_column_grips(cx);
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
        if let Event::WindowGeomChange(change) = event {
            let raw = change.new_geom.inner_size.x;
            let dpi = change.new_geom.dpi_factor.max(1.0);
            let logical = raw / dpi;
            let width = if dpi > 1.05 && (640.0..=4000.0).contains(&logical) {
                logical
            } else {
                raw
            };
            self.on_window_width(cx, width);
        }
        self.handle_column_grips(cx, event);
        let grip = self.ui.view(cx, ids!(composer_grip)).area();
        match event.hits(cx, grip) {
            Hit::FingerDown(e) => {
                self.composer_drag = true;
                self.drag_y = e.abs.y;
                self.drag_h = self.composer_h;
            }
            Hit::FingerUp(_) => self.composer_drag = false,
            Hit::FingerMove(e) if self.composer_drag => {
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

fn light_stage(cx: &mut Cx, ui: &WidgetRef, status: &str) {
    // 0 规划 … 6 完成。返回 7 表示七步都已完成。没有对应状态时不打勾。
    let cursor: i32 = if status.contains("完成") {
        7
    } else if status.contains("验证") {
        5
    } else if status.contains("审批") {
        3
    } else if status.contains("运行") {
        2
    } else if status.contains("规划")
        || status.contains("草稿")
        || status.contains("重编排")
        || status.contains("暂停")
    {
        0
    } else {
        -1
    };
    let steps = [
        (
            ids!(stage_plan_done),
            ids!(stage_plan_on),
            ids!(stage_plan_idle),
        ),
        (
            ids!(stage_env_done),
            ids!(stage_env_on),
            ids!(stage_env_idle),
        ),
        (
            ids!(stage_build_done),
            ids!(stage_build_on),
            ids!(stage_build_idle),
        ),
        (
            ids!(stage_approve_done),
            ids!(stage_approve_on),
            ids!(stage_approve_idle),
        ),
        (
            ids!(stage_link_done),
            ids!(stage_link_on),
            ids!(stage_link_idle),
        ),
        (
            ids!(stage_verify_done),
            ids!(stage_verify_on),
            ids!(stage_verify_idle),
        ),
        (
            ids!(stage_finish_done),
            ids!(stage_finish_on),
            ids!(stage_finish_idle),
        ),
    ];
    for (index, (done, on, idle)) in steps.into_iter().enumerate() {
        let index = index as i32;
        ui.widget(cx, done)
            .set_visible(cx, cursor == 7 || (cursor >= 0 && index < cursor));
        ui.widget(cx, on)
            .set_visible(cx, cursor >= 0 && cursor < 7 && index == cursor);
        ui.widget(cx, idle)
            .set_visible(cx, cursor < 0 || index > cursor);
    }
    let note = if cursor < 0 {
        if status.is_empty() {
            ""
        } else {
            status
        }
    } else {
        ""
    };
    ui.label(cx, ids!(stage_note)).set_text(cx, note);
}

fn set_view_size(
    cx: &mut Cx,
    ui: &WidgetRef,
    id: &[LiveId],
    width: Option<f64>,
    height: Option<f64>,
) {
    let widget = ui.widget(cx, id);
    let Some(mut view) = widget.borrow_mut::<View>() else {
        return;
    };
    if let Some(width) = width {
        view.walk.width = Size::Fixed(width);
    }
    if let Some(height) = height {
        view.walk.height = Size::Fixed(height);
    }
    view.redraw(cx);
}

fn chat_lines(transcript: &str) -> Vec<ChatLine> {
    transcript
        .split("\n\n")
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (kind, who, body) = split_speaker(line);
            ChatLine {
                kind,
                who: who.to_string(),
                body: body.to_string(),
            }
        })
        .collect()
}

fn split_speaker(line: &str) -> (u8, &str, &str) {
    for (kind, who, prefix) in [
        (0, "你", "你："),
        (1, "Octos", "Octos："),
        (2, "Cursor", "Cursor："),
        (3, "livstyle", "livstyle："),
    ] {
        if let Some(body) = line.strip_prefix(prefix) {
            return (kind, who, body);
        }
    }
    (3, "记录", line)
}

fn agent_button_text(runtime: &str) -> (&'static str, &'static str) {
    let current = runtime
        .split('：')
        .nth(1)
        .unwrap_or("")
        .split('，')
        .next()
        .unwrap_or("")
        .trim();
    let octos = if current.contains("Octos") {
        "Octos  当前"
    } else {
        "Octos"
    };
    let cursor = if current.contains("Cursor") {
        "Cursor  当前"
    } else {
        "Cursor"
    };
    (octos, cursor)
}

fn link_tone(text: &str) -> u8 {
    const BAD: &[&str] = &["没连上", "失败", "没有", "不在", "断", "没打开", "握手失败"];
    if BAD.iter().any(|word| text.contains(word)) {
        return 2;
    }
    if text.contains("正在检查") || text.trim().is_empty() {
        return 0;
    }
    const OK: &[&str] = &["已连接", "已配置", "已接通", "已打开", "会话已开", "已恢复"];
    if OK.iter().any(|word| text.contains(word))
        || text.starts_with("Octos")
        || text.starts_with("Cursor")
    {
        return 1;
    }
    0
}

fn show_tone(cx: &mut Cx, ui: &WidgetRef, octos: bool, tone: u8) {
    let (wait, ok, bad) = if octos {
        (ids!(octos_wait), ids!(octos_ok), ids!(octos_bad))
    } else {
        (ids!(cursor_wait), ids!(cursor_ok), ids!(cursor_bad))
    };
    ui.view(cx, wait).set_visible(cx, tone == 0);
    ui.view(cx, ok).set_visible(cx, tone == 1);
    ui.view(cx, bad).set_visible(cx, tone == 2);
}

fn status_color(status: &str) -> Vec4 {
    if status.contains("失败") || status.contains("取消") {
        vec4(1.0, 0.361, 0.459, 1.0)
    } else if status.contains("完成") {
        vec4(0.0, 0.839, 0.627, 1.0)
    } else if status.contains("运行") || status.contains("规划") || status.contains("验证") {
        vec4(0.0, 0.784, 1.0, 1.0)
    } else if status.contains("未知")
        || status.contains("审批")
        || status.contains("暂停")
        || status.contains("受阻")
    {
        vec4(1.0, 0.710, 0.278, 1.0)
    } else {
        vec4(0.557, 0.659, 0.780, 1.0)
    }
}
