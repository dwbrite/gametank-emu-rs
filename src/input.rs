use KeyState::{Held, JustPressed, JustReleased, Released};

#[derive(Copy, Clone)]
#[derive(Eq, Hash, PartialEq)]
pub enum ControllerButton {
    Up,
    Down,
    Left,
    Right,
    B,
    A,
    Start,
    C,
}

#[derive(Copy, Clone)]
#[derive(Eq, Hash, PartialEq)]
pub enum InputCommand {
    Controller1(ControllerButton),
    Controller2(ControllerButton),
    PlayPause,
    SoftReset,
    HardReset,
}

#[derive(Copy, Clone)]
#[derive(Eq, Hash, PartialEq)]
pub enum KeyState {
    JustPressed,
    Held,
    JustReleased,
    Released
}

impl KeyState {
    pub fn is_pressed(&self) -> bool {
        match self {
            JustPressed => { true }
            Held => { true }
            JustReleased => { false }
            Released => { false }
        }
    }

    pub fn new(pressed: bool) -> Self {
        if pressed {
            return JustPressed
        }
        Released
    }
    
    pub fn update_state(&self, pressed: bool) -> Self {
        if pressed {
            return match self {
                JustPressed => { JustPressed }
                Held => { Held }
                JustReleased => { JustPressed }
                Released => { JustPressed }
            }
        }
        match self {
            JustPressed => { JustReleased }
            Held => { JustReleased }
            JustReleased => { Released }
            Released => { Released }
        }
    } 

    pub fn update(&self) -> Self {
        match self {
            JustPressed => { Held }
            Held => { Held }
            JustReleased => { Released }
            Released => { Released }
        }
    }
}