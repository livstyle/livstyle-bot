//! Guided ESP32 DIY desk. Wiring checks are deterministic.
//! A user tap records a confirmation; it does not become an observed electrical state.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Canvas {
    Wiring,
    Camera,
    Scene,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    Observed,
    Simulated,
    Expected,
    Unknown,
}

impl Evidence {
    pub fn label(self) -> &'static str {
        match self {
            Evidence::Observed => "实测",
            Evidence::Simulated => "模拟",
            Evidence::Expected => "预期",
            Evidence::Unknown => "未知",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    NeedsConfirm(&'static str),
    Reject(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wire {
    pub color: &'static str,
    pub from_part: &'static str,
    pub from_pin: &'static str,
    pub to_part: &'static str,
    pub to_pin: &'static str,
    pub gpio: Option<u8>,
    pub action: &'static str,
    pub why: &'static str,
    pub safety: &'static str,
    pub verify: &'static str,
    pub steps: [&'static str; 3],
}

pub const PHASES: [&str; 7] = [
    "需求确认",
    "硬件识别与准备",
    "接线",
    "代码生成",
    "构建与部署",
    "实时调试",
    "验收与保存",
];

const DHT11: [Wire; 3] = [
    Wire {
        color: "红色",
        from_part: "DHT11",
        from_pin: "VCC",
        to_part: "开发板",
        to_pin: "3V3",
        gpio: None,
        action: "把红色杜邦线接到开发板 3V3，先不要接 VIN。",
        why: "这个配方只使用 3.3V 供电。VIN 通常是 5V，接错会损坏模块。",
        safety: "断电再插线。确认红线没有碰到 VIN，也没有同时碰到旁边的脚。",
        verify: "目视：红线一端在 DHT11 的 VCC，另一端在丝印 3V3。这不是万用表测量。",
        steps: [
            "找到开发板上标有 3V3 的引脚",
            "将红色杜邦线插入 3V3",
            "保持 VIN 暂时不连接",
        ],
    },
    Wire {
        color: "黄色",
        from_part: "DHT11",
        from_pin: "DATA",
        to_part: "开发板",
        to_pin: "GPIO4",
        gpio: Some(4),
        action: "把黄色杜邦线从 DHT11 的 DATA 接到 GPIO4。",
        why: "配方把数据脚固定在 GPIO4。没有板卡确认时，不能改接到别的脚。",
        safety: "不要接到丝印为 BOOT、USB 或闪存占用的脚。GPIO4 只在当前这份草稿资料里未被标为保留。",
        verify: "目视：黄线两端是 DHT11 DATA 和 GPIO4。板卡资料仍是草稿时，结果保持需要确认。",
        steps: [
            "找到 GPIO4，不要找 VIN",
            "黄色线的另一端留在 DHT11 的 DATA",
            "旁边的脚保持空着",
        ],
    },
    Wire {
        color: "黑色",
        from_part: "DHT11",
        from_pin: "GND",
        to_part: "开发板",
        to_pin: "GND",
        gpio: None,
        action: "把黑色杜邦线从 DHT11 的 GND 接到开发板 GND。",
        why: "没有共地时，后面的电平没有参考，传感器读数不能当成测量。",
        safety: "黑线只接 GND。不要把 GND 接到 3V3 或 VIN。",
        verify: "目视：黑线两端都在 GND 丝印上。还没有导通测量。",
        steps: [
            "找到开发板 GND",
            "黑色线插入 GND",
            "确认没有和红色线换位",
        ],
    },
];

#[derive(Clone, Debug)]
struct Session {
    phase: usize,
    wire: usize,
    wire_ack: [bool; 3],
    canvas: Canvas,
    camera: bool,
    handed_code: bool,
}

#[derive(Clone, Debug)]
pub struct Desk {
    session: Option<Session>,
    note: String,
}

impl Default for Desk {
    fn default() -> Self {
        Self::home()
    }
}

#[derive(Clone, Debug)]
pub struct WireRow {
    pub text: String,
    pub current: bool,
}

#[derive(Clone, Debug)]
pub struct Face {
    pub home: bool,
    pub title: String,
    pub subtitle: String,
    pub step_label: String,
    pub phases: Vec<String>,
    pub action: String,
    pub why: String,
    pub safety: String,
    pub verify: String,
    pub steps: String,
    pub wires: Vec<WireRow>,
    pub legend: String,
    pub temp: String,
    pub humidity: String,
    pub wifi: String,
    pub gpio: String,
    pub evidence_note: String,
    pub camera: String,
    pub scene: String,
    pub info: String,
    pub note: String,
    pub canvas: Canvas,
    pub show_wiring: bool,
}

pub enum Confirm {
    Moved(String),
    Stay(String),
}

impl Desk {
    pub fn tell(&mut self, note: impl Into<String>) {
        self.note = note.into();
    }

    pub fn home() -> Self {
        Self {
            session: None,
            note: "选一个已经写好接线清单的作品。没有清单的目标只会交给导师，不会猜引脚。".into(),
        }
    }

    pub fn open_supported(&mut self) {
        self.session = Some(Session {
            phase: 1,
            wire: 0,
            wire_ack: [false; 3],
            canvas: Canvas::Wiring,
            camera: false,
            handed_code: false,
        });
        self.note = "需求已记下：ESP32 温湿度。下一步是确认板上有串口，还不能接线。".into();
    }

    pub fn unsupported(name: &str) -> String {
        format!("{name}还没有经过核对的接线清单。界面不会给出引脚，也不会把它标成可以烧录。")
    }

    pub fn set_canvas(&mut self, canvas: Canvas) {
        if let Some(session) = self.session.as_mut() {
            session.canvas = canvas;
        } else {
            self.note = "先打开温湿度项目，再切换接线、实物或三维视图。".into();
        }
    }

    /// Workbench is the wiring desk. It does not stay on the 3D canvas.
    pub fn show_workbench(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.canvas = Canvas::Wiring;
        }
    }

    /// 3D debug shows the recipe diagram. It is not a second copy of the goal form.
    pub fn show_scene(&mut self) {
        if self.session.is_none() {
            self.open_supported();
        }
        if let Some(session) = self.session.as_mut() {
            session.canvas = Canvas::Scene;
        }
        self.note = "三维调试是温湿度配方的示意图。要改目标，回到工作台。示意图不能代替串口或烧录结果。".into();
    }

    pub fn set_camera(&mut self, on: bool) {
        if let Some(session) = self.session.as_mut() {
            session.camera = on;
            if on {
                self.note = "摄像头只在你打开后使用。这一版没有采集画面，也不会上传。".into();
            } else {
                self.note = "摄像头已关闭。".into();
            }
        }
    }

    pub fn help_prompt(&self) -> String {
        let Some(session) = &self.session else {
            return "我想做一个硬件作品，但还没选定已验证的配方。请先问我开发板和传感器型号，不要猜引脚。".into();
        };
        let phase = PHASES[session.phase];
        if session.phase == 2 {
            let wire = &DHT11[session.wire];
            format!(
                "温湿度配方卡在接线。当前只做这一根线：{} {} 的 {} 到 {} 的 {}。我做不到。请只排查这一根，不要改引脚，也不要宣布已经烧录。",
                wire.color, wire.from_part, wire.from_pin, wire.to_part, wire.to_pin
            )
        } else {
            format!("温湿度配方卡在「{phase}」。请根据这一步继续问我，不要假设设备已经跑起来。")
        }
    }

    pub fn code_prompt(&mut self) -> Result<String, String> {
        let Some(session) = self.session.as_mut() else {
            return Err("还没有项目。".into());
        };
        if session.phase < 3 {
            return Err("接线和开发板确认还没做完，先不生成代码。".into());
        }
        session.handed_code = true;
        session.phase = 4;
        self.note = "配方已交给当前导师。对话里出现代码也不等于已经编译或写入开发板。".into();
        Ok(
            "请为已确认的 ESP32 + DHT11 配方准备代码。接线固定为 VCC→3V3、DATA→GPIO4、GND→GND，不要接 VIN。不要把工程改成 Toit，除非我明确同意。写完后停下来，不要宣称已经烧录或已经读到温湿度。"
                .into(),
        )
    }

    pub fn deploy_gate(&self) -> Result<(), String> {
        let Some(session) = &self.session else {
            return Err("还没有项目。".into());
        };
        if session.phase < 4 {
            return Err("还没到部署。这一步不会写设备。".into());
        }
        Ok(())
    }

    pub fn check_report(&self, board_status: &str) -> String {
        let Some(session) = &self.session else {
            return "还没有接线清单。".into();
        };
        if session.phase < 2 {
            return "还在确认开发板。没有接线可以检查。".into();
        }
        let mut lines = Vec::new();
        for (index, wire) in DHT11.iter().enumerate() {
            let mark = if session.wire_ack[index] {
                "已让你确认"
            } else if session.phase == 2 && session.wire == index {
                "当前这根"
            } else {
                "还没轮到"
            };
            let check = check_wire(wire, board_status);
            lines.push(format!(
                "{mark}  {} {}→{}：{}",
                wire.color,
                wire.from_pin,
                wire.to_pin,
                check_text(check)
            ));
        }
        lines.push("检查不会把目视确认升级成实测电压。".into());
        lines.join("\n")
    }

    pub fn confirm(&mut self, board_status: &str, log: &str) -> Confirm {
        let Some(session) = self.session.as_mut() else {
            return Confirm::Stay("先在首页选温湿度监测器。".into());
        };
        match session.phase {
            1 => {
                if !saw_port(board_status) {
                    let detail: String =
                        "没有看到串口。先点「刷新串口」。型号不确定时不能开始接线。".into();
                    self.note = detail.clone();
                    return Confirm::Stay(detail);
                }
                session.phase = 2;
                session.wire = 0;
                let detail = "串口有记录。开始第一根线。串口出现不等于引脚已经核对。".to_string();
                self.note = detail.clone();
                Confirm::Moved(detail)
            }
            2 => {
                let wire = DHT11[session.wire];
                match check_wire(&wire, board_status) {
                    Check::Reject(reason) => {
                        self.note = reason.into();
                        Confirm::Stay(reason.into())
                    }
                    Check::NeedsConfirm(_) => {
                        let index = session.wire;
                        session.wire_ack[index] = true;
                        if index + 1 < DHT11.len() {
                            session.wire += 1;
                            let detail = format!(
                                "已记下你对{}线的目视确认。电气结果仍是需要确认。下一根：{}。",
                                wire.color,
                                DHT11[session.wire].action
                            );
                            self.note = detail.clone();
                            Confirm::Moved(detail)
                        } else {
                            session.phase = 3;
                            let detail = "三根线都有你的确认。还没有电压测量，代码也还没生成。".to_string();
                            self.note = detail.clone();
                            Confirm::Moved(detail)
                        }
                    }
                }
            }
            3 => {
                let detail = "代码这一步不能只点完成。用「交给导师」，等对话里出现结果。".to_string();
                self.note = detail.clone();
                Confirm::Stay(detail)
            }
            4 => {
                let detail = "准备部署打开烧录确认。没有端口、芯片或产物摘要时不会写设备，这一步也不会标成成功。".to_string();
                self.note = detail.clone();
                Confirm::Stay(detail)
            }
            5 => {
                let (temp, evidence) = temperature(log);
                if evidence != Evidence::Observed {
                    let detail = format!("串口里还没有温度读数（{temp}）。不能进入验收。");
                    self.note = detail.clone();
                    return Confirm::Stay(detail);
                }
                session.phase = 6;
                let detail = format!("串口里读到温度 {temp}。可以看验收说明。烧录是否成功仍要另有工具返回。");
                self.note = detail.clone();
                Confirm::Moved(detail)
            }
            _ => {
                let detail = "验收停在本机说明里。没有新的串口测量，就不会写成作品已完成。".to_string();
                self.note = detail.clone();
                Confirm::Stay(detail)
            }
        }
    }

    /// Serial text that actually contains a sensor reading opens the debug phase.
    /// Scanning the port does not.
    pub fn note_log(&mut self, log: &str) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.phase == 4 && temperature(log).1 == Evidence::Observed {
            session.phase = 5;
            self.note = "串口里出现了温度字段，调试阶段改看这段日志。它还不能单独证明烧录成功。".into();
        }
    }

    pub fn face(&self, board_status: &str, log: &str) -> Face {
        let (temp, temp_ev) = temperature(log);
        let (humidity, hum_ev) = humidity(log);
        let (wifi, wifi_ev) = wifi(log);
        let (gpio, gpio_ev) = gpio4(log);
        let Some(session) = &self.session else {
            return Face {
                home: true,
                title: "你想做一个什么作品？".into(),
                subtitle: "先选有接线清单的项目。点亮 LED 和人体感应灯还没有核对过的引脚表。".into(),
                step_label: String::new(),
                phases: PHASES.iter().map(|item| format!("○ {item}")).collect(),
                action: "选温湿度监测器，或先描述你的目标。没有接线清单的目标不会进入分步指导。".into(),
                why: "导师一次只推进一件已经写进清单的事。".into(),
                safety: "还没确认开发板之前，不要按示意图把线插到 VIN。".into(),
                verify: "温湿度项目打开后，第一步是看到串口，不是看到温度数字。".into(),
                steps: "1. 点温湿度监测器\n2. 或在上面写下目标\n3. LED 和人体感应灯没有引脚表".into(),
                wires: Vec::new(),
                legend: legend(),
                temp: format!("{temp} · {}", temp_ev.label()),
                humidity: format!("{humidity} · {}", hum_ev.label()),
                wifi: format!("{wifi} · {}", wifi_ev.label()),
                gpio: format!("{gpio} · {}", gpio_ev.label()),
                evidence_note: "没有项目时不展示传感器成功值。".into(),
                camera: "摄像头关闭。".into(),
                scene: scene_caption(),
                info: info_card(),
                note: self.note.clone(),
                canvas: Canvas::Wiring,
                show_wiring: false,
            };
        };
        let wire = &DHT11[session.wire.min(DHT11.len() - 1)];
        let (action, why, safety, verify, steps) = if session.phase == 2 {
            (
                wire.action.to_string(),
                wire.why.to_string(),
                wire.safety.to_string(),
                wire.verify.to_string(),
                wire.steps
                    .iter()
                    .enumerate()
                    .map(|(i, step)| format!("{}. {step}", i + 1))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        } else {
            phase_copy(session.phase, session.handed_code)
        };
        Face {
            home: false,
            title: "点亮你的第一个传感器".into(),
            subtitle: "ESP32 温湿度。一次只做下面这一件事。".into(),
            step_label: format!("第 {} 步 / {}", session.phase + 1, PHASES.len()),
            phases: PHASES
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let mark = if index < session.phase {
                        "✓"
                    } else if index == session.phase {
                        "●"
                    } else {
                        "○"
                    };
                    format!("{mark} {name}")
                })
                .collect(),
            action,
            why,
            safety,
            verify,
            steps,
            wires: DHT11
                .iter()
                .enumerate()
                .map(|(index, item)| WireRow {
                    text: format!(
                        "{}  {}  {} {} → {} {}",
                        if session.wire_ack[index] {
                            "已确认"
                        } else if session.phase == 2 && session.wire == index {
                            "当前"
                        } else {
                            "等待"
                        },
                        item.color,
                        item.from_part,
                        item.from_pin,
                        item.to_part,
                        item.to_pin
                    ),
                    current: session.phase == 2 && session.wire == index,
                })
                .collect(),
            legend: legend(),
            temp: format!("{temp} · {}", temp_ev.label()),
            humidity: format!("{humidity} · {}", hum_ev.label()),
            wifi: format!("{wifi} · {}", wifi_ev.label()),
            gpio: format!("{gpio} · {}", gpio_ev.label()),
            evidence_note: format!(
                "开发板：{board_status}。接线确认是{}，不是实测电压。",
                Evidence::Expected.label()
            ),
            camera: if session.camera {
                "已允许使用摄像头。本机没有接入预览，画面不会上传，也不能代替烧录结果。".into()
            } else {
                "摄像头关闭。需要时再打开。打开后这一版仍然没有采集画面。".into()
            },
            scene: scene_caption(),
            info: info_card(),
            note: self.note.clone(),
            canvas: session.canvas,
            show_wiring: session.phase >= 2,
        }
    }
}

fn phase_copy(phase: usize, handed: bool) -> (String, String, String, String, String) {
    match phase {
        1 => (
            "先确认开发板已经出现在串口列表里。".into(),
            "接线清单依赖板卡型号。没看到设备时不会猜 3V3 和 GPIO4。".into(),
            "先不要插传感器电源。识别芯片会使开发板复位。".into(),
            "右侧设备状态出现串口路径，或你点了刷新之后不再是「没有串口」。".into(),
            "1. 打开「设备与硬件」或点刷新串口\n2. 看状态是不是仍写着没有串口\n3. 看到端口后再点我已完成".into(),
        ),
        3 => (
            "把固定接线交给导师写代码。不要只点完成。".into(),
            "代码还没有生成。导师写出的文件也还不是设备上的程序。".into(),
            "不要同意把工程改成 Toit，除非你清楚这是另一套运行时。".into(),
            "对话里出现针对 GPIO4 的代码之后，阶段才会进到部署说明。".into(),
            "1. 点「交给导师」\n2. 在 AI 导师页看回复\n3. 不要把回复当成已经烧录".into(),
        ),
        4 => {
            let lead = if handed {
                "配方已经交给导师。"
            } else {
                "还可以先把配方交给导师。"
            };
            (
                format!("{lead}准备部署会打开烧录确认。没有识别过的串口和产物摘要时，不会写设备。"),
                "编译成功不等于温湿度已经读到。确认烧录之后，仍要看工具返回和串口。".into(),
                "只有卡片写明端口、芯片和摘要，并且你点了确认，才会写这块板。".into(),
                "没有弹出可核对的摘要，就说明这次没有烧录。点完成也不会把阶段标成成功。".into(),
                "1. 点准备部署\n2. 看卡片上的端口和摘要\n3. 没有摘要就不要当成已经写入".into(),
            )
        }
        5 => (
            "看串口里的温度字段，不要看示意图。".into(),
            "只有日志里的数字算实测。示意图上的板子是模拟。".into(),
            "调试时保持接线不动，不要带电拔传感器。".into(),
            "日志出现「温度:」或 temp: 后面的数字。".into(),
            "1. 展开下面的设备记录\n2. 找到温度字段\n3. 没有字段就点我遇到问题".into(),
        ),
        _ => (
            "验收只复述已经拿到的证据。".into(),
            "作品可以留在当前任务里继续改。没有新测量就不会标成跑通。".into(),
            "保存不会上传整份工程。".into(),
            "温度来自串口才算读到。烧录成功要另有工具返回，这里没有。".into(),
            "1. 看温度旁边的「实测」或「未知」\n2. 未知就不能当验收通过\n3. 要改代码就回到 AI 导师".into(),
        ),
    }
}

pub fn check_wire(wire: &Wire, board_status: &str) -> Check {
    if wire.to_pin == "VIN" || wire.from_pin == "VIN" {
        return Check::Reject("这条线接到了 VIN。当前配方禁止使用 VIN。");
    }
    if !saw_port(board_status) {
        return Check::NeedsConfirm("还没有串口记录，不能把这根线标成已核对。");
    }
    let board = board_status.to_ascii_lowercase();
    if board.contains("esp32c5") && wire.gpio == Some(4) {
        return Check::NeedsConfirm(
            "ESP32-C5 草稿资料没有把 GPIO4 列为保留脚，但资料未定稿，结果保持需要确认。",
        );
    }
    if !board.contains("esp") {
        return Check::NeedsConfirm("还没有识别到 ESP 芯片，不能默认这根线和当前板匹配。");
    }
    Check::NeedsConfirm("设备字样已出现。没有电压或导通测量，结果保持需要确认。")
}

fn check_text(check: Check) -> String {
    match check {
        Check::NeedsConfirm(text) | Check::Reject(text) => text.into(),
    }
}

fn saw_port(status: &str) -> bool {
    let text = status.to_ascii_lowercase();
    if text.contains("没有串口") || text.trim().is_empty() {
        return false;
    }
    text.contains("tty")
        || text.contains("com")
        || text.contains("esp")
        || text.contains("串口")
}

fn temperature(log: &str) -> (String, Evidence) {
    reading(log, &["温度:", "温度：", "temp:", "Temp:"], "°C")
}

fn humidity(log: &str) -> (String, Evidence) {
    reading(log, &["湿度:", "湿度：", "humidity:", "Humidity:"], "%")
}

fn reading(log: &str, keys: &[&str], unit: &str) -> (String, Evidence) {
    for key in keys {
        if let Some(rest) = log.split(key).nth(1) {
            let token: String = rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !token.is_empty() {
                return (format!("{token}{unit}"), Evidence::Observed);
            }
        }
    }
    ("未观测".into(), Evidence::Unknown)
}

fn wifi(log: &str) -> (String, Evidence) {
    let lower = log.to_ascii_lowercase();
    if lower.contains("wifi_connected") || log.contains("Wi-Fi 已连接") || log.contains("WiFi connected")
    {
        ("已连接".into(), Evidence::Observed)
    } else {
        ("未观测".into(), Evidence::Unknown)
    }
}

fn gpio4(log: &str) -> (String, Evidence) {
    let upper = log.to_ascii_uppercase();
    if upper.contains("GPIO4") || upper.contains("GPIO 4") {
        if upper.contains("HIGH") {
            return ("HIGH".into(), Evidence::Observed);
        }
        if upper.contains("LOW") {
            return ("LOW".into(), Evidence::Observed);
        }
    }
    ("未观测".into(), Evidence::Unknown)
}

fn legend() -> String {
    "红色：3V3 供电\n黄色：GPIO4 数据\n黑色：GND 地线\nVIN：不要接".into()
}

fn scene_caption() -> String {
    format!(
        "三维板是示意图，证据类型：{}。拖动可以转动。它不代表引脚上的真实电压。",
        Evidence::Simulated.label()
    )
}

fn info_card() -> String {
    "ESP32 温湿度\n元件：DHT11\n更新方式：串口烧录尚未在这里执行\nToit/Jaguar：这条配方未验证，不会自动转换".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_never_uses_vin() {
        assert!(DHT11.iter().all(|wire| wire.to_pin != "VIN" && wire.from_pin != "VIN"));
        assert_eq!(DHT11.len(), 3);
        assert_eq!(DHT11[1].gpio, Some(4));
    }

    #[test]
    fn vin_wire_is_rejected() {
        let wire = Wire {
            to_pin: "VIN",
            from_pin: "VCC",
            ..DHT11[0]
        };
        assert!(matches!(check_wire(&wire, "ttyACM0 esp32"), Check::Reject(_)));
    }

    #[test]
    fn known_board_still_needs_confirm() {
        assert!(matches!(
            check_wire(&DHT11[1], "识别到 esp32c5 /dev/ttyACM0"),
            Check::NeedsConfirm(_)
        ));
        assert!(matches!(check_wire(&DHT11[0], "没有串口"), Check::NeedsConfirm(_)));
    }

    #[test]
    fn empty_log_is_unknown_and_sample_log_is_observed() {
        assert_eq!(temperature("串口扫描完成").1, Evidence::Unknown);
        assert_eq!(temperature("温度: 24.8").0, "24.8°C");
        assert_eq!(temperature("温度: 24.8").1, Evidence::Observed);
        assert_eq!(humidity("湿度: 56.2").0, "56.2%");
        assert_eq!(wifi("hello").1, Evidence::Unknown);
        assert_eq!(gpio4("GPIO4 LOW").0, "LOW");
    }

    #[test]
    fn confirm_does_not_invent_a_sensor_reading() {
        let mut desk = Desk::home();
        desk.open_supported();
        assert!(matches!(desk.confirm("没有串口", ""), Confirm::Stay(_)));
        assert!(matches!(
            desk.confirm("/dev/ttyACM0", "扫描到端口"),
            Confirm::Moved(_)
        ));
        let face = desk.face("/dev/ttyACM0", "扫描到端口");
        assert!(face.temp.contains("未观测"));
        assert!(face.temp.contains("未知"));
        assert_eq!(face.canvas, Canvas::Wiring);
    }

    #[test]
    fn wiring_acks_do_not_skip_to_acceptance() {
        let mut desk = Desk::home();
        desk.open_supported();
        desk.confirm("esp32 /dev/ttyUSB0", "");
        for _ in 0..3 {
            desk.confirm("esp32 /dev/ttyUSB0", "");
        }
        let face = desk.face("esp32 /dev/ttyUSB0", "");
        assert!(face.step_label.contains("代码生成") || face.phases[3].starts_with('●'));
        assert!(matches!(desk.confirm("esp32 /dev/ttyUSB0", ""), Confirm::Stay(_)));
        assert!(desk.code_prompt().is_ok());
        assert!(desk.deploy_gate().is_ok());
        assert!(matches!(
            desk.confirm("esp32 /dev/ttyUSB0", ""),
            Confirm::Stay(_)
        ));
        desk.note_log("温度: 24.8 湿度: 40");
        let face = desk.face("esp32 /dev/ttyUSB0", "温度: 24.8 湿度: 40");
        assert!(face.temp.contains("实测"));
        assert!(face.phases[5].starts_with('●') || face.phases[6].starts_with('●') || face.phases[5].starts_with('✓'));
    }

    #[test]
    fn scene_page_is_not_the_goal_form() {
        let mut desk = Desk::home();
        desk.show_scene();
        let face = desk.face("没有串口", "");
        assert!(!face.home);
        assert_eq!(face.canvas, Canvas::Scene);
        desk.show_workbench();
        assert_eq!(desk.face("没有串口", "").canvas, Canvas::Wiring);
    }

    fn camera_starts_off_and_scene_is_simulated() {
        let mut desk = Desk::home();
        desk.open_supported();
        let face = desk.face("没有串口", "");
        assert!(face.camera.contains("关闭"));
        assert!(face.scene.contains("模拟"));
        desk.set_camera(true);
        assert!(desk.face("", "").camera.contains("不会上传"));
    }

    #[test]
    fn led_recipe_is_refused() {
        let text = Desk::unsupported("点亮 LED");
        assert!(text.contains("不会给出引脚"));
    }
}
