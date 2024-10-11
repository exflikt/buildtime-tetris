use macroquad::color::{colors, Color};
use macroquad::input::{self, KeyCode};
use macroquad::shapes;
use macroquad::text;
use macroquad::window;
use rand::distributions;
use std::io::Write;
use std::path::Path;

fn main() {
    let score = run_tetris();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("score.rs");
    let text = format!(r#"const SCORE: u32 = {score};"#);
    std::fs::File::create(dest_path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/");
}

fn keys_registered<const N: usize>(key_codes: [KeyCode; N]) -> bool {
    use std::sync::RwLock;
    static FREEZE_DURATION: RwLock<u8> = RwLock::new(0);

    let duration = *FREEZE_DURATION.read().unwrap();
    if duration == 0 && key_codes.iter().any(|&k| input::is_key_down(k)) {
        *FREEZE_DURATION.write().unwrap() = 60;
        return true;
    } else if duration > 0 {
        *FREEZE_DURATION.write().unwrap() -= 1;
    }
    false
}

const GRID_CELL_SIZE: f32 = 32.;
const MARGIN: f32 = 20.;
const PIECE_PREVIEW_WIDTH: f32 = GRID_CELL_SIZE * 5.0;
const SCREEN_WIDTH: f32 =
    Grid::WIDTH as f32 * GRID_CELL_SIZE + MARGIN * 2.0 + PIECE_PREVIEW_WIDTH + MARGIN;
const SCREEN_HEIGHT: f32 = MARGIN + Grid::HEIGHT as f32 * GRID_CELL_SIZE + MARGIN;

const BORDER_COLOR: Color = colors::BLACK;
const BACKGROUND_COLOR: Color = Color::new(0.125, 0.1484375, 0.2265625, 1.);

struct Game {
    pub state: State,
    grid: Grid,
    pos: (u8, u8),
    tetromino: Tetromino,
    rot: Rotation,
    holding_tetromino: Option<Tetromino>,
    swapped: bool,
    next_tetromino: Tetromino,
    level: Level,
    tick: u32,
    score: u32,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum State {
    #[default]
    Start,
    Play,
    Pause,
    Over,
    WindowClose,
}

struct Grid {
    cells: [Option<Tetromino>; Grid::WIDTH as usize * Grid::HEIGHT as usize],
}

impl Grid {
    const WIDTH: u8 = 10;
    const HEIGHT: u8 = 22;

    const fn new() -> Grid {
        Grid {
            cells: [None; Grid::WIDTH as usize * Grid::HEIGHT as usize],
        }
    }

    /// Remove filled rows and move other rows downward.
    /// Returns the score according to the number of rows deleted.
    fn squash_filled_rows(&mut self) -> u32 {
        let mut src_range_indices: Vec<u8> = Vec::new();
        let mut min_y = Grid::HEIGHT;
        for y in (0..Grid::HEIGHT).rev() {
            let mut min_y_updated = false;
            let mut filled = true;
            for x in 0..Grid::WIDTH {
                if self.at(x, y).is_some() {
                    if !min_y_updated {
                        min_y = y;
                        min_y_updated = true;
                    } else if !filled {
                        break;
                    }
                } else if filled {
                    filled = false;
                }
            }
            if filled {
                src_range_indices.push(y);
            }
        }
        let no_filled_rows = src_range_indices.len();
        if min_y != Grid::HEIGHT {
            src_range_indices.push(min_y.saturating_sub(1));
        }

        for (nth_del, rows) in src_range_indices.windows(2).enumerate() {
            let y_dst_base = rows[0] + nth_del as u8;
            let y_src_range = (rows[1] + 1)..rows[0];
            for (y_src_i, y_src) in y_src_range.rev().enumerate() {
                let y_dst = y_dst_base - y_src_i as u8;
                for x in 0..Grid::WIDTH {
                    *self.at_mut(x, y_dst) = self.at(x, y_src).clone();
                }
            }
        }
        if min_y != Grid::HEIGHT {
            for y_dst in min_y..min_y + no_filled_rows as u8 {
                for x in 0..Grid::WIDTH {
                    *self.at_mut(x, y_dst) = None;
                }
            }
        }

        Grid::_to_score(no_filled_rows)
    }

    const fn _to_score(no_squashed_rows: usize) -> u32 {
        assert!(no_squashed_rows <= 4);
        match no_squashed_rows {
            0 => 0,
            1 => 5,
            2 => 15,
            3 => 30,
            4 => 50,
            // SAFETY: asserted that `no_squahsed_row` is less than or equal to 4
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    fn at(&self, x: u8, y: u8) -> &Option<Tetromino> {
        assert!(x < Self::WIDTH);
        assert!(y < Self::HEIGHT);
        let idx = (y * Grid::WIDTH + x) as usize;
        // SAFETY: asserts ensure that the idx is be the range of [0, WIDTH * HEIGHT - 1].
        unsafe { self.cells.get_unchecked(idx) }
    }

    fn at_mut(&mut self, x: u8, y: u8) -> &mut Option<Tetromino> {
        assert!(x < Self::WIDTH);
        assert!(y < Self::HEIGHT);
        let idx = (y * Grid::WIDTH + x) as usize;
        // SAFETY: asserts ensure that the idx is be the range of [0, WIDTH * HEIGHT - 1].
        unsafe { self.cells.get_unchecked_mut(idx) }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Tetromino {
    I,
    O,
    T,
    J,
    L,
    S,
    Z,
}

impl Tetromino {
    const fn fill_color(self) -> Color {
        match self {
            Tetromino::I => Color::new(0., 1.0, 1., 1.),
            Tetromino::O => Color::new(1., 1.0, 0., 1.),
            Tetromino::T => Color::new(1., 0.0, 1., 1.),
            Tetromino::J => Color::new(0., 0.0, 1., 1.),
            Tetromino::L => Color::new(1., 0.5, 0., 1.),
            Tetromino::S => Color::new(0., 1.0, 0., 1.),
            Tetromino::Z => Color::new(1., 0.0, 0., 1.),
        }
    }

    const fn ghost_color(self) -> Color {
        let mut color = self.fill_color();
        color.a = 0.3;
        color
    }

    const fn neighbors(self, rot: Rotation) -> [(i8, i8); 4] {
        use Rotation::{DEG0, DEG180, DEG270, DEG90};
        match (self, rot) {
            (Tetromino::I, DEG0 | DEG180) => [(-1, 0), (0, 0), (1, 0), (2, 0)],
            (Tetromino::I, DEG90 | DEG270) => [(0, -1), (0, 0), (0, 1), (0, 2)],
            (Tetromino::O, _) => [(0, 0), (1, 0), (0, 1), (1, 1)],
            (Tetromino::T, DEG0) => [(0, -1), (-1, 0), (0, 0), (1, 0)],
            (Tetromino::T, DEG90) => [(0, -1), (0, 0), (1, 0), (0, 1)],
            (Tetromino::T, DEG180) => [(-1, 0), (0, 0), (1, 0), (0, 1)],
            (Tetromino::T, DEG270) => [(0, -1), (-1, 0), (0, 0), (0, 1)],
            (Tetromino::J, DEG0) => [(0, -1), (0, 0), (-1, 1), (0, 1)],
            (Tetromino::J, DEG90) => [(-1, -1), (-1, 0), (0, 0), (1, 0)],
            (Tetromino::J, DEG180) => [(0, -1), (1, -1), (0, 0), (0, 1)],
            (Tetromino::J, DEG270) => [(-1, 0), (0, 0), (1, 0), (1, 1)],
            (Tetromino::L, DEG0) => [(0, -1), (0, 0), (0, 1), (1, 1)],
            (Tetromino::L, DEG90) => [(-1, 0), (0, 0), (1, 0), (-1, 1)],
            (Tetromino::L, DEG180) => [(-1, -1), (0, -1), (0, 0), (0, 1)],
            (Tetromino::L, DEG270) => [(1, -1), (-1, 0), (0, 0), (1, 0)],
            (Tetromino::S, DEG0 | DEG180) => [(0, 0), (1, 0), (-1, 1), (0, 1)],
            (Tetromino::S, DEG90 | DEG270) => [(0, -1), (0, 0), (1, 0), (1, 1)],
            (Tetromino::Z, DEG0 | DEG180) => [(-1, 0), (0, 0), (0, 1), (1, 1)],
            (Tetromino::Z, DEG90 | DEG270) => [(0, -1), (-1, 0), (0, 0), (-1, 1)],
        }
    }
}

impl distributions::Distribution<Tetromino> for distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> Tetromino {
        let variant: u8 = rng.gen_range(0..=Tetromino::Z as u8);
        // SAFETY: the line above restricts the range of the random number generator to the number of variants in `Tetromino` enum.
        unsafe { std::mem::transmute(variant) }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Rotation {
    #[default]
    DEG0,
    DEG90,
    DEG180,
    DEG270,
}

impl Rotation {
    const fn spin_cw(self) -> Rotation {
        match self {
            Rotation::DEG0 => Rotation::DEG90,
            Rotation::DEG90 => Rotation::DEG180,
            Rotation::DEG180 => Rotation::DEG270,
            Rotation::DEG270 => Rotation::DEG0,
        }
    }

    const fn spin_acw(self) -> Rotation {
        match self {
            Rotation::DEG0 => Rotation::DEG270,
            Rotation::DEG90 => Rotation::DEG0,
            Rotation::DEG180 => Rotation::DEG90,
            Rotation::DEG270 => Rotation::DEG180,
        }
    }
}

struct Level {
    tick_rate: u32,
    piece_count: u32,
}

impl Level {
    const fn new() -> Level {
        Level {
            tick_rate: 30,
            piece_count: 1,
        }
    }

    fn update(&mut self) {
        self.piece_count += 1;

        let prev_rate = self.tick_rate;

        self.tick_rate = match self.piece_count {
            0..=25 => 30,
            26..=50 => 25,
            51..=100 => 20,
            101..=200 => 15,
            201..=300 => 12,
            301..=500 => 10,
            501..=700 => 8,
            701..=900 => 6,
            _ => 5,
        };

        if self.tick_rate != prev_rate {
            eprintln!("Tick rate: {}", self.tick_rate)
        }
    }
}

impl Game {
    fn new() -> Self {
        let tetromino = rand::random();
        Game {
            state: State::Start,
            grid: Grid::new(),
            pos: (Grid::WIDTH / 2, 1),
            tetromino,
            rot: Default::default(),
            holding_tetromino: None,
            swapped: false,
            next_tetromino: rand::random(),
            level: Level::new(),
            tick: 0,
            score: 0,
        }
    }

    fn _movable_with(&self, rot: Rotation, x_dir: i8, y_dir: i8) -> bool {
        let (x_from, y_from) = self.pos;
        let neighbors = self.tetromino.neighbors(rot);
        for (dx, dy) in neighbors {
            let x = x_from.checked_add_signed(dx + x_dir);
            let y = y_from.checked_add_signed(dy + y_dir);
            if let [Some(x), Some(y)] = [x, y] {
                if x >= Grid::WIDTH || y >= Grid::HEIGHT || self.grid.at(x, y).is_some() {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    fn update(&mut self) {
        match self.state {
            State::Start => {
                if input::is_key_pressed(KeyCode::Enter) {
                    self.state = State::Play;
                } else if input::is_key_pressed(KeyCode::Q) {
                    self.state = State::WindowClose;
                }
            }
            State::Play => {
                if input::is_key_pressed(KeyCode::Escape) {
                    self.state = State::Pause;
                    return;
                }

                if keys_registered([KeyCode::Left]) && self._movable_with(self.rot, -1, 0) {
                    self.pos.0 -= 1;
                } else if keys_registered([KeyCode::Right]) && self._movable_with(self.rot, 1, 0) {
                    self.pos.0 += 1;
                } else if keys_registered([KeyCode::Down]) {
                    // soft drop the tetromino
                    if self._movable_with(self.rot, 0, 1) {
                        self.pos.1 += 1;
                    } else {
                        place_tetromino_then_update(self);
                        return;
                    }
                } else if keys_registered([KeyCode::Space]) {
                    // hard drop the tetromino
                    while self._movable_with(self.rot, 0, 1) {
                        self.pos.1 += 1;
                    }
                    place_tetromino_then_update(self);
                    return;
                } else if keys_registered([KeyCode::Up, KeyCode::X]) {
                    let new_rot = self.rot.spin_cw();
                    for x_offset in [0, -1, 1, -2i8, 2] {
                        if self._movable_with(new_rot, x_offset, 0) {
                            self.pos.0 = self.pos.0.saturating_add_signed(x_offset);
                            self.rot = new_rot;
                            return;
                        }
                    }
                } else if keys_registered([KeyCode::LeftControl, KeyCode::RightControl, KeyCode::Z])
                {
                    let new_rot = self.rot.spin_acw();
                    for x_offset in [0, -1, 1, -2i8, 2] {
                        if self._movable_with(new_rot, x_offset, 0) {
                            self.pos.0 = self.pos.0.saturating_add_signed(x_offset);
                            self.rot = new_rot;
                            return;
                        }
                    }
                } else if keys_registered([KeyCode::LeftShift, KeyCode::RightShift, KeyCode::C])
                    && !self.swapped
                {
                    if let Some(hold) = self.holding_tetromino {
                        self.holding_tetromino = Some(self.tetromino);
                        self.tetromino = hold;
                        self.pos = (Grid::WIDTH / 2, 1);
                        self.rot = Default::default();
                    } else {
                        self.holding_tetromino = Some(self.tetromino);
                        reset_piece(self);
                        self.pos = (Grid::WIDTH / 2, 1);
                    }
                    self.swapped = true;
                    return;
                }

                if self.tick >= self.level.tick_rate {
                    if self._movable_with(self.rot, 0, 1) {
                        self.pos.1 += 1;
                        self.tick = 0;
                    } else {
                        place_tetromino_then_update(self);
                    }
                } else {
                    self.tick += 1;
                }
                fn reset_piece(game: &mut Game) {
                    game.tetromino = game.next_tetromino;
                    game.next_tetromino = rand::random();
                    game.rot = Default::default();
                }
                fn place_tetromino_then_update(game: &mut Game) {
                    let neighbors = game.tetromino.neighbors(game.rot);
                    let (x, y) = game.pos;
                    for (dx, dy) in neighbors {
                        let (x, overflowed) = x.overflowing_add_signed(dx);
                        assert!(overflowed == false);
                        let (y, overflowed) = y.overflowing_add_signed(dy);
                        assert!(overflowed == false);
                        *game.grid.at_mut(x, y) = Some(game.tetromino);
                    }
                    game.score += game.grid.squash_filled_rows();
                    game.pos = (Grid::WIDTH / 2, 1);
                    if !game._movable_with(game.rot, 0, 0) {
                        game.state = State::Over;
                        return;
                    }
                    reset_piece(game);
                    game.swapped = false;
                    game.level.update();
                    game.tick = 0;
                }
            }
            State::Pause => {
                if input::is_key_released(KeyCode::Enter) {
                    self.state = State::Play;
                } else if input::is_key_pressed(KeyCode::Q) {
                    self.state = State::WindowClose;
                }
            }
            State::Over => {
                if input::is_key_pressed(KeyCode::Enter) {
                    *self = Game::new();
                    self.state = State::Play;
                } else if input::is_key_pressed(KeyCode::Q) {
                    self.state = State::WindowClose;
                }
            }
            State::WindowClose => {
                panic!("`update` method should not be called when the state is in WindowClose");
            }
        }
    }

    fn draw(&mut self) {
        window::clear_background(BORDER_COLOR);

        draw_grid(&self.grid);

        let neighbors = self.tetromino.neighbors(self.rot);
        let mut ghost_offset = 0;
        while self._movable_with(self.rot, 0, ghost_offset + 1) {
            ghost_offset += 1;
        }
        draw_tetromino(self.pos, self.tetromino, neighbors, ghost_offset);

        if self.state != State::Play {
            draw_overlay(self.state);
        }

        let x_right_bar: f32 = MARGIN + (f32::from(Grid::WIDTH) * GRID_CELL_SIZE) + MARGIN;
        let y_score = MARGIN + GRID_CELL_SIZE;
        let y_hold = draw_score(self.score, (x_right_bar, y_score));
        let y_next = draw_tetromino_box(self.holding_tetromino, (x_right_bar, y_hold));
        let y_next = y_next + GRID_CELL_SIZE * f32::from(Grid::HEIGHT / 4);
        let _ = draw_tetromino_box(Some(self.next_tetromino), (x_right_bar, y_next));

        fn draw_grid(grid: &Grid) {
            let [x_base, y_base] = [MARGIN; 2];
            let [w, h] = [
                Grid::WIDTH as f32 * GRID_CELL_SIZE + MARGIN,
                Grid::HEIGHT as f32 * GRID_CELL_SIZE + MARGIN,
            ];
            let [x, y] = [x_base - MARGIN / 2., y_base - MARGIN / 2.];
            shapes::draw_rectangle_lines(x, y, w, h, MARGIN, BACKGROUND_COLOR);
            for y in 0..Grid::HEIGHT {
                for x in 0..Grid::WIDTH {
                    let cell = grid.at(x, y);
                    let [w, h] = [GRID_CELL_SIZE; 2];
                    let [x, y] = [x_base + w * x as f32, y_base + h * y as f32];
                    let color = cell.map(Tetromino::fill_color).unwrap_or(BORDER_COLOR);
                    shapes::draw_rectangle(x, y, w, h, color);
                }
            }
        }

        fn draw_tetromino(
            (x, y): (u8, u8),
            tetromino: Tetromino,
            neighbors: [(i8, i8); 4],
            ghost_offset: i8,
        ) {
            let [x_base, y_base] = [MARGIN; 2];
            for (dx, dy) in neighbors {
                let [w, h] = [GRID_CELL_SIZE; 2];
                let x = x_base + w * x.saturating_add_signed(dx) as f32;
                let y_orig = y_base + h * y.saturating_add_signed(dy) as f32;
                shapes::draw_rectangle(x, y_orig, w, h, tetromino.fill_color());
                if ghost_offset != 0 {
                    let y_ghost = y_base + h * y.saturating_add_signed(dy + ghost_offset) as f32;
                    shapes::draw_rectangle(x, y_ghost, w, h, tetromino.ghost_color());
                }
            }
        }

        fn draw_overlay(state: State) {
            let [x, y] = [MARGIN; 2];
            let (w, h) = (
                GRID_CELL_SIZE * f32::from(Grid::WIDTH),
                GRID_CELL_SIZE * f32::from(Grid::HEIGHT),
            );
            shapes::draw_rectangle(x, y, w, h, Color::new(0., 0., 0., 0.75));

            let [base_x, base_y] = [GRID_CELL_SIZE * 2.5, SCREEN_HEIGHT / 2.];
            const SIZE_TITLE: f32 = 50.;
            const SIZE_DESC: f32 = 20.;
            const COLOR_TITLE: Color = colors::WHITE;
            const COLOR_DESC: Color = colors::LIGHTGRAY;
            if state == State::Pause {
                let [x, y] = [base_x + GRID_CELL_SIZE, base_y - 50.];
                text::draw_text("PAUSED", x, y, SIZE_TITLE, COLOR_TITLE);
                let msg = "Press Enter to unpause";
                text::draw_text(msg, base_x, base_y, SIZE_DESC, COLOR_DESC);
            } else if state == State::Over {
                let [x, y] = [base_x, base_y - 50.];
                text::draw_text("GAME OVER", x, y, SIZE_TITLE, COLOR_TITLE);
                let msg = "Press ENTER to restart";
                text::draw_text(msg, base_x, base_y, SIZE_DESC, COLOR_DESC);
            } else if state == State::Start {
                let [x, y] = [base_x + GRID_CELL_SIZE, base_y - 50.];
                text::draw_text("TETRIS", x, y, SIZE_TITLE, COLOR_TITLE);
                let msg = "Press ENTER to start";
                let [x, y] = [base_x + GRID_CELL_SIZE / 2., base_y];
                text::draw_text(msg, x, y, SIZE_DESC, COLOR_DESC);
                let [x, y] = [base_x + GRID_CELL_SIZE, base_y + 50.];
                text::draw_text("Press Q to quit", x, y, SIZE_DESC, COLOR_DESC);
            }
        }

        fn draw_score(score: u32, (x_base, y_base): (f32, f32)) -> f32 {
            text::draw_text("Score:", x_base, y_base, 20., colors::LIGHTGRAY);
            let [x, y] = [x_base, y_base + MARGIN];
            text::draw_text(&score.to_string(), x, y, 20., colors::LIGHTGRAY);
            y_base + MARGIN * 2.
        }

        fn draw_tetromino_box(tetromino: Option<Tetromino>, (x_base, y_base): (f32, f32)) -> f32 {
            const BOX_MARGIN: f32 = GRID_CELL_SIZE + MARGIN;
            let [w, h] = [
                GRID_CELL_SIZE * 2. + BOX_MARGIN * 2.,
                GRID_CELL_SIZE * 1. + BOX_MARGIN * 2.,
            ];
            shapes::draw_rectangle(x_base, y_base, w, h, BACKGROUND_COLOR);

            if let Some(tetromino) = tetromino {
                let [x_base, y_base] = [x_base + BOX_MARGIN, y_base + BOX_MARGIN];
                for (dx, dy) in tetromino.neighbors(Default::default()) {
                    let [w, h] = [GRID_CELL_SIZE; 2];
                    let [x, y] = [x_base + w * dx as f32, y_base + h * dy as f32];
                    let (x, y) = match tetromino {
                        Tetromino::O => (x, y - h / 2.),
                        Tetromino::T => (x + w / 2., y + h / 2.),
                        Tetromino::J => (x + w, y),
                        Tetromino::S => (x + w / 2., y - h / 2.),
                        Tetromino::Z => (x + w / 2., y - h / 2.),
                        _ => (x, y),
                    };
                    shapes::draw_rectangle(x, y, w, h, tetromino.fill_color());
                }
            }
            y_base + h + MARGIN
        }
    }
}

fn run_tetris() -> u32 {
    use std::sync::OnceLock;
    static SCORE_CELL: OnceLock<u32> = OnceLock::new();

    macroquad::Window::new("buildtime_tetris", async {
        let mut game = Game::new();
        window::request_new_screen_size(SCREEN_WIDTH, SCREEN_HEIGHT);
        while game.state != State::WindowClose {
            game.update();
            game.draw();
            window::next_frame().await
        }

        SCORE_CELL.set(game.score).unwrap();
    });

    SCORE_CELL.get().copied().unwrap_or(0)
}
