use std::ops::{Deref, DerefMut};
use crate::cartridges::Cartridge;

pub struct Cartridge32K {
    data: [u8; 0x8000]
}

impl Deref for Cartridge32K {
    type Target = [u8; 0x8000];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for Cartridge32K {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl Cartridge for Cartridge32K {
    fn from_slice(slice: &[u8]) -> Self {
        let mut data = [0; 0x8000];
        data[0x0000..0x8000].copy_from_slice(&slice);
        Self {
            data,
        }
    }
}
