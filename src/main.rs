use rand::Rng;
use std::cmp;
use tcod::colors::*;
use tcod::console::*;
use tcod::input::{self, Event, Key, Mouse};
use tcod::map::{FovAlgorithm, Map as FovMap};

// define screen size
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

// define the map size & color for tile
const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;
const COLOR_DARK_WALL: Color = Color {
    r: 40,
    g: 40,
    b: 40,
};
const COLOR_DARK_GROUND: Color = Color {
    r: 60,
    g: 60,
    b: 60,
};

// dungeon params
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;

// set limit for FPS & color for the lit tiles
const LIMIT_FPS: i32 = 40;
const COLOR_LIGHT_WALL: Color = Color {
    r: 164,
    g: 168,
    b: 132,
};
const COLOR_LIGHT_GROUND: Color = Color {
    r: 202,
    g: 206,
    b: 162,
};

// define the FOV algorithm
const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 10;

// define max monsters per room
const MAX_ROOM_MONSTERS: i32 = 3;
const PLAYER: usize = 0;

// GUI
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;

// message log
const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;

// struct for encapsulation of all libtcod values
struct Tcod {
    root: Root,
    con: Offscreen,
    panel: Offscreen,
    fov: FovMap,
    key: Key,
    mouse: Mouse,
}

// define custom type for map
type Map = Vec<Vec<Tile>>;

// struct for encapsulation of all game values
struct Game {
    map: Map,
    messages: Messages,
}

// hold the properties of a given tile on the map
#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    explored: bool,
    block_sight: bool,
}

impl Tile {
    pub fn empty() -> Self {
        Tile {
            blocked: false,
            explored: false,
            block_sight: false,
        }
    }

    pub fn wall() -> Self {
        Tile {
            blocked: true,
            explored: false,
            block_sight: true,
        }
    }
}

// room shape on the map
#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect {
            x1: x,
            y1: y,
            x2: x + w,
            y2: y + h,
        }
    }

    // find the centre of the rect and return a tuple of coords
    pub fn centre(&self) -> (i32, i32) {
        let centre_x = (self.x1 + self.x2) / 2;
        let centre_y = (self.y1 + self.y2) / 2;
        (centre_x, centre_y)
    }

    // return true if intersect
    pub fn intersects_with(&self, other: &Rect) -> bool {
        (self.x1 <= other.x2)
            && (self.x2 >= other.x1)
            && (self.y1 <= other.y2)
            && (self.y2 >= other.y1)
    }
}

// holds any"thing" that can be represented by a letter on console
#[derive(Debug)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: Color,
    name: String,
    blocks: bool,
    alive: bool,
    fighter: Option<Fighter>,
    ai: Option<Ai>,
}

impl Object {
    pub fn new(x: i32, y: i32, char: char, color: Color, name: &str, blocks: bool) -> Self {
        Object {
            x: x,
            y: y,
            char: char,
            color: color,
            name: name.into(),
            blocks: blocks,
            alive: false,
            fighter: None,
            ai: None,
        }
    }

    // draw to console
    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None)
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    // take damage
    pub fn take_damage(&mut self, damage: i32, game: &mut Game) {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }

        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self, game);
            }
        }
    }

    // attack the appropriate amount and take damage off target
    pub fn attack(&mut self, target: &mut Object, game: &mut Game) {
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defense);
        if damage > 0 {
            game.messages.add(
                format!(
                    "{} attacks {} for {} hit points.",
                    self.name, target.name, damage
                ),
                WHITE,
            );
            target.take_damage(damage, game);
        } else {
            game.messages.add(
                format!(
                    "{} attacks {} but it has no effect!",
                    self.name, target.name
                ),
                WHITE,
            );
        }
    }
}

// combat properties and methods
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defense: i32,
    power: i32,
    on_death: DeathCallback,
}

struct Messages {
    messages: Vec<(String, Color)>,
}

impl Messages {
    pub fn new() -> Self {
        Self { messages: vec![] }
    }

