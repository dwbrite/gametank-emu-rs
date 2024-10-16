use std::ops::{Deref, DerefMut};
use crate::cartridges::Cartridge;

pub struct Cartridge2M {
    data: [u8; 0x200000],
    slice: [u8; 0x8000],
    current_page: u8,
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
        let mut data = [0u8; 0x200000];
        data.copy_from_slice(&slice);
        
        let mut slice = [0u8; 0x8000];
        slice[0..0x8000].copy_from_slice(&data[(0x200000-0x8000)..(0x200000)]);
        Self {
            data,
            slice,
            current_page: 127,
        }
    }
}
