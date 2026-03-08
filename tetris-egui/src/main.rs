use eframe::egui::{self, Color32, Key, Pos2, Rect, Stroke, Vec2};
use eframe::{App, Frame, WebOptions};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const BOARD_W: usize = 10;
const BOARD_H: usize = 20;
const BOARD_W_I32: i32 = BOARD_W as i32;
const BOARD_H_I32: i32 = BOARD_H as i32;
const CELL: f32 = 22.0;

#[derive(Clone, Copy)]
struct PieceDef {
    color: Color32,
    rotations: [[(i32, i32); 4]; 4],
}

#[derive(Clone, Copy)]
struct ActivePiece {
    def: PieceDef,
    rotation: usize,
    x: i32,
    y: i32,
}

struct TetrisApp {
    board: [[Option<Color32>; BOARD_W]; BOARD_H],
    bag: Vec<PieceDef>,
    current: ActivePiece,
    next: PieceDef,
    score: u32,
    lines: u32,
    level: u32,
    game_over: bool,
    last_drop_time: f64,
}

impl Default for TetrisApp {
    fn default() -> Self {
        reseed_rng();
        let mut app = Self {
            board: [[None; BOARD_W]; BOARD_H],
            bag: Vec::new(),
            current: spawn_piece(piece_i()),
            next: piece_o(),
            score: 0,
            lines: 0,
            level: 1,
            game_over: false,
            last_drop_time: 0.0,
        };
        let first = app.draw_piece();
        let second = app.draw_piece();
        app.current = spawn_piece(first);
        app.next = second;
        app
    }
}

impl App for TetrisApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(Color32::from_rgb(226, 232, 240));
        visuals.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(226, 232, 240);
        visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(226, 232, 240);
        visuals.widgets.active.fg_stroke.color = Color32::from_rgb(241, 245, 249);
        visuals.widgets.hovered.fg_stroke.color = Color32::from_rgb(241, 245, 249);
        visuals.panel_fill = Color32::from_rgb(8, 12, 22);
        ctx.set_visuals(visuals);

        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        let now = ctx.input(|i| i.time);
        let drop_interval = (0.7_f64 - ((self.level.saturating_sub(1)) as f64 * 0.05)).max(0.08);

        self.handle_input(ctx);

        if !self.game_over && now - self.last_drop_time >= drop_interval {
            self.last_drop_time = now;
            self.step_down();
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(8, 12, 22))
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.heading("Tiny Tetris");
                        ui.label("Left/Right: move");
                        ui.label("Up or X: rotate");
                        ui.label("Down: soft drop");
                        ui.label("Space: hard drop");
                        ui.label("R: reset");
                    });
                    ui.add_space(18.0);
                    ui.vertical(|ui| {
                        stat(ui, "Score", self.score.to_string());
                        stat(ui, "Lines", self.lines.to_string());
                        stat(ui, "Level", self.level.to_string());
                    });
                });

                ui.add_space(12.0);
                ui.horizontal_top(|ui| {
                    draw_board(ui, &self.board, Some(self.current));
                    ui.add_space(18.0);
                    ui.vertical(|ui| {
                        ui.heading("Next");
                        draw_preview(ui, self.next);
                        ui.add_space(12.0);
                        if self.game_over {
                            ui.colored_label(Color32::from_rgb(248, 113, 113), "Game over");
                            if ui.button("Play again").clicked() {
                                *self = Self::default();
                            }
                        } else {
                            ui.colored_label(Color32::from_rgb(96, 165, 250), "Still better than doomscrolling.");
                        }
                    });
                });
            });
    }
}

impl TetrisApp {
    fn handle_input(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(Key::R)) {
            *self = Self::default();
            return;
        }
        if self.game_over {
            return;
        }