    pub fn add<T: Into<String>>(&mut self, message: T, color: Color) {
        self.messages.push((message.into(), color));
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &(String, Color)> {
        self.messages.iter()
    }
}

// carve out a non-blocked section of the map
fn create_room(room: Rect, map: &mut Map) {
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
}

// carve vertical tunnel
fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

// carve horizontal tunnel
fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

// fn for creating a new blank map vector
fn make_map(objects: &mut Vec<Object>) -> Map {
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    let mut rooms = vec![];

    for _ in 0..MAX_ROOMS {
        // random width and height
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        // random position within map
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);

        let new_room = Rect::new(x, y, w, h);

        // check for intersection of new room with each other room in the list
        let intersects = rooms
            .iter()
            .any(|other_room| new_room.intersects_with(other_room));

        if !intersects {
            // Create room if no intersect
            create_room(new_room, &mut map);
            place_objects(new_room, &map, objects);
            // store the centre coords of new room
            let (new_x, new_y) = new_room.centre();
            // place the player in the centre of the first room
            if rooms.is_empty() {
                objects[PLAYER].set_pos(new_x, new_y);
            } else {
                // connect each room to the previous room after the first
                let (prev_x, prev_y) = rooms[rooms.len() - 1].centre();
                // connect the rooms with tunnels
                if rand::random() {
                    create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                    create_v_tunnel(prev_y, new_y, new_x, &mut map);
                } else {
                    // first move vertically, then horizontally
                    create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                    create_h_tunnel(prev_x, new_x, new_y, &mut map);
                }
            }
            rooms.push(new_room);
        }
    }
    map
}

fn place_objects(room: Rect, map: &Map, objects: &mut Vec<Object>) {
    // randomize the number of mosters to create
    let num_monsters = rand::thread_rng().gen_range(0, MAX_ROOM_MONSTERS + 1);

    for _ in 0..num_monsters {
        // randomise location in room
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        if !is_blocked(x, y, map, objects) {
            let mut monster = if rand::random::<f32>() < 0.8 {
                // create grunt, 80%
                let mut grunt = Object::new(x, y, 'G', ORANGE, "Grunt", true);
                grunt.fighter = Some(Fighter {
                    max_hp: 10,
                    hp: 10,
                    defense: 0,
                    power: 3,
                    on_death: DeathCallback::Enemy,
                });
                grunt.ai = Some(Ai::Basic);
                grunt
            } else {
                // create elite
                let mut elite = Object::new(x, y, 'E', AZURE, "Arbiter", true);
                elite.fighter = Some(Fighter {
                    max_hp: 16,
                    hp: 16,
                    defense: 1,
                    power: 4,
                    on_death: DeathCallback::Enemy,
                });
                elite.ai = Some(Ai::Basic);
                elite
            };
            monster.alive = true;
            objects.push(monster);
        }
    }
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    if map[x as usize][y as usize].blocked {
        return true;
    }

    // return true if there is any object on the pos that is blocked
    objects
        .iter()
        .any(|object| object.blocks && object.pos() == (x, y))
}

fn render_bar(
    panel: &mut Offscreen,
    x: i32,
    y: i32,
    total_width: i32,
    name: &str,
    value: i32,
    maximum: i32,
    bar_color: Color,
    back_color: Color,
) {
    // calculation for the width of the bar
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;

    // render in the background
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    // render the bar with the calculated width on top
    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    // add some descriptive text
    panel.set_default_foreground(WHITE);
    panel.print_ex(
        x + total_width / 2,
        y,
        BackgroundFlag::None,
        TextAlignment::Center,
        &format!("{}: {}/{}", name, value, maximum),
    );
}

