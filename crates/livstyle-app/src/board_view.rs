//! Offscreen 3D board. Drag orbits it; a linked serial port lights the USB plug.

use std::sync::Mutex;

use makepad_widgets::*;

pub struct BoardVisual {
    pub ports: u32,
    pub identified: bool,
}

pub static BOARD: Mutex<BoardVisual> = Mutex::new(BoardVisual {
    ports: 0,
    identified: false,
});

pub fn set_board_visual(status: &str) {
    let identified = status.to_ascii_lowercase().contains("esp");
    let ports = if identified {
        1
    } else if status.contains("没有串口") {
        0
    } else {
        status
            .split_whitespace()
            .find_map(|word| word.parse::<u32>().ok())
            .unwrap_or(0)
    };
    if let Ok(mut board) = BOARD.lock() {
        board.ports = ports;
        board.identified = identified;
    }
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.math.*
    use mod.shader.*
    use mod.draw
    use mod.geom

    mod.draw.DrawBoardScene = mod.std.set_type_default() do #(DrawBoardScene::script_shader(vm)){
        ..mod.draw.DrawQuad
        scene_texture: texture_2d(float)
        tint: vec4(1.0, 1.0, 1.0, 1.0)

        pixel: fn() {
            let color = self.scene_texture.sample_as_bgra(self.pos)
            return Pal.premul(color * self.tint)
        }
    }

    mod.widgets.BoardStageBase = #(BoardStage::register_widget(vm))
    mod.widgets.BoardStage = set_type_default() do mod.widgets.BoardStageBase{
        width: Fill
        height: Fill
        clear_color: #x0b1220
        draw_bg: mod.draw.DrawBoardScene{}
        draw_cube: mod.draw.DrawCube{}
    }
}

#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawBoardScene {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    tint: Vec4f,
}

impl DrawBoardScene {
    fn set_scene_texture(&mut self, texture: &Texture) {
        self.draw_super.draw_vars.set_texture(0, texture);
    }

    fn draw_abs(&mut self, cx: &mut Cx2d, rect: Rect) {
        self.draw_super.draw_abs(cx, rect);
    }
}

#[derive(Script, ScriptHook, WidgetRef, WidgetRegister)]
pub struct BoardStage {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[live]
    draw_bg: DrawBoardScene,
    #[live]
    draw_cube: DrawCube,
    #[live(vec4(0.043, 0.071, 0.125, 1.0))]
    clear_color: Vec4f,
    #[new]
    pass: DrawPass,
    #[new]
    draw_list: DrawList,
    #[new]
    color_texture: Texture,
    #[new]
    depth_texture: Texture,
    #[rust]
    area: Area,
    #[rust(false)]
    initialized: bool,
    #[rust(0.6)]
    yaw: f32,
    #[rust(0.42)]
    pitch: f32,
    #[rust(false)]
    dragging: bool,
    #[rust]
    drag_last: DVec2,
    #[rust]
    next_frame: NextFrame,
}

impl BoardStage {
    fn ensure_initialized(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        self.color_texture = Texture::new_with_format(
            cx,
            TextureFormat::RenderBGRAu8 {
                size: TextureSize::Auto,
                initial: true,
            },
        );
        self.depth_texture = Texture::new_with_format(
            cx,
            TextureFormat::DepthD32 {
                size: TextureSize::Auto,
                initial: true,
            },
        );
        self.pass.set_color_texture(
            cx,
            &self.color_texture,
            DrawPassClearColor::ClearWith(self.clear_color),
        );
        self.pass
            .set_depth_texture(cx, &self.depth_texture, DrawPassClearDepth::ClearWith(1.0));
        cx.passes[self.pass.draw_pass_id()].keep_camera_matrix = true;
        self.next_frame = cx.new_next_frame();
    }

    fn scene(&self, rect: Rect, time: f64) -> SceneState3D {
        let aspect = (rect.size.x / rect.size.y).max(0.001) as f32;
        let distance = 4.4;
        let yaw = self.yaw;
        let pitch = self.pitch.clamp(-0.2, 1.05);
        let eye = vec3(
            yaw.sin() * pitch.cos() * distance,
            pitch.sin() * distance + 0.35,
            yaw.cos() * pitch.cos() * distance,
        );
        let target = vec3(0.0, 0.05, 0.0);
        SceneState3D {
            time,
            camera_pos: eye,
            view: Mat4f::look_at(eye, target, vec3(0.0, 1.0, 0.0)),
            projection: Mat4f::perspective(32.0, aspect, 0.05, 40.0),
            viewport_rect: rect,
        }
    }

    fn cube(&mut self, cx: &mut CxDraw, pos: Vec3f, size: Vec3f, color: Vec4f) {
        self.draw_cube.transform = Mat4f::identity();
        self.draw_cube.cube_pos = pos;
        self.draw_cube.cube_size = size;
        self.draw_cube.color = color;
        self.draw_cube.depth_clip = 0.0;
        self.draw_cube.light_dir = vec3(-0.35, 0.86, 0.38);
        self.draw_cube.draw(cx);
    }

