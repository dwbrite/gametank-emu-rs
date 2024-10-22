#[derive(Debug, Default)]
pub struct GamePad {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub b: bool,
    pub a: bool,
    pub c: bool,
    pub start: bool,
    
    pub port_select: bool,
}