        if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
            self.try_move(-1, 0);
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
            self.try_move(1, 0);
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
            self.step_down();
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::X)) {
            self.try_rotate(1);
        }
        if ctx.input(|i| i.key_pressed(Key::Z)) {
            self.try_rotate(3);
        }
        if ctx.input(|i| i.key_pressed(Key::Space)) {
            while self.try_move(0, 1) {}
            self.lock_piece();
        }
    }

    fn try_move(&mut self, dx: i32, dy: i32) -> bool {
        let mut next = self.current;
        next.x += dx;
        next.y += dy;
        if self.fits(next) {
            self.current = next;
            true
        } else {
            false
        }
    }

    fn try_rotate(&mut self, delta: usize) {
        let mut next = self.current;
        next.rotation = (next.rotation + delta) % 4;
        for kick in [0, -1, 1, -2, 2] {
            let mut kicked = next;
            kicked.x += kick;
            if self.fits(kicked) {
                self.current = kicked;
                return;
            }
        }
    }

    fn step_down(&mut self) {
        if !self.try_move(0, 1) {
            self.lock_piece();
        }
    }

    fn lock_piece(&mut self) {
        for (x, y) in piece_cells(self.current) {
            if (0..BOARD_H_I32).contains(&y) && (0..BOARD_W_I32).contains(&x) {
                self.board[y as usize][x as usize] = Some(self.current.def.color);
            }
        }

        let cleared = self.clear_lines();
        self.lines += cleared;
        self.score += match cleared {
            1 => 100,
            2 => 300,
            3 => 500,
            4 => 800,
            _ => 0,
        } * self.level;
        self.level = 1 + self.lines / 10;

        self.current = spawn_piece(self.next);
        self.next = self.draw_piece();
        if !self.fits(self.current) {
            self.game_over = true;
        }
    }

    fn clear_lines(&mut self) -> u32 {
        let mut kept = Vec::with_capacity(BOARD_H);
        let mut cleared = 0_u32;
        for row in self.board {
            if row.iter().all(Option::is_some) {
                cleared += 1;
            } else {
                kept.push(row);
            }
        }
        while kept.len() < BOARD_H {
            kept.insert(0, [None; BOARD_W]);
        }
        self.board.copy_from_slice(&kept[..BOARD_H]);
        cleared
    }

    fn fits(&self, piece: ActivePiece) -> bool {
        piece_cells(piece).into_iter().all(|(x, y)| {
            (0..BOARD_W_I32).contains(&x)
                && y < BOARD_H_I32
                && (y < 0 || self.board[y as usize][x as usize].is_none())
        })
    }

    fn draw_piece(&mut self) -> PieceDef {
        if self.bag.is_empty() {
            self.bag = all_pieces().to_vec();
            fastrand::shuffle(&mut self.bag);
        }
        self.bag.pop().expect("tetris piece bag is never empty after refill")
    }
}

fn stat(ui: &mut egui::Ui, label: &str, value: String) {
    ui.group(|ui| {
        ui.label(egui::RichText::new(label).color(Color32::from_gray(160)));
        ui.label(egui::RichText::new(value).strong().size(18.0));
    });
}

fn draw_board(
    ui: &mut egui::Ui,
    board: &[[Option<Color32>; BOARD_W]; BOARD_H],
    active: Option<ActivePiece>,
) {
    let board_size = Vec2::new(BOARD_W as f32 * CELL, BOARD_H as f32 * CELL);
    let (response, painter) = ui.allocate_painter(board_size, egui::Sense::hover());
    let origin = response.rect.min;

    painter.rect_filled(response.rect, 8.0, Color32::from_rgb(15, 23, 42));
    painter.rect_stroke(
        response.rect,
        8.0,
        Stroke::new(1.0, Color32::from_gray(70)),
        egui::StrokeKind::Outside,
    );

    for (y, row) in board.iter().enumerate() {
        for (x, color) in row.iter().enumerate() {
            let rect = cell_rect(origin, x as i32, y as i32);
            painter.rect_stroke(
                rect,
                2.0,
                Stroke::new(1.0, Color32::from_gray(28)),
                egui::StrokeKind::Outside,
            );
            if let Some(color) = color {
                fill_cell(&painter, rect, *color);
            }
        }
    }

    if let Some(active) = active {
        for (x, y) in piece_cells(active) {
            if y >= 0 {
                fill_cell(&painter, cell_rect(origin, x, y), active.def.color);
            }
        }
    }
}

fn draw_preview(ui: &mut egui::Ui, piece: PieceDef) {
    let size = Vec2::new(6.0 * CELL, 6.0 * CELL);
    let (response, painter) = ui.allocate_painter(size, egui::Sense::hover());
    painter.rect_filled(response.rect, 8.0, Color32::from_rgb(15, 23, 42));
    let preview_origin = response.rect.min + Vec2::new(CELL, CELL);
    for (x, y) in piece.rotations[0] {
        let rect = Rect::from_min_size(
            Pos2::new(
                preview_origin.x + x as f32 * CELL,
                preview_origin.y + y as f32 * CELL,
            ),
            Vec2::splat(CELL),
        );
        fill_cell(&painter, rect, piece.color);
    }
}

fn cell_rect(origin: Pos2, x: i32, y: i32) -> Rect {
    Rect::from_min_size(
        Pos2::new(origin.x + x as f32 * CELL, origin.y + y as f32 * CELL),
        Vec2::splat(CELL),
    )
}