    fn draw_board(&mut self, cx: &mut Cx3d, scene: SceneState3D) {
        let (ports, identified) = BOARD
            .lock()
            .map(|board| (board.ports, board.identified))
            .unwrap_or((0, false));
        let linked = ports > 0 || identified;
        let pulse = 0.55 + 0.45 * (scene.time as f32 * 2.4).sin().abs();

        self.draw_list.begin_always(cx);
        self.draw_list
            .set_view_transform(cx, &Mat4f::rotation(vec3(self.pitch * 0.15, self.yaw, 0.0)));
        cx.begin_scene_3d(scene);

        self.cube(
            cx,
            vec3(0.0, 0.0, 0.0),
            vec3(2.35, 0.07, 1.28),
            vec4(0.04, 0.16, 0.13, 1.0),
        );
        self.cube(
            cx,
            vec3(0.0, 0.045, 0.0),
            vec3(2.15, 0.012, 1.08),
            vec4(0.03, 0.22, 0.16, 1.0),
        );
        let chip = if identified {
            vec4(0.86, 0.91, 0.94, 1.0)
        } else {
            vec4(0.22, 0.27, 0.31, 1.0)
        };
        self.cube(cx, vec3(-0.15, 0.09, -0.05), vec3(0.72, 0.1, 0.72), chip);
        self.cube(
            cx,
            vec3(-0.15, 0.15, -0.05),
            vec3(0.28, 0.04, 0.28),
            vec4(0.1, 0.12, 0.14, 1.0),
        );
        let usb = if linked {
            vec4(0.45 * pulse, 0.68 * pulse, 1.0 * pulse, 1.0)
        } else {
            vec4(0.16, 0.2, 0.24, 1.0)
        };
        self.cube(cx, vec3(0.0, 0.08, 0.72), vec3(0.42, 0.12, 0.22), usb);
        let plugs = ports.clamp(0, 3);
        for i in 0..plugs {
            let x = -0.55 + i as f32 * 0.28;
            self.cube(
                cx,
                vec3(x, 0.07, 0.78),
                vec3(0.08, 0.08, 0.08),
                vec4(0.55, 0.75, 1.0, 1.0),
            );
        }
        self.cube(
            cx,
            vec3(-1.02, 0.06, 0.0),
            vec3(0.08, 0.06, 0.92),
            vec4(0.72, 0.74, 0.62, 1.0),
        );
        self.cube(
            cx,
            vec3(1.02, 0.06, 0.0),
            vec3(0.08, 0.06, 0.92),
            vec4(0.72, 0.74, 0.62, 1.0),
        );
        if identified {
            self.cube(
                cx,
                vec3(0.85, 0.28, -0.42),
                vec3(0.03, 0.42, 0.03),
                vec4(0.15, 0.85, 0.72, 1.0),
            );
        }

        cx.end_scene_3d();
        self.draw_list.end(cx);
    }
}

fn set_pass_camera(cx: &mut Cx, pass: &DrawPass, scene: &SceneState3D) {
    let camera_inv = scene.view.invert();
    let pass_uniforms = &mut cx.passes[pass.draw_pass_id()].pass_uniforms;
    pass_uniforms.camera_projection = scene.projection;
    pass_uniforms.camera_projection_r = scene.projection;
    pass_uniforms.camera_view = scene.view;
    pass_uniforms.camera_view_r = scene.view;
    pass_uniforms.depth_projection = scene.projection;
    pass_uniforms.depth_projection_r = scene.projection;
    pass_uniforms.depth_view = scene.view;
    pass_uniforms.depth_view_r = scene.view;
    pass_uniforms.camera_inv = camera_inv;
    pass_uniforms.camera_inv_r = camera_inv;
}

impl WidgetNode for BoardStage {
    fn widget_uid(&self) -> WidgetUid {
        self.uid
    }

    fn walk(&mut self, _cx: &mut Cx) -> Walk {
        self.walk
    }

    fn area(&self) -> Area {
        self.area
    }

    fn redraw(&mut self, cx: &mut Cx) {
        self.area.redraw(cx);
    }
}

impl Widget for BoardStage {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        match event.hits(cx, self.area) {
            Hit::FingerDown(e) => {
                self.dragging = true;
                self.drag_last = e.abs;
            }
            Hit::FingerUp(_) => self.dragging = false,
            Hit::FingerMove(e) if self.dragging => {
                let dx = (e.abs.x - self.drag_last.x) as f32;
                let dy = (e.abs.y - self.drag_last.y) as f32;
                self.yaw += dx * 0.012;
                self.pitch = (self.pitch + dy * 0.008).clamp(0.15, 1.05);
                self.drag_last = e.abs;
                self.area.redraw(cx);
            }
            _ => {}
        }
        if self.next_frame.is_event(event).is_some() {
            if !self.dragging {
                self.yaw += 0.004;
            }
            self.next_frame = cx.new_next_frame();
            self.area.redraw(cx);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let rect = cx.walk_turtle_with_area(&mut self.area, walk);
        if rect.size.x <= 1.0 || rect.size.y <= 1.0 {
            return DrawStep::done();
        }
        self.ensure_initialized(cx.cx);
        self.pass.set_size(cx, rect.size);
        self.pass.set_color_texture(
            cx,
            &self.color_texture,
            DrawPassClearColor::ClearWith(self.clear_color),
        );
        self.pass
            .set_depth_texture(cx, &self.depth_texture, DrawPassClearDepth::ClearWith(1.0));

        let scene = self.scene(rect, cx.time());
        cx.make_child_pass(&self.pass);
        cx.begin_pass(&self.pass, None);
        set_pass_camera(cx.cx, &self.pass, &scene);
        self.draw_board(&mut Cx3d::new(cx.cx), scene);
        cx.end_pass(&self.pass);

        self.draw_bg.set_scene_texture(&self.color_texture);
        self.draw_bg.draw_abs(cx, rect);
        self.area = self.draw_bg.draw_super.area;
        cx.set_pass_area(&self.pass, self.area);
        DrawStep::done()
    }
}