// for rendering all objects to the screen
fn render_all(tcod: &mut Tcod, game: &mut Game, objects: &[Object], fov_recompute: bool) {
    // recompute if changes are made to the fov
    if fov_recompute {
        let player = &objects[PLAYER];
        tcod.fov
            .compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
    }

    // go through tiles and set color
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            // check if coords are in x and y
            let visible = tcod.fov.is_in_fov(x, y);
            // see if sight is blocked ie wall
            let wall = game.map[x as usize][y as usize].block_sight;
            let color = match (visible, wall) {
                (false, true) => COLOR_DARK_WALL,
                (false, false) => COLOR_DARK_GROUND,
                (true, true) => COLOR_LIGHT_WALL,
                (true, false) => COLOR_LIGHT_GROUND,
            };

            let explored = &mut game.map[x as usize][y as usize].explored;
            if visible {
                *explored = true;
            }

            // only render the tiles if they have been explored
            if *explored {
                tcod.con
                    .set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }
    }

    let mut to_draw: Vec<_> = objects
        .iter()
        .filter(|o| tcod.fov.is_in_fov(o.x, o.y))
        .collect();

    to_draw.sort_by(|o1, o2| o1.blocks.cmp(&o2.blocks));
    for object in &to_draw {
        object.draw(&mut tcod.con);
    }

    blit(
        &tcod.con,
        (0, 0),
        (MAP_WIDTH, MAP_HEIGHT),
        &mut tcod.root,
        (0, 0),
        1.0,
        1.0,
    );

    // show the players state
    tcod.root.set_default_foreground(WHITE);
    if let Some(fighter) = objects[PLAYER].fighter {
        tcod.root.print_ex(
            1,
            SCREEN_HEIGHT - 2,
            BackgroundFlag::None,
            TextAlignment::Left,
            format!("HP: {}/{}", fighter.hp, fighter.max_hp),
        );
    }

    // rendering for GUI
    tcod.panel.set_default_background(BLACK);
    tcod.panel.clear();

    // print game messages
    let mut y = MSG_HEIGHT as i32;
    for &(ref msg, color) in game.messages.iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
    }

    let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].fighter.map_or(0, |f| f.max_hp);

    // player stats
    render_bar(
        &mut tcod.panel,
        1,
        1,
        BAR_WIDTH,
        "HP",
        hp,
        max_hp,
        DARKER_AMBER,
        Color { r: 51, g: 51, b: 0 },
    );

    // display the names of objects under the mouse
    tcod.panel.set_default_foreground(LIGHT_GREY);
    tcod.panel.print_ex(
        1,
        0,
        BackgroundFlag::None,
        TextAlignment::Left,
        get_names_under_mouse(tcod.mouse, objects, &tcod.fov),
    );

    blit(
        &tcod.panel,
        (0, 0),
        (SCREEN_WIDTH, PANEL_HEIGHT),
        &mut tcod.root,
        (0, PANEL_Y),
        1.0,
        1.0,
    );
}

// handle all of the keyboard input
fn handle_keys(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) -> PlayerAction {
    use tcod::input::KeyCode::*;
    use PlayerAction::*;

    let player_alive = objects[PLAYER].alive;
    match (tcod.key, tcod.key.text(), player_alive) {
        // toggle full
        (
            Key {
                code: Enter,
                alt: true,
                ..
            },
            _,
            _,
        ) => {
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
            DidntTakeTurn
        }
        // exit game
        (Key { code: Escape, .. }, _, _) => Exit,
        // top left is 0,0
        (Key { code: Up, .. }, _, true) => {
            player_move_or_attack(0, -1, game, objects);
            TookTurn
        }
        (Key { code: Down, .. }, _, true) => {
            player_move_or_attack(0, 1, game, objects);
            TookTurn
        }
        (Key { code: Left, .. }, _, true) => {
            player_move_or_attack(-1, 0, game, objects);
            TookTurn
        }
        (Key { code: Right, .. }, _, true) => {
            player_move_or_attack(1, 0, game, objects);
            TookTurn
        }
        _ => DidntTakeTurn,
    }
}

// move the object
fn move_by(id: usize, dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let (x, y) = objects[id].pos();
    if !is_blocked(x + dx, y + dy, map, objects) {
        objects[id].set_pos(x + dx, y + dy);
    }
}

fn player_move_or_attack(dx: i32, dy: i32, game: &mut Game, objects: &mut [Object]) {
    // coords to where player wants to move or perform action
    let x = objects[PLAYER].x + dx;
    let y = objects[PLAYER].y + dy;

    // try to find and attackable object at the spot
    let target_id = objects
        .iter()
        .position(|object| object.fighter.is_some() && object.pos() == (x, y));

    // attack if there is an attackable object, else move
    match target_id {
        Some(target_id) => {
            let (player, target) = mut_two(PLAYER, target_id, objects);
            player.attack(target, game);
        }
        None => {
            move_by(PLAYER, dx, dy, &game.map, objects);
        }
    }
}

fn move_towards(id: usize, target_x: i32, target_y: i32, map: &Map, objects: &mut [Object]) {
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    // normalize to length 1 then convert to integer so movement is one grid
    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_by(id, dx, dy, map, objects);
}