fn fill_cell(painter: &egui::Painter, rect: Rect, color: Color32) {
    painter.rect_filled(rect.shrink(1.5), 4.0, color);
    painter.rect_stroke(
        rect.shrink(1.5),
        4.0,
        Stroke::new(1.0, color.gamma_multiply(1.4)),
        egui::StrokeKind::Outside,
    );
}

fn spawn_piece(def: PieceDef) -> ActivePiece {
    ActivePiece {
        def,
        rotation: 0,
        x: 3,
        y: -1,
    }
}

fn piece_cells(piece: ActivePiece) -> [(i32, i32); 4] {
    let cells = piece.def.rotations[piece.rotation];
    cells.map(|(dx, dy)| (piece.x + dx, piece.y + dy))
}

fn all_pieces() -> [PieceDef; 7] {
    [
        piece_i(),
        piece_o(),
        piece_t(),
        piece_s(),
        piece_z(),
        piece_j(),
        piece_l(),
    ]
}

fn reseed_rng() {
    #[cfg(target_arch = "wasm32")]
    {
        let seed = (js_sys::Math::random() * (u64::MAX as f64)) as u64;
        fastrand::seed(seed);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x7eed_u64);
        fastrand::seed(seed);
    }
}

fn piece_i() -> PieceDef {
    PieceDef {
        color: Color32::from_rgb(34, 211, 238),
        rotations: [
            [(0, 1), (1, 1), (2, 1), (3, 1)],
            [(2, 0), (2, 1), (2, 2), (2, 3)],
            [(0, 2), (1, 2), (2, 2), (3, 2)],
            [(1, 0), (1, 1), (1, 2), (1, 3)],
        ],
    }
}

fn piece_o() -> PieceDef {
    PieceDef {
        color: Color32::from_rgb(250, 204, 21),
        rotations: [[(1, 0), (2, 0), (1, 1), (2, 1)]; 4],
    }
}

fn piece_t() -> PieceDef {
    PieceDef {
        color: Color32::from_rgb(168, 85, 247),
        rotations: [
            [(1, 0), (0, 1), (1, 1), (2, 1)],
            [(1, 0), (1, 1), (2, 1), (1, 2)],
            [(0, 1), (1, 1), (2, 1), (1, 2)],
            [(1, 0), (0, 1), (1, 1), (1, 2)],
        ],
    }
}

fn piece_s() -> PieceDef {
    PieceDef {
        color: Color32::from_rgb(74, 222, 128),
        rotations: [
            [(1, 0), (2, 0), (0, 1), (1, 1)],
            [(1, 0), (1, 1), (2, 1), (2, 2)],
            [(1, 1), (2, 1), (0, 2), (1, 2)],
            [(0, 0), (0, 1), (1, 1), (1, 2)],
        ],
    }
}

fn piece_z() -> PieceDef {
    PieceDef {
        color: Color32::from_rgb(248, 113, 113),
        rotations: [
            [(0, 0), (1, 0), (1, 1), (2, 1)],
            [(2, 0), (1, 1), (2, 1), (1, 2)],
            [(0, 1), (1, 1), (1, 2), (2, 2)],
            [(1, 0), (0, 1), (1, 1), (0, 2)],
        ],
    }
}

fn piece_j() -> PieceDef {
    PieceDef {
        color: Color32::from_rgb(96, 165, 250),
        rotations: [
            [(0, 0), (0, 1), (1, 1), (2, 1)],
            [(1, 0), (2, 0), (1, 1), (1, 2)],
            [(0, 1), (1, 1), (2, 1), (2, 2)],
            [(1, 0), (1, 1), (0, 2), (1, 2)],
        ],
    }
}

fn piece_l() -> PieceDef {
    PieceDef {
        color: Color32::from_rgb(251, 146, 60),
        rotations: [
            [(2, 0), (0, 1), (1, 1), (2, 1)],
            [(1, 0), (1, 1), (1, 2), (2, 2)],
            [(0, 1), (1, 1), (2, 1), (0, 2)],
            [(0, 0), (1, 0), (1, 1), (1, 2)],
        ],
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("missing window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("missing document"))?;
    let canvas = document
        .get_element_by_id("the_canvas")
        .ok_or_else(|| JsValue::from_str("missing canvas"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let web_options = WebOptions::default();
    eframe::WebRunner::new()
        .start(
            canvas,
            web_options,
            Box::new(|_cc| Ok(Box::new(TetrisApp::default()))),
        )
        .await
}

fn main() {}
