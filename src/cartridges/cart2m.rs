use std::ops::{Deref, DerefMut};
use crate::cartridges::Cartridge;


#[derive(Debug, Clone)]
pub struct Cartridge2M {
    _data: Box<[u8; 0x200000]>,
    slice: Box<[u8; 0x8000]>,
    _current_page: u8,
}

impl Deref for Cartridge2M {
    type Target = [u8; 0x8000];

    fn deref(&self) -> &Self::Target {
        &self.slice
    }
}

impl DerefMut for Cartridge2M {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.slice
    }
}


// TODO: VIA
impl Cartridge for Cartridge2M {
    fn from_slice(slice: &[u8]) -> Self {
        let mut _data = [0u8; 0x200000];
        _data.copy_from_slice(&slice);
        
        let mut slice = [0u8; 0x8000];
        slice[0..0x8000].copy_from_slice(&_data[(0x200000-0x8000)..(0x200000)]);
        Self {
            _data: Box::new(_data),
            slice: Box::new(slice),
            _current_page: 127,
        }
    }
}