fn ai_take_turn(enemy_id: usize, tcod: &Tcod, game: &mut Game, objects: &mut [Object]) {
    // let an enemy take its turn, if it is in fov
    let (enemy_x, enemy_y) = objects[enemy_id].pos();
    if tcod.fov.is_in_fov(enemy_x, enemy_y) {
        if objects[enemy_id].distance_to(&objects[PLAYER]) >= 2.0 {
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(enemy_id, player_x, player_y, &game.map, objects);
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            let (enemy, player) = mut_two(enemy_id, PLAYER, objects);
            enemy.attack(player, game);
        }
    }
}

// this allows two separate elements of a given slice to be mut borrowed
fn mut_two<T>(first_index: usize, second_index: usize, items: &mut [T]) -> (&mut T, &mut T) {
    // panic if both index equal, which would never happen
    assert!(first_index != second_index);
    let split_at_index = cmp::max(first_index, second_index);
    // split_at_mut splits a vector at the given index
    let (first_slice, second_slice) = items.split_at_mut(split_at_index);
    if first_index < second_index {
        (&mut first_slice[first_index], &mut second_slice[0])
    } else {
        (&mut second_slice[0], &mut first_slice[second_index])
    }
}

fn player_death(player: &mut Object, game: &mut Game) {
    game.messages.add("You died.", RED);

    player.char = '%';
    player.color = DARK_RED;
}

fn enemy_death(enemy: &mut Object, game: &mut Game) {
    game.messages
        .add(format!("{} is dead.", enemy.name), YELLOW);
    enemy.char = '%';
    enemy.color = DARK_RED;
    enemy.blocks = false;
    enemy.fighter = None;
    enemy.ai = None;
    enemy.name = format!("Remains of {}", enemy.name);
}

// return names of objects under mouse
fn get_names_under_mouse(mouse: Mouse, objects: &[Object], fov_map: &FovMap) -> String {
    let (x, y) = (mouse.cx as i32, mouse.cy as i32);

    let names = objects
        .iter()
        .filter(|obj| obj.pos() == (x, y) && fov_map.is_in_fov(obj.x, obj.y))
        .map(|obj| obj.name.clone())
        .collect::<Vec<_>>();

    names.join(", ")
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ai {
    Basic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeathCallback {
    Player,
    Enemy,
}

impl DeathCallback {
    fn callback(self, object: &mut Object, game: &mut Game) {
        use DeathCallback::*;
        let callback = match self {
            Player => player_death,
            Enemy => enemy_death,
        };
        callback(object, game);
    }
}

fn main() {
    // create window with init of custom vals
    let root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Buck")
        .init();

    let mut tcod = Tcod {
        root,
        con: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        key: Default::default(),
        mouse: Default::default(),
    };

    tcod::system::set_fps(LIMIT_FPS);

    let mut player = Object::new(0, 0, 'B', BLACK, "Buck", true);
    player.alive = true;
    player.fighter = Some(Fighter {
        max_hp: 30,
        hp: 30,
        defense: 2,
        power: 5,
        on_death: DeathCallback::Player,
    });

    let mut objects = vec![player];

    let mut game = Game {
        map: make_map(&mut objects), // player is objects[PLAYER]
        messages: Messages::new(),
    };

    // set the fov according to genrated map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            tcod.fov.set(
                x,
                y,
                !game.map[x as usize][y as usize].block_sight,
                !game.map[x as usize][y as usize].blocked,
            );
        }
    }

    game.messages.add(
        "Explore the depths beneath the streets of New Mombasa.",
        WHITE,
    );

    let mut previous_player_position = (-1, -1);
    // keep the window open
    while !tcod.root.window_closed() {
        tcod.con.clear();

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => tcod.mouse = m,
            Some((_, Event::Key(k))) => tcod.key = k,
            _ => tcod.key = Default::default(),
        }

        // render all of the objects and the game map
        // render the screen
        let fov_recompute = previous_player_position != (objects[PLAYER].pos());
        render_all(&mut tcod, &mut game, &objects, fov_recompute);

        // Flush draws everything to the screen
        tcod.root.flush();

        previous_player_position = objects[PLAYER].pos();
        let player_action = handle_keys(&mut tcod, &mut game, &mut objects);
        if player_action == PlayerAction::Exit {
            break;
        }

        // let enemies attack
        if objects[PLAYER].alive && player_action != PlayerAction::DidntTakeTurn {
            for id in 0..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, &tcod, &mut game, &mut objects);
                }
            }
        }
    }
}